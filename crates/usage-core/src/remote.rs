//! v1 manual snapshot storage. User archives are never treated as rebuildable
//! index files. Merge owns its input rows; cached publications are typed/immutable.
use crate::{
    Error, PriceSchedule, Usage,
    index::IndexedSession,
    inventory::FileStamp,
    parser::SessionDetail,
    query::{self, SessionSummary},
    timestamp, usage,
};
use chrono_tz::Tz;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
pub const SCHEMA: &str = "cousash.remote-snapshot";
#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub device_short_code: String,
    pub label: String,
    pub platform: String,
    pub hostname: String,
    pub session_count: usize,
    pub usage: Usage,
    pub imported_at: String,
    pub exported_at: String,
    pub generated_at: String,
}
#[derive(Clone)]
pub struct Remote {
    pub metadata: Metadata,
    pub sessions: Vec<IndexedSession>,
}
pub struct Store {
    root: PathBuf,
    pub current_device_code: String,
    cache: BTreeMap<PathBuf, (FileStamp, Arc<Remote>)>,
    zone: Tz,
    dirty: bool,
}
fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").into()
}
fn fallback<'a>(values: &[&'a Value]) -> &'a Value {
    values
        .iter()
        .copied()
        .find(|v| !v.is_null() && v.as_str() != Some(""))
        .unwrap_or(&Value::Null)
}
pub fn safe_code(value: &str) -> String {
    regex::Regex::new("[^a-z0-9_-]+")
        .unwrap()
        .replace_all(&value.trim().to_lowercase(), "-")
        .trim_matches('-')
        .chars()
        .take(80)
        .collect()
}
fn invalid(message: &str) -> Error {
    Error::InvalidData(message.into())
}
fn read(path: &Path) -> Result<Value, Error> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes),
    )?)
}
fn write(path: &Path, value: &Value) -> Result<(), Error> {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
/// Old snapshots can omit fields introduced by later parsers. Defaults are
/// applied once at the archive boundary, not repeatedly during queries.
pub fn typed<T: Default + Serialize + DeserializeOwned + 'static>(
    mut value: Value,
) -> Result<T, Error> {
    use std::{
        any::TypeId,
        sync::{LazyLock, RwLock},
    };
    static DEFAULTS: LazyLock<RwLock<HashMap<TypeId, Arc<Value>>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));
    let key = TypeId::of::<T>();
    let cached = { DEFAULTS.read().unwrap().get(&key).cloned() };
    let defaults = if let Some(value) = cached {
        value
    } else {
        let value = Arc::new(serde_json::to_value(T::default())?);
        DEFAULTS.write().unwrap().insert(key, value.clone());
        value
    };
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("session must be an object"))?;
    // Missing fields use serde defaults. Explicit nulls in old optional metadata
    // need normalization at this boundary; wire shapes retain their v1 form.
    for (key, value) in object {
        if value.is_null()
            && let Some(default) = defaults.get(key)
            && !default.is_null()
        {
            *value = default.clone();
        }
    }
    Ok(serde_json::from_value(value)?)
}
fn summary(value: &Value) -> Result<SessionSummary, Error> {
    typed(value.clone())
}
fn validate(value: &Value) -> Result<String, Error> {
    if !value.is_object() {
        return Err(invalid("snapshot must be a JSON object"));
    }
    if value["schema"] != SCHEMA {
        return Err(invalid("unsupported snapshot schema"));
    }
    let version = &value["version"];
    let version = if version.is_null() || version == "" {
        0
    } else if let Some(n) = version.as_i64() {
        n
    } else if let Some(n) = version.as_f64() {
        n as i64
    } else if let Some(n) = version.as_bool() {
        i64::from(n)
    } else {
        version
            .as_str()
            .unwrap_or("")
            .parse()
            .map_err(|_| invalid("snapshot version is invalid"))?
    };
    if version > 1 {
        return Err(invalid("snapshot version is newer than this dashboard"));
    }
    if !value["device"].is_object() || !value["snapshot"].is_object() {
        return Err(invalid("snapshot is missing device or data"));
    }
    let code = safe_code(value["device"]["short_code"].as_str().unwrap_or(""));
    if code.is_empty() {
        return Err(invalid("snapshot is missing device short code"));
    }
    if !value["snapshot"]["sessions"].is_array() || !value["snapshot"]["details_by_uid"].is_object()
    {
        return Err(invalid("snapshot is missing session data"));
    }
    Ok(code)
}
fn metadata(value: &Value) -> Result<Metadata, Error> {
    let code = validate(value)?;
    let device = &value["device"];
    let snapshot = &value["snapshot"];
    Ok(Metadata {
        label: device["label"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&code)
            .into(),
        device_short_code: code,
        platform: text(&device["platform"]),
        hostname: text(&device["hostname"]),
        session_count: snapshot["sessions"].as_array().unwrap().len(),
        usage: serde_json::from_value(snapshot["summary"]["usage"].clone())?,
        imported_at: text(&value["imported_at"]),
        exported_at: text(&value["exported_at"]),
        generated_at: text(&snapshot["generated_at"]),
    })
}
fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
// Python v1 uses sorted keys and the default comma/colon spacing for the
// fallback identity of very old rows without session_id, uid or path.
fn identity_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!(
                        "{}: {}",
                        serde_json::to_string(k).unwrap(),
                        identity_json(&map[k])
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Value::Array(rows) => format!(
            "[{}]",
            rows.iter()
                .map(identity_json)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Number(n) if n.is_f64() => {
            let text = format!("{:?}", n.as_f64().unwrap());
            if let Some((base, exp)) = text.split_once('e') {
                let exponent: i32 = exp.parse().unwrap();
                format!("{base}e{exponent:+03}")
            } else {
                text
            }
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}
fn key(row: &Value) -> String {
    for key in ["session_id", "uid", "path"] {
        if let Some(value) = row[key].as_str().filter(|s| !s.is_empty()) {
            return value.into();
        }
    }
    format!("{:x}", Sha1::digest(identity_json(row).as_bytes()))
}
fn merge(mut existing: Value, mut incoming: Value, zone: Tz) -> Result<Value, Error> {
    let mut rows = Vec::<Value>::new();
    let mut indices = HashMap::new();
    let mut details = Map::new();
    for source in [&mut existing, &mut incoming] {
        let mut source_details = object(source["details_by_uid"].take());
        if let Some(sessions) = source["sessions"].take().as_array_mut() {
            for session in sessions {
                if !session.is_object() {
                    continue;
                }
                let session = session.take();
                let identity = key(&session);
                let index = indices.get(&identity).copied().unwrap_or(rows.len());
                if index < rows.len() {
                    if let Some(uid) = rows[index]["uid"].as_str() {
                        details.remove(uid);
                    }
                    rows[index] = session;
                } else {
                    indices.insert(identity, index);
                    rows.push(session);
                }
                if let Some(uid) = rows[index]["uid"].as_str()
                    && let Some(detail) = source_details.remove(uid).filter(Value::is_object)
                {
                    details.insert(uid.into(), detail);
                }
            }
        }
    }
    rows.sort_by_cached_key(|row| {
        std::cmp::Reverse(text(fallback(&[
            &row["end_at"],
            &row["updated_at"],
            &row["start_at"],
        ])))
    });
    let summaries = rows.iter().map(summary).collect::<Result<Vec<_>, _>>()?;
    let totals = query::summarize(&summaries)?;
    let typed_details = details
        .values()
        .map(|d| typed::<SessionDetail>(d.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let daily = query::daily_usage(&typed_details, zone)?;
    Ok(
        json!({"generated_at":fallback(&[&incoming["generated_at"],&json!(timestamp::format(chrono::Utc::now()))]),"codex_home":text(fallback(&[&incoming["codex_home"],&existing["codex_home"]])),"codex_sources":incoming.get("codex_sources").filter(|v|v.is_array()).cloned().unwrap_or(json!([])),"sessions":rows,"details_by_uid":details,"summary":totals,"daily_usage":daily}),
    )
}
impl Store {
    pub fn new(root: PathBuf, current_device_code: String, zone: Tz) -> Self {
        Self {
            root,
            current_device_code,
            cache: BTreeMap::new(),
            zone,
            dirty: false,
        }
    }
    fn path(&self, code: &str) -> Result<PathBuf, Error> {
        let code = safe_code(code);
        if code.is_empty() {
            return Err(invalid("missing device short code"));
        }
        Ok(self.root.join(format!("{code}.json")))
    }
    pub fn import(
        &mut self,
        mut incoming: Value,
        label: Option<&str>,
        allow_current: bool,
    ) -> Result<Value, Error> {
        let code = validate(&incoming)?;
        if code == self.current_device_code && !allow_current {
            return Ok(
                json!({"ok":false,"needs_confirmation":true,"reason":"current_device","device_short_code":code,"suggested_label":fallback(&[&incoming["device"]["label"],&json!(code)])}),
            );
        }
        let path = self.path(&code)?;
        let existing = read(&path).ok();
        let valid = existing.as_ref().filter(|e| validate(e).is_ok());
        let old_label = valid
            .map(|v| text(&v["device"]["label"]))
            .unwrap_or_default();
        let candidate = if !old_label.is_empty() {
            old_label
        } else {
            text(fallback(&[&incoming["device"]["label"], &json!(code)]))
        };
        let new_label =
            crate::parser::clean(label.filter(|s| !s.is_empty()).unwrap_or(&candidate), 120);
        if existing.is_none() && label.is_none_or(str::is_empty) {
            return Ok(
                json!({"ok":false,"needs_label":true,"device_short_code":code,"suggested_label":new_label,"platform":text(&incoming["device"]["platform"]),"hostname":text(&incoming["device"]["hostname"])}),
            );
        }
        let now = timestamp::format(chrono::Utc::now());
        let exported = fallback(&[
            &incoming["exported_at"],
            &incoming["snapshot"]["generated_at"],
            &json!(now),
        ])
        .clone();
        let created = existing.is_none();
        let old = existing
            .filter(|e| validate(e).is_ok())
            .map(|mut v| v["snapshot"].take())
            .unwrap_or(json!({}));
        let snapshot = merge(old, incoming["snapshot"].take(), self.zone)?;
        let mut device = incoming["device"].take();
        device["short_code"] = json!(code);
        device["label"] = json!(new_label);
        let stored = json!({"schema":SCHEMA,"version":1,"device":device,"exported_at":exported,"imported_at":now,"snapshot":snapshot});
        write(&path, &stored)?;
        self.cache.remove(&path);
        self.dirty = true;
        Ok(json!({"ok":true,"created":created,"remote":metadata(&stored)?}))
    }
    pub fn rename(&mut self, code: &str, label: &str) -> Result<Value, Error> {
        let path = self.path(code)?;
        let mut payload = read(&path)?;
        let code = validate(&payload)?;
        let clean = crate::parser::clean(label, 120);
        payload["device"]["label"] = json!(if clean.is_empty() { code } else { clean });
        write(&path, &payload)?;
        self.cache.remove(&path);
        self.dirty = true;
        Ok(json!({"ok":true,"remote":metadata(&payload)?}))
    }
    pub fn delete(&mut self, code: &str) -> Result<(), Error> {
        let path = self.path(code)?;
        fs::remove_file(&path)?;
        self.cache.remove(&path);
        self.dirty = true;
        Ok(())
    }
    pub fn refresh(&mut self, prices: &PriceSchedule) -> Result<(bool, Vec<Arc<Remote>>), Error> {
        let mut paths = Vec::new();
        match fs::read_dir(&self.root) {
            Ok(entries) => {
                for entry in entries {
                    let path = entry?.path();
                    if path.extension().is_some_and(|s| s == "json") {
                        paths.push(path);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        paths.sort();
        let old = self.cache.len();
        self.cache.retain(|p, _| paths.contains(p));
        let mut changed = self.dirty || old != self.cache.len();
        let mut result = Vec::new();
        for path in paths {
            let stamp = FileStamp::read(&path)?;
            if let Some((old, remote)) = self.cache.get(&path)
                && *old == stamp
            {
                result.push(remote.clone());
                continue;
            }
            let validated =
                read(&path).and_then(|payload| metadata(&payload).map(|meta| (payload, meta)));
            let Ok((payload, meta)) = validated else {
                changed |= self.cache.remove(&path).is_some();
                continue;
            };
            let transformed = transform(payload, meta, prices)?;
            let remote = Arc::new(transformed);
            self.cache.insert(path, (stamp, remote.clone()));
            result.push(remote);
            changed = true;
        }
        self.dirty = false;
        Ok((changed, result))
    }
}
fn transform(
    mut payload: Value,
    metadata: Metadata,
    prices: &PriceSchedule,
) -> Result<Remote, Error> {
    let code = &metadata.device_short_code;
    let label = &metadata.label;
    let mut sessions = Vec::new();
    let absent_detail = Arc::new(SessionDetail::default());
    let absent_events = Arc::new(Vec::new());
    let summary_schema = crate::wire::summary_schema();
    let detail_schema = crate::wire::detail_schema();
    let mut details = object(payload["snapshot"]["details_by_uid"].take());
    for row in payload["snapshot"]["sessions"]
        .take()
        .as_array_mut()
        .ok_or_else(|| invalid("missing sessions"))?
    {
        if !row.is_object() {
            continue;
        }
        let raw_uid = text(fallback(&[&row["uid"], &row["session_id"], &row["path"]]));
        let uid = format!("{:x}", Sha1::digest(format!("{code}:{raw_uid}")))[..16].to_owned();
        let raw_detail = row["uid"]
            .as_str()
            .and_then(|id| details.remove(id))
            .filter(Value::is_object);
        let has_detail = raw_detail.is_some();
        let mut wire = crate::wire::RemoteWire {
            summary: crate::wire::Shape::capture(row, &summary_schema, true),
            detail: raw_detail
                .as_ref()
                .map(|detail| crate::wire::Shape::capture(detail, &detail_schema, false))
                .unwrap_or_default(),
        };
        let stored_cost = row["estimated_cost_breakdown_usd"].clone();
        let transformed = [
            "uid",
            "environment",
            "environment_id",
            "is_remote",
            "remote_device_short_code",
            "remote_imported_at",
            "remote_exported_at",
            "codex_home",
        ];
        wire.summary.include(&transformed);
        wire.detail.include(&transformed);
        wire.detail.include(&[
            "estimated_cost_usd",
            "estimated_cost_breakdown_usd",
            "price_model_known",
        ]);
        if raw_detail
            .as_ref()
            .is_none_or(|d| d.get("applied_price_segments").is_none())
        {
            wire.detail.include(&["applied_price_segments"]);
        }

        let mut summary: SessionSummary = typed(row.take())?;

        summary.uid = uid.clone();
        summary.environment = label.clone();
        summary.environment_id = format!("remote-{code}");
        summary.is_remote = true;
        summary.remote_device_short_code = code.clone();
        summary.remote_imported_at = metadata.imported_at.clone();
        summary.remote_exported_at = metadata.exported_at.clone();
        summary.codex_home = format!("remote:{code}");
        let Some(raw_detail) = raw_detail else {
            sessions.push(IndexedSession {
                detail: absent_detail.clone(),
                events: absent_events.clone(),
                has_detail: false,
                summary: Some(Arc::new(summary)),
                wire: Some(Arc::new(wire)),
            });
            continue;
        };
        let mut detail: SessionDetail = typed(raw_detail)?;
        detail.identity.uid = uid;
        detail.environment = summary.environment.clone();
        detail.environment_id = summary.environment_id.clone();
        detail.is_remote = true;
        detail.remote_device_short_code = code.clone();
        detail.remote_imported_at = metadata.imported_at.clone();
        detail.remote_exported_at = metadata.exported_at.clone();
        detail.codex_home = summary.codex_home.clone();
        let rows: Vec<_> = detail
            .timeline
            .iter()
            .filter(|e| e.total_token_usage.has_tokens())
            .collect();
        if !rows.is_empty() && rows.iter().all(|e| !e.model.is_empty()) {
            let model = if detail.model.is_empty() {
                &summary.model
            } else {
                &detail.model
            };
            let tier = if detail.service_tier.is_empty() {
                &summary.service_tier
            } else {
                &detail.service_tier
            };
            let time = if detail.end_at.is_empty() {
                &summary.end_at
            } else {
                &detail.end_at
            };
            detail.pricing = prices.price_events(
                &usage::normalize(&detail.timeline)?,
                model,
                detail.total_token_usage,
                Some(time),
                tier,
            )?;
            wire.summary.include(&[
                "estimated_cost_usd",
                "estimated_cost_breakdown_usd",
                "price_model_known",
            ]);
            wire.detail.include(&[
                "estimated_cost_usd",
                "estimated_cost_breakdown_usd",
                "price_model_known",
                "applied_price_segments",
            ]);
            summary.estimated_cost_usd = detail.pricing.estimated_cost_usd;
            summary.estimated_cost_breakdown_usd =
                detail.pricing.estimated_cost_breakdown_usd.clone();
            summary.price_model_known = detail.pricing.price_model_known;
        } else {
            wire.detail
                .replace("estimated_cost_breakdown_usd", stored_cost);
            detail.pricing.estimated_cost_usd = summary.estimated_cost_usd;
            detail.pricing.estimated_cost_breakdown_usd =
                summary.estimated_cost_breakdown_usd.clone();
            detail.pricing.price_model_known = summary.price_model_known;
        }
        let mut indexed = IndexedSession::from_detail(Arc::new(detail))?;
        indexed.wire = Some(Arc::new(wire));
        indexed.summary = Some(Arc::new(summary));
        indexed.has_detail = has_detail;
        sessions.push(indexed);
    }
    Ok(Remote { metadata, sessions })
}

#[cfg(test)]
mod identity_tests {
    #[test]
    fn anonymous_v1_rows_use_stable_python_json_identity() {
        let value = serde_json::json!({"z":[1,1e-7,1e16],"a":"合成"});
        assert_eq!(
            super::identity_json(&value),
            r#"{"a": "合成", "z": [1, 1e-07, 1e+16]}"#
        );
    }
}
