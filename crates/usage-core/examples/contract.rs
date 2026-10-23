//! Test-only JSON-lines adapter. Production callers use the typed library.
use serde_json::{Value, json};
use std::io::{self, BufRead};
use usage_core::{Error, PriceSchedule, RawEvent, Usage, reader, usage};
fn run(value: Value, pricing: &PriceSchedule) -> Result<Value, Error> {
    match value["op"].as_str().unwrap_or("") {
        "timestamp" => Ok(serde_json::to_value(
            value["timestamp"]
                .as_str()
                .and_then(usage_core::timestamp::parse)
                .map(usage_core::timestamp::format),
        )?),
        "price" => Ok(serde_json::to_value(pricing.quote(
            value["model"].as_str().unwrap_or(""),
            value["timestamp"].as_str(),
            value["input_tokens"].as_i64(),
            value["service_tier"].as_str().unwrap_or(""),
        ))?),
        "cost" => Ok(serde_json::to_value(pricing.estimate(
            serde_json::from_value(value["usage"].clone())?,
            value["model"].as_str().unwrap_or(""),
            value["timestamp"].as_str(),
            value["service_tier"].as_str().unwrap_or(""),
        ))?),
        "normalize_usage" => Ok(serde_json::to_value(serde_json::from_value::<Usage>(
            value["usage"].clone(),
        )?)?),
        "timeline" | "pricing" => {
            let events: Vec<RawEvent> = serde_json::from_value(value["timeline"].clone())?;
            let normalized = usage::normalize(&events)?;
            if value["op"] == "pricing" {
                let fallback = if value["usage"].is_object() {
                    serde_json::from_value(value["usage"].clone())?
                } else {
                    Usage::default()
                };
                return Ok(serde_json::to_value(pricing.price_events(
                    &normalized,
                    value["model"].as_str().unwrap_or(""),
                    fallback,
                    value["timestamp"].as_str(),
                    value["service_tier"].as_str().unwrap_or(""),
                )?)?);
            }
            Ok(json!(normalized.iter().map(|e| json!({"source_order":e.source_order,"timestamp":usage_core::timestamp::format(e.timestamp),"cumulative":e.cumulative,"delta":e.delta})).collect::<Vec<_>>()))
        }
        "remote_store" => {
            let root = std::path::PathBuf::from(value["root"].as_str().unwrap());
            let mut store = usage_core::remote::Store::new(
                root.clone(),
                value["device"].as_str().unwrap().into(),
                chrono_tz::UTC,
            );
            match value["action"].as_str().unwrap() {
                "import" => store.import(
                    value["snapshot"].clone(),
                    value["label"].as_str(),
                    value["allow_current_device"].as_bool().unwrap_or(false),
                ),
                "rename" => store.rename(
                    value["code"].as_str().unwrap(),
                    value["label"].as_str().unwrap(),
                ),
                "delete" => {
                    store.delete(value["code"].as_str().unwrap())?;
                    Ok(Value::Null)
                }
                "ranged" => {
                    let (_, remotes) = store.refresh(pricing)?;
                    let snapshot = usage_core::index::Snapshot {
                        revision: 1,
                        generated_at: "2026-10-21T00:00:00Z".into(),
                        sessions: remotes
                            .into_iter()
                            .flat_map(|r| r.sessions.clone())
                            .collect(),
                    };
                    let period = usage_core::query::Period::bounds(
                        "custom",
                        value["start"].as_str(),
                        value["end"].as_str(),
                        chrono_tz::UTC,
                        value["now"]
                            .as_str()
                            .and_then(usage_core::timestamp::parse)
                            .unwrap_or_else(chrono::Utc::now),
                    )?;
                    let view =
                        usage_core::view::View::new(&snapshot, period, chrono_tz::UTC, pricing)?;
                    let mut details = serde_json::Map::new();
                    for row in view.summaries() {
                        details.insert(
                            row.uid.clone(),
                            view.legacy_detail(&row.uid)
                                .map_err(|e| Error::InvalidData(format!("{e:?}")))?
                                .unwrap(),
                        );
                    }
                    Ok(json!({"sessions":view.wire_summaries()?,"details_by_uid":details}))
                }
                "transformed" => {
                    let (_, remotes) = store.refresh(pricing)?;
                    let mut sessions = Vec::new();
                    let mut details = serde_json::Map::new();
                    let mut sources = Vec::new();
                    for remote in remotes {
                        let meta = &remote.metadata;
                        let code = &meta.device_short_code;
                        sources.push(json!({"id":format!("remote-{code}"),"label":meta.label,"codex_home":format!("remote:{code}"),"is_remote":true}));
                        for entry in &remote.sessions {
                            let mut row = serde_json::to_value(entry.summary.as_ref().unwrap())?;
                            entry.wire.as_ref().unwrap().summary.apply(&mut row);
                            sessions.push(row);
                            if entry.has_detail {
                                let mut row = serde_json::to_value(entry.detail.as_ref())?;
                                entry.wire.as_ref().unwrap().detail.apply(&mut row);
                                details.insert(entry.detail.identity.uid.clone(), row);
                            }
                        }
                    }
                    Ok(json!([sessions, details, sources]))
                }
                _ => Err(Error::InvalidData("unknown remote test action".into())),
            }
        }
        "indexed_scan" => {
            let sources = serde_json::from_value(value["sources"].clone())?;
            let config = usage_core::index::IndexConfig {
                database: std::path::PathBuf::from(value["database"].as_str().unwrap()),
                sources,
            };
            let mut index = usage_core::index::Index::open(config)?;
            index.refresh(|_| {})?;
            let snapshot = index.snapshot();
            let period = usage_core::query::Period::bounds(
                "all",
                None,
                None,
                chrono_tz::UTC,
                chrono::Utc::now(),
            )?;
            let view = usage_core::view::View::new(&snapshot, period, chrono_tz::UTC, pricing)?;
            let sessions: Vec<_> = view.summaries().collect();
            let details: serde_json::Map<String, Value> = view
                .details()
                .map(|detail| {
                    (
                        detail.identity.uid.clone(),
                        serde_json::to_value(detail).unwrap(),
                    )
                })
                .collect();
            Ok(
                json!({"sessions":sessions,"summary":usage_core::query::summarize(view.summaries())?,"daily_usage":view.daily_usage,"details_by_uid":details}),
            )
        }
        "normalize_files" => {
            let mut files = Vec::new();
            for row in value["files"].as_array().unwrap() {
                let source = usage_core::parser::LogSource {
                    id: row["source_id"].as_str().unwrap().into(),
                    label: row["source_label"].as_str().unwrap().into(),
                    home: std::path::PathBuf::from(row["home"].as_str().unwrap()),
                };
                let mut parsed = usage_core::parser::parse_file(
                    std::path::Path::new(row["path"].as_str().unwrap()),
                    row["kind"].as_str().unwrap(),
                    &source,
                    pricing,
                    None,
                    None,
                )?;
                let ids =
                    std::collections::BTreeSet::from([parsed.detail.identity.session_id.clone()]);
                let mut tiers = usage_core::runtime::service_tiers(&source.home, &ids);
                if let Some(events) = tiers.remove(&parsed.detail.identity.session_id) {
                    usage_core::normalize::apply_runtime_tiers(&mut parsed, events);
                }
                files.push(parsed);
            }
            let mut files = usage_core::normalize::normalize_sessions(files, pricing)?;
            if value["with_summary"] == true {
                files.sort_by_key(|f| std::cmp::Reverse(f.detail.total_token_usage.total_tokens));
                let sessions: Vec<_> = files
                    .iter()
                    .map(|f| usage_core::query::SessionSummary::from(&f.detail))
                    .collect();
                let summary = usage_core::query::summarize(&sessions)?;
                let daily = usage_core::query::daily_usage(
                    files.iter().map(|f| &f.detail),
                    chrono_tz::UTC,
                )?;
                let details: serde_json::Map<String, Value> = files
                    .into_iter()
                    .map(|f| {
                        (
                            f.detail.identity.uid.clone(),
                            serde_json::to_value(f.detail).unwrap(),
                        )
                    })
                    .collect();
                return Ok(
                    json!({"sessions":sessions,"summary":summary,"daily_usage":daily,"details_by_uid":details}),
                );
            }
            let result: serde_json::Map<String, Value> = files
                .into_iter()
                .map(|file| {
                    (
                        file.detail.identity.uid.clone(),
                        serde_json::to_value(file.detail).unwrap(),
                    )
                })
                .collect();
            Ok(Value::Object(result))
        }
        "period_detail" => {
            let detail = serde_json::from_value(value["detail"].clone())?;
            let start = usage_core::timestamp::parse(value["start"].as_str().unwrap()).unwrap();
            let end = usage_core::timestamp::parse(value["end"].as_str().unwrap()).unwrap();
            Ok(serde_json::to_value(usage_core::query::detail_for_period(
                &detail, start, end, pricing,
            )?)?)
        }
        "parse_file" => {
            let source = usage_core::parser::LogSource {
                id: value["source_id"].as_str().unwrap_or("fixture").into(),
                label: value["source_label"].as_str().unwrap_or("Synthetic").into(),
                home: std::path::PathBuf::from(value["home"].as_str().unwrap()),
            };
            let base = if value["base"].is_object() {
                Some(serde_json::from_value(value["base"].clone())?)
            } else {
                None
            };
            let parsed = usage_core::parser::parse_file(
                std::path::Path::new(value["path"].as_str().unwrap()),
                value["kind"].as_str().unwrap_or("active"),
                &source,
                pricing,
                base,
                value["end"].as_u64(),
            )?;
            if value["return_state"] == true {
                Ok(serde_json::to_value(parsed)?)
            } else {
                Ok(serde_json::to_value(parsed.detail)?)
            }
        }
        "reader" => {
            let mut rows = Vec::new();
            let stats = reader::read_rollout(
                std::path::Path::new(value["path"].as_str().unwrap()),
                value["start"].as_u64().unwrap_or(0),
                value["end"].as_u64(),
                |row| {
                    rows.push(row);
                    Ok(())
                },
            )?;
            Ok(json!({"rows":rows,"stats":stats}))
        }
        _ => Err(Error::InvalidData("unknown test operation".into())),
    }
}
fn main() {
    let pricing = PriceSchedule::bundled();
    for line in io::stdin().lock().lines() {
        let result = line
            .map_err(Error::Io)
            .and_then(|line| serde_json::from_str(&line).map_err(Error::Json))
            .and_then(|value| run(value, &pricing));
        match result {
            Ok(value) => println!("{}", value),
            Err(e) => println!("{}", json!({"error":e.to_string()})),
        }
    }
}
