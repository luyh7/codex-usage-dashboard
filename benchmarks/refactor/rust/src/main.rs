//! Measurement prototype, not a replacement for the production analyzer.
//! Each operation's supported contract is recorded in the benchmark report.
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use regex::bytes::Regex;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value as V, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

const KEYS: [&str; 6] = [
    "input_tokens",
    "cached_input_tokens",
    "cache_write_tokens",
    "output_tokens",
    "reasoning_output_tokens",
    "total_tokens",
];
fn s(v: &V) -> &str {
    v.as_str().unwrap_or("")
}
fn n(v: &V) -> i64 {
    v.as_i64()
        .or_else(|| v.as_f64().map(|x| x as i64))
        .or_else(|| v.as_bool().map(i64::from))
        .unwrap_or(0)
}
fn arr(v: &V) -> &[V] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn usage(v: &V) -> [i64; 6] {
    std::array::from_fn(|i| {
        n(if i == 2 && v.get(KEYS[i]).is_none() {
            &v["cache_write_input_tokens"]
        } else {
            &v[KEYS[i]]
        })
    })
}
fn uj(a: [i64; 6]) -> V {
    V::Object(
        KEYS.iter()
            .zip(a)
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect(),
    )
}
fn add(a: [i64; 6], b: [i64; 6]) -> [i64; 6] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: [i64; 6], b: [i64; 6]) -> [i64; 6] {
    std::array::from_fn(|i| (a[i] - b[i]).max(0))
}
fn delta(a: [i64; 6], b: [i64; 6]) -> [i64; 6] {
    if (a[5] != 0 || b[5] != 0) && a[5] < b[5]
        || a[5] == 0 && b[5] == 0 && [0, 1, 2, 3, 5].iter().any(|&i| a[i] < b[i])
    {
        a
    } else {
        sub(a, b)
    }
}
fn stamp(v: &V) -> Option<DateTime<Utc>> {
    let t = s(v);
    DateTime::parse_from_rfc3339(t)
        .map(|x| x.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|x| Utc.from_utc_datetime(&x))
        })
}
fn deltas(v: &V) -> Vec<(V, [i64; 6])> {
    let mut rows: Vec<_> = arr(v)
        .iter()
        .filter_map(|r| stamp(&r["timestamp"]).map(|t| (r, t)))
        .collect();
    rows.sort_by_key(|r| r.1);
    let mut prev = [0; 6];
    rows.into_iter()
        .enumerate()
        .map(|(i, (row, _))| {
            let cur = usage(&row["total_token_usage"]);
            if i == 0 && row["last_token_usage"].is_object() {
                let last = usage(&row["last_token_usage"]);
                let base = sub(cur, last);
                if last.iter().any(|x| *x > 0) && add(base, last) == cur {
                    prev = base;
                }
            }
            let d = delta(cur, prev);
            prev = cur;
            (row.clone(), d)
        })
        .collect()
}
fn fingerprint(v: &V) -> ([i64; 6], [i64; 6]) {
    (
        usage(&v["total_token_usage"]),
        usage(&v["last_token_usage"]),
    )
}
fn fork(c: &V) -> V {
    let child: Vec<_> = arr(&c["child"]).iter().filter(|x| x.is_object()).collect();
    let parent: Vec<_> = arr(&c["parent"])
        .iter()
        .filter(|x| x.is_object())
        .map(fingerprint)
        .collect();
    let cf: Vec<_> = child.iter().map(|x| fingerprint(x)).collect();
    let mut best = 0;
    if !cf.is_empty() {
        for start in 0..parent.len() {
            let mut len = 0;
            while len < cf.len() && start + len < parent.len() && cf[len] == parent[start + len] {
                len += 1
            }
            best = best.max(len);
        }
    }
    let inherited = if best > 0 {
        usage(&child[best - 1]["total_token_usage"])
    } else {
        [0; 6]
    };
    let mut prev = inherited;
    let mut total = [0; 6];
    let mut last = [0; 6];
    let mut rows = vec![];
    for row in &child[best..] {
        let cur = usage(&row["total_token_usage"]);
        last = delta(cur, prev);
        prev = cur;
        total = add(total, last);
        let mut r = (*row).clone();
        r["total_token_usage"] = uj(total);
        r["last_token_usage"] = uj(last);
        rows.push(r);
    }
    json!({"prefix":best,"rebased":rows,"total":uj(total),"last":uj(last),"inherited":uj(inherited)})
}
fn round6(x: f64) -> f64 {
    (x * 1_000_000.).round_ties_even() / 1_000_000.
}
fn canonical(model: &str) -> String {
    let m = model.trim().to_lowercase();
    let m = m.rsplit('/').next().unwrap_or("").trim();
    match m {
        "gpt-daybreak-blue-latest" => "gpt-5.6-sol".into(),
        "gpt-daybreak-red-latest" => "gpt-5.6-cyber".into(),
        _ => m.into(),
    }
}
fn rate<'a>(m: &str, rates: &'a V) -> Option<(String, &'a V)> {
    if let Some(p) = rates.get(m) {
        return Some((m.into(), p));
    }
    let map = rates.as_object()?;
    let mut keys: Vec<_> = map.keys().collect();
    keys.sort_by_key(|x| std::cmp::Reverse(x.len()));
    for k in keys {
        if m.starts_with(&format!("{k}-")) {
            return Some((k.clone(), &map[k]));
        }
    }
    None
}
fn effective(model: &str, selected: &V, prices: &V, schedules: &V) -> V {
    let rows = arr(schedules);
    let idx = rows
        .iter()
        .position(|x| &x[0] == selected)
        .unwrap_or(rows.len() - 1);
    let mut e = selected.clone();
    for row in rows[..idx].iter().rev() {
        if rate(model, &row[1]).is_some_and(|(_, p)| p == prices) {
            e = row[0].clone();
        } else {
            break;
        }
    }
    e
}
fn price(c: &V, cfg: &V) -> V {
    let m = canonical(s(&c["model"]));
    let schedules = arr(&cfg["schedules"]);
    let moment = stamp(&c["timestamp"]);
    let selected = if let Some(t) = moment {
        schedules
            .iter()
            .rev()
            .find(|r| r[0].is_null() || stamp(&r[0]).is_some_and(|x| t >= x))
            .unwrap_or(&schedules[0])
    } else {
        schedules.last().unwrap()
    };
    let Some((mut model, p)) = rate(&m, &selected[1]) else {
        return V::Null;
    };
    let mut e = effective(&m, &selected[0], p, &cfg["schedules"]);
    let mut tier = V::Null;
    for family in arr(&cfg["families"]) {
        if family["short"].get(&model).is_some() {
            tier = json!("short");
            if !c["input_tokens"].is_null() && n(&c["input_tokens"]) > n(&family["threshold"]) {
                let long = if family["schedules"].is_array() {
                    arr(&family["schedules"])
                        .iter()
                        .rev()
                        .find(|r| selected[0].is_null() || stamp(&selected[0]) >= stamp(&r[0]))
                        .unwrap_or(&arr(&family["schedules"])[0])[1]
                        .clone()
                } else {
                    family["long"].clone()
                };
                if let Some((lm, lp)) = rate(&m, &long) {
                    model = lm;
                    tier = json!("long");
                    if family["schedules"].is_array() {
                        e = effective(&m, &selected[0], lp, &family["schedules"]);
                    }
                    // The owned long-context prices are returned through the shared helper.
                    return finish_price(c, cfg, &model, &e, &tier, lp);
                }
            }
            break;
        }
    }
    finish_price(c, cfg, &model, &e, &tier, p)
}
fn finish_price(c: &V, cfg: &V, model: &str, e: &V, tier: &V, p: &V) -> V {
    let service = s(&c["service_tier"]).trim().to_lowercase();
    let mult = match service.as_str() {
        "fast" | "priority" => cfg["fast"][model].as_f64(),
        "ultrafast" => cfg["ultrafast"][model].as_f64(),
        _ => Some(1.),
    };
    let Some(mult) = mult else { return V::Null };
    let prices: Map<_, _> = p
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), json!(v.as_f64().unwrap() * mult)))
        .collect();
    json!({"model":model,"effective_at":e,"context_tier":tier,"service_tier":service,"cost_multiplier":mult,"prices":prices})
}
fn parts(u: &V, p: &V) -> V {
    let u = usage(u);
    let input = u[0].max(0);
    let cached = u[1].max(0).min(input);
    let output = u[3].max(0);
    let reason = u[4].max(0).min(output);
    json!({"input_tokens":(input-cached) as f64*p["input"].as_f64().unwrap()/1e6,"cached_input_tokens":cached as f64*p["cached_input"].as_f64().unwrap()/1e6,"cache_write_tokens":0.0,"output_tokens":output as f64*p["output"].as_f64().unwrap()/1e6,"reasoning_output_tokens":reason as f64*p["output"].as_f64().unwrap()/1e6})
}
fn group_add(map: &mut Map<String, V>, key: String, init: V, u: [i64; 6]) {
    let v = map.entry(key).or_insert(init);
    v["sessions"] = json!(n(&v["sessions"]) + 1);
    v["usage"] = uj(add(usage(&v["usage"]), u));
}
fn summary(rows: &[V]) -> V {
    let mut total = [0; 6];
    let mut active = 0;
    let mut archived = 0;
    let mut cost = 0.;
    let mut known = 0;
    let mut model = Map::new();
    let mut project = Map::new();
    let mut env = Map::new();
    let mut day = Map::new();
    for r in rows {
        let u = usage(&r["total_token_usage"]);
        total = add(total, u);
        if s(&r["source"]) == "active" {
            active += 1
        }
        if s(&r["source"]) == "archived" {
            archived += 1
        }
        if let Some(c) = r["estimated_cost_usd"].as_f64() {
            cost += c;
            known += 1;
        }
        let m = if s(&r["model"]).is_empty() {
            "unknown"
        } else {
            s(&r["model"])
        };
        group_add(
            &mut model,
            m.into(),
            json!({"model":m,"sessions":0,"usage":uj([0;6])}),
            u,
        );
        let pk = ["project_root", "project"]
            .iter()
            .map(|k| s(&r[k]))
            .find(|x| !x.is_empty())
            .unwrap_or("unknown");
        let pl = if s(&r["project"]).is_empty() {
            pk.rsplit(['/', '\\']).next().unwrap_or("unknown")
        } else {
            s(&r["project"])
        };
        group_add(
            &mut project,
            pk.into(),
            json!({"project":pl,"project_root":pk,"sessions":0,"usage":uj([0;6])}),
            u,
        );
        let ei = if s(&r["environment_id"]).is_empty() {
            "local"
        } else {
            s(&r["environment_id"])
        };
        let el = if s(&r["environment"]).is_empty() {
            ei
        } else {
            s(&r["environment"])
        };
        group_add(
            &mut env,
            ei.into(),
            json!({"id":ei,"label":el,"sessions":0,"usage":uj([0;6])}),
            u,
        );
        let d = ["end_at", "start_at"]
            .iter()
            .map(|k| s(&r[k]))
            .find(|x| !x.is_empty())
            .unwrap_or("unknown");
        let d = &d[..d.len().min(10)];
        group_add(
            &mut day,
            d.into(),
            json!({"date":d,"sessions":0,"usage":uj([0;6])}),
            u,
        );
    }
    let sorted = |m: Map<String, V>| {
        let mut a: Vec<_> = m.into_values().collect();
        a.sort_by_key(|r| std::cmp::Reverse(n(&r["usage"]["total_tokens"])));
        a
    };
    let mut projects = sorted(project);
    projects.truncate(20);
    let mut days: Vec<_> = day.into_values().collect();
    days.sort_by(|a, b| s(&a["date"]).cmp(s(&b["date"])));
    json!({"session_count":rows.len(),"active_count":active,"archived_count":archived,"usage":uj(total),"estimated_cost_usd":round6(cost),"estimated_cost_known_count":known,"by_model":sorted(model),"by_project":projects,"by_environment":sorted(env),"by_day":days,"top_session_uid":rows.first().map(|r|&r["uid"])})
}
fn daily(details: &[V]) -> V {
    let mut days = std::collections::BTreeMap::<String, [i64; 6]>::new();
    for d in details {
        for (r, u) in deltas(&d["timeline"]) {
            if u[5] <= 0 {
                continue;
            }
            let t = stamp(&r["timestamp"])
                .unwrap()
                .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                .format("%Y-%m-%d")
                .to_string();
            let entry = days.entry(t).or_insert([0; 6]);
            *entry = add(*entry, u);
        }
    }
    json!(
        days.into_iter()
            .map(|(date, u)| json!({"date":date,"usage":uj(u)}))
            .collect::<Vec<_>>()
    )
}
fn remote(c: &V) -> V {
    let mut sessions = Map::new();
    let mut details = Map::new();
    for snapshot in [&c["existing"], &c["incoming"]] {
        for r in arr(&snapshot["sessions"]) {
            let key = ["session_id", "uid", "path"]
                .iter()
                .map(|k| s(&r[k]))
                .find(|x| !x.is_empty())
                .expect("fixture identity");
            if let Some(old) = sessions.get(key) {
                details.shift_remove(s(&old["uid"]));
            }
            sessions.insert(key.into(), r.clone());
            let uid = s(&r["uid"]);
            if snapshot["details_by_uid"][uid].is_object() {
                details.insert(uid.into(), snapshot["details_by_uid"][uid].clone());
            }
        }
    }
    let mut rows: Vec<_> = sessions.into_values().collect();
    let time = |r: &V| {
        ["end_at", "updated_at", "start_at"]
            .iter()
            .map(|k| s(&r[k]).to_string())
            .find(|x| !x.is_empty())
            .unwrap_or_default()
    };
    rows.sort_by_key(|r| std::cmp::Reverse(time(r)));
    let d: Vec<_> = details.values().cloned().collect();
    json!({"generated_at":c["incoming"]["generated_at"],"codex_home":c["incoming"]["codex_home"],"codex_sources":c["incoming"]["codex_sources"],"summary":summary(&rows),"daily_usage":daily(&d),"sessions":rows,"details_by_uid":details})
}
fn read_rows(c: &V, re: &Regex) -> V {
    let mut f = fs::File::open(s(&c["path"])).unwrap();
    let start = n(&c["start"]) as u64;
    let end = c["end"].as_u64().unwrap_or(f.metadata().unwrap().len());
    f.seek(SeekFrom::Start(start)).unwrap();
    let mut reader = BufReader::with_capacity(
        c["buffer_bytes"].as_u64().unwrap_or(8192) as usize,
        f.take(end.saturating_sub(start)),
    );
    let mut raw = vec![];
    let mut first = start == 0;
    let mut consumed = 0;
    let mut skipped = 0;
    let mut skipped_bytes = 0;
    let mut counts = Map::new();
    let mut errors = 0;
    let mut totals = [0; 6];
    let mut rows = vec![];
    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw).unwrap() == 0 {
            break;
        }
        let complete = raw.ends_with(b"\n");
        let mut item = None;
        if raw.len() >= 4096 && raw.ends_with(b"}}\n") {
            if let Some(cap) = re.captures(&raw) {
                let outer = std::str::from_utf8(&cap[2]).unwrap();
                let inner = std::str::from_utf8(&cap[3]).unwrap();
                if outer == "response_item"
                    && [
                        "custom_tool_call_output",
                        "function_call_output",
                        "image_generation_call",
                        "reasoning",
                        "tool_search_output",
                    ]
                    .contains(&inner)
                    || outer == "event_msg" && inner == "image_generation_end"
                {
                    item = Some(
                        json!({"timestamp":std::str::from_utf8(&cap[1]).unwrap(),"type":outer,"payload":{"type":inner}}),
                    );
                    skipped += 1;
                    skipped_bytes += raw.len();
                }
            }
        }
        if item.is_none() {
            let bytes = if first && raw.starts_with(&[239, 187, 191]) {
                &raw[3..]
            } else {
                &raw[..]
            };
            let decoded = String::from_utf8_lossy(bytes);
            let line = decoded.trim();
            first = false;
            if line.is_empty() {
                if complete {
                    consumed += raw.len()
                }
                continue;
            }
            match serde_json::from_str::<V>(line) {
                Ok(v) => item = Some(v),
                Err(_) => {
                    if !complete {
                        break;
                    }
                    errors += 1;
                    item = Some(json!({"__parse_error__":true}));
                }
            }
        }
        first = false;
        consumed += raw.len();
        let v = item.unwrap();
        if !v.is_object() {
            continue;
        }
        let key = format!("{}/{}", s(&v["type"]), s(&v["payload"]["type"]));
        let old = n(counts.get(&key).unwrap_or(&V::Null));
        counts.insert(key, json!(old + 1));
        totals = add(totals, usage(&v["payload"]["info"]["total_token_usage"]));
        if c["return_rows"] == true {
            rows.push(v);
        }
    }
    json!({"bytes_read":consumed,"fast_skipped_line_count":skipped,"fast_skipped_bytes":skipped_bytes,"errors":errors,"counts":counts,"usage_checksum":uj(totals),"rows":rows})
}
fn inventory(path: &Path, rows: &mut Vec<V>) {
    for e in fs::read_dir(path).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if e.file_type().unwrap().is_dir() {
            inventory(&p, rows)
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            let m = e.metadata().unwrap();
            rows.push(json!([p.to_string_lossy(), m.len()]));
        }
    }
}
fn execute(c: &V, re: &Regex, db: Option<&Connection>, blob: &[u8]) -> V {
    match s(&c["op"]) {
        "reader" => read_rows(c, re),
        "timeline" => json!(
            deltas(&c["timeline"])
                .into_iter()
                .map(|(r, u)| json!([r, uj(u)]))
                .collect::<Vec<_>>()
        ),
        "fork" => json!(arr(&c["cases"]).iter().map(fork).collect::<Vec<_>>()),
        "pricing" => json!(
            arr(&c["cases"])
                .iter()
                .map(|q| price(q, &c["config"]))
                .collect::<Vec<_>>()
        ),
        "cost" => json!(
            arr(&c["cases"])
                .iter()
                .map(|q| parts(&q["usage"], &q["prices"]))
                .collect::<Vec<_>>()
        ),
        "summary" => summary(arr(&c["sessions"])),
        "daily" => daily(arr(&c["details"])),
        "remote" => remote(c),
        "encode" => {
            let raw = serde_json::to_vec(&c["payload"]).unwrap();
            let mut e = ZlibEncoder::new(Vec::new(), Compression::fast());
            e.write_all(&raw).unwrap();
            json!({"bytes":e.finish().unwrap().len()})
        }
        "decode" => {
            let mut raw = Vec::new();
            ZlibDecoder::new(blob).read_to_end(&mut raw).unwrap();
            serde_json::from_slice(&raw).unwrap()
        }
        "serialize" => json!({"bytes":serde_json::to_vec(&c["payload"]).unwrap().len()}),
        "inventory" => {
            let mut rows = vec![];
            inventory(Path::new(s(&c["root"])), &mut rows);
            rows.sort_by(|a, b| s(&a[0]).cmp(s(&b[0])));
            json!(rows)
        }
        "query" => {
            let conn = db.unwrap();
            let mut stmt = conn.prepare_cached(s(&c["sql"])).unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!([r.get::<_, String>(0)?, r.get::<_, i64>(1)?]))
                })
                .unwrap();
            json!(rows.map(Result::unwrap).collect::<Vec<_>>())
        }
        _ => panic!("unsupported op"),
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let c: V = serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
    let samples: usize = args.get(2).map(|x| x.parse().unwrap()).unwrap_or(9);
    let re=Regex::new(r#"^\{"timestamp":"([0-9T:.+Z-]+)","type":"(response_item|event_msg)","payload":\{"type":"([a-zA-Z0-9_-]+)"(?:,|\})"#).unwrap();
    let db = if c["op"] == "query" {
        Some(Connection::open(s(&c["db"])).unwrap())
    } else {
        None
    };
    let blob = if c["op"] == "decode" {
        fs::read(s(&c["blob"])).unwrap()
    } else {
        vec![]
    };
    let (times, result) = match s(&c["op"]) {
        "timeline_typed" => measure(samples, || typed_timeline(&c["timeline"])),
        "cost_typed" => measure(samples, || {
            arr(&c["cases"]).iter().map(typed_cost).collect::<Vec<_>>()
        }),
        "summary_typed" => measure(samples, || typed_summary(arr(&c["sessions"]))),
        _ => measure(samples, || execute(&c, &re, db.as_ref(), &blob)),
    };
    if c["op"] == "encode" {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::fast());
        e.write_all(&serde_json::to_vec(&c["payload"]).unwrap())
            .unwrap();
        fs::write(s(&c["output_blob"]), e.finish().unwrap()).unwrap();
    }
    if c["op"] == "serialize" {
        fs::write(
            s(&c["output_json"]),
            serde_json::to_vec(&c["payload"]).unwrap(),
        )
        .unwrap();
    }
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let rss = status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|x| x.parse::<u64>().ok());
    println!(
        "{}",
        json!({"samples_ms":times,"result":result,"peak_rss_kib":rss})
    );
}

fn measure<T: Serialize>(samples: usize, mut f: impl FnMut() -> T) -> (Vec<f64>, V) {
    let mut times = vec![];
    let mut last = None;
    for i in 0..samples + 2 {
        let t = Instant::now();
        let out = std::hint::black_box(f());
        let ms = t.elapsed().as_secs_f64() * 1000.;
        if i >= 2 {
            times.push(ms)
        }
        last = Some(out);
    }
    (times, serde_json::to_value(last.unwrap()).unwrap())
}
#[derive(Serialize, Clone, Copy, Default)]
struct Usage {
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}
impl From<[i64; 6]> for Usage {
    fn from(a: [i64; 6]) -> Self {
        Self {
            input_tokens: a[0],
            cached_input_tokens: a[1],
            cache_write_tokens: a[2],
            output_tokens: a[3],
            reasoning_output_tokens: a[4],
            total_tokens: a[5],
        }
    }
}
impl Usage {
    fn plus(&mut self, a: [i64; 6]) {
        self.input_tokens += a[0];
        self.cached_input_tokens += a[1];
        self.cache_write_tokens += a[2];
        self.output_tokens += a[3];
        self.reasoning_output_tokens += a[4];
        self.total_tokens += a[5];
    }
}
fn typed_timeline(v: &V) -> Vec<(&V, Usage)> {
    let mut rows: Vec<_> = arr(v)
        .iter()
        .filter_map(|r| stamp(&r["timestamp"]).map(|t| (r, t)))
        .collect();
    rows.sort_by_key(|r| r.1);
    let mut prev = [0; 6];
    rows.into_iter()
        .enumerate()
        .map(|(i, (r, _))| {
            let cur = usage(&r["total_token_usage"]);
            if i == 0 && r["last_token_usage"].is_object() {
                let last = usage(&r["last_token_usage"]);
                let base = sub(cur, last);
                if last.iter().any(|x| *x > 0) && add(base, last) == cur {
                    prev = base;
                }
            }
            let du = delta(cur, prev);
            prev = cur;
            (r, du.into())
        })
        .collect()
}
#[derive(Serialize)]
struct Cost {
    input_tokens: f64,
    cached_input_tokens: f64,
    cache_write_tokens: f64,
    output_tokens: f64,
    reasoning_output_tokens: f64,
}
fn typed_cost(c: &V) -> Cost {
    let u = usage(&c["usage"]);
    let p = &c["prices"];
    let input = u[0].max(0);
    let cache = u[1].max(0).min(input);
    let output = u[3].max(0);
    Cost {
        input_tokens: (input - cache) as f64 * p["input"].as_f64().unwrap() / 1e6,
        cached_input_tokens: cache as f64 * p["cached_input"].as_f64().unwrap() / 1e6,
        cache_write_tokens: 0.,
        output_tokens: output as f64 * p["output"].as_f64().unwrap() / 1e6,
        reasoning_output_tokens: u[4].max(0).min(output) as f64 * p["output"].as_f64().unwrap()
            / 1e6,
    }
}
#[derive(Serialize, Default)]
struct Group {
    #[serde(flatten)]
    labels: Map<String, V>,
    sessions: usize,
    usage: Usage,
}
fn typed_group(
    map: &mut std::collections::HashMap<String, usize>,
    groups: &mut Vec<Group>,
    key: &str,
    labels: impl FnOnce() -> V,
    u: [i64; 6],
) {
    let ix = *map.entry(key.into()).or_insert_with(|| {
        groups.push(Group {
            labels: labels().as_object().unwrap().clone(),
            ..Default::default()
        });
        groups.len() - 1
    });
    groups[ix].sessions += 1;
    groups[ix].usage.plus(u);
}
#[derive(Serialize)]
struct Summary<'a> {
    session_count: usize,
    active_count: usize,
    archived_count: usize,
    usage: Usage,
    estimated_cost_usd: f64,
    estimated_cost_known_count: usize,
    by_model: Vec<Group>,
    by_project: Vec<Group>,
    by_environment: Vec<Group>,
    by_day: Vec<Group>,
    top_session_uid: Option<&'a V>,
}
fn typed_summary(rows: &[V]) -> Summary<'_> {
    let mut result = Summary {
        session_count: rows.len(),
        active_count: 0,
        archived_count: 0,
        usage: Usage::default(),
        estimated_cost_usd: 0.,
        estimated_cost_known_count: 0,
        by_model: vec![],
        by_project: vec![],
        by_environment: vec![],
        by_day: vec![],
        top_session_uid: rows.first().map(|r| &r["uid"]),
    };
    let mut mm = std::collections::HashMap::new();
    let mut pm = std::collections::HashMap::new();
    let mut em = std::collections::HashMap::new();
    let mut dm = std::collections::HashMap::new();
    for r in rows {
        let u = usage(&r["total_token_usage"]);
        result.usage.plus(u);
        if s(&r["source"]) == "active" {
            result.active_count += 1
        }
        if s(&r["source"]) == "archived" {
            result.archived_count += 1
        }
        if let Some(c) = r["estimated_cost_usd"].as_f64() {
            result.estimated_cost_usd += c;
            result.estimated_cost_known_count += 1
        }
        let m = if s(&r["model"]).is_empty() {
            "unknown"
        } else {
            s(&r["model"])
        };
        typed_group(&mut mm, &mut result.by_model, m, || json!({"model":m}), u);
        let pk = ["project_root", "project"]
            .iter()
            .map(|k| s(&r[k]))
            .find(|x| !x.is_empty())
            .unwrap_or("unknown");
        let pl = if s(&r["project"]).is_empty() {
            pk.rsplit(['/', '\\']).next().unwrap_or("unknown")
        } else {
            s(&r["project"])
        };
        typed_group(
            &mut pm,
            &mut result.by_project,
            pk,
            || json!({"project":pl,"project_root":pk}),
            u,
        );
        let ei = if s(&r["environment_id"]).is_empty() {
            "local"
        } else {
            s(&r["environment_id"])
        };
        let el = if s(&r["environment"]).is_empty() {
            ei
        } else {
            s(&r["environment"])
        };
        typed_group(
            &mut em,
            &mut result.by_environment,
            ei,
            || json!({"id":ei,"label":el}),
            u,
        );
        let day = ["end_at", "start_at"]
            .iter()
            .map(|k| s(&r[k]))
            .find(|x| !x.is_empty())
            .unwrap_or("unknown");
        let day = &day[..day.len().min(10)];
        typed_group(&mut dm, &mut result.by_day, day, || json!({"date":day}), u);
    }
    result.estimated_cost_usd = round6(result.estimated_cost_usd);
    for groups in [
        &mut result.by_model,
        &mut result.by_project,
        &mut result.by_environment,
    ] {
        groups.sort_by_key(|g| std::cmp::Reverse(g.usage.total_tokens));
    }
    result.by_project.truncate(20);
    result
        .by_day
        .sort_by(|a, b| s(&a.labels["date"]).cmp(s(&b.labels["date"])));
    result
}
