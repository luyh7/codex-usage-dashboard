//! Read-only runtime service-tier evidence; unavailable/locked old schemas are
//! optional evidence, never a reason to create or modify a Codex database.
use crate::parser::TierEvent;
use crate::timestamp;
use chrono::{DateTime, Duration};
use regex::Regex;
use rusqlite::{Connection, OpenFlags, params_from_iter};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::LazyLock,
    time,
};
static TOKENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|[{}():,]"#).unwrap());
pub fn tier_from_body(body: &str) -> String {
    if !body.contains("Submission sub=Submission {") {
        return String::new();
    }
    let tokens: Vec<_> = TOKENS.find_iter(body).map(|m| m.as_str()).collect();
    let marker = ["thread_settings", ":", "ThreadSettingsOverrides", "{"];
    for index in 0..tokens.len().saturating_sub(marker.len()) {
        if tokens[index..index + 4] != marker {
            continue;
        }
        let mut depth = 1;
        for cursor in index + 4..tokens.len() {
            match tokens[cursor] {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        return String::new();
                    }
                }
                "service_tier" if depth == 1 && tokens.get(cursor + 1) == Some(&":") => {
                    let values = &tokens[(cursor + 2).min(tokens.len())..];
                    if values.starts_with(&["Some", "(", "None", ")"]) {
                        return "default".into();
                    }
                    if values.len() >= 7
                        && values[..4] == ["Some", "(", "Some", "("]
                        && values[5..7] == [")", ")"]
                        && [
                            "\"priority\"",
                            "\"fast\"",
                            "\"ultrafast\"",
                            "\"default\"",
                            "\"standard\"",
                        ]
                        .contains(&values[4])
                    {
                        return values[4].trim_matches('"').into();
                    }
                    return String::new();
                }
                _ => {}
            }
        }
    }
    String::new()
}
fn read(
    home: &Path,
    threads: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<TierEvent>>, rusqlite::Error> {
    let mut result = BTreeMap::<String, Vec<TierEvent>>::new();
    if threads.is_empty() {
        return Ok(result);
    }
    let connection = Connection::open_with_flags(
        home.join("logs_2.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(time::Duration::from_millis(200))?;
    let threads: Vec<_> = threads.iter().collect();
    for batch in threads.chunks(400) {
        let placeholders = vec!["?"; batch.len()].join(",");
        let sql = format!(
            "SELECT thread_id,ts,ts_nanos,feedback_log_body FROM logs WHERE thread_id IN ({placeholders}) AND target = 'codex_core::session::handlers' AND feedback_log_body LIKE '%thread_settings: ThreadSettingsOverrides%' AND feedback_log_body LIKE '%service_tier:%' ORDER BY ts,ts_nanos,id"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(batch.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            ))
        })?;
        for row in rows {
            let (id, seconds, nanos, body) = row?;
            let tier = tier_from_body(&body);
            if tier.is_empty() {
                continue;
            }
            if let Some(moment) = DateTime::from_timestamp(seconds, 0)
                .and_then(|t| t.checked_add_signed(Duration::microseconds(nanos.div_euclid(1000))))
            {
                result.entry(id).or_default().push(TierEvent {
                    timestamp: timestamp::format(moment),
                    service_tier: tier,
                });
            }
        }
    }
    Ok(result)
}
pub fn service_tiers(home: &Path, threads: &BTreeSet<String>) -> BTreeMap<String, Vec<TierEvent>> {
    read(home, threads).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_content_cannot_supply_settings() {
        assert_eq!(
            tier_from_body(
                r#"Submission sub=Submission { text: "thread_settings: ThreadSettingsOverrides { service_tier: Some(Some(\"priority\")) }" }"#
            ),
            ""
        );
        assert_eq!(
            tier_from_body(
                r#"Submission sub=Submission { thread_settings: ThreadSettingsOverrides { nested: Other { service_tier: Some(Some("priority")) }, service_tier: Some(None) } }"#
            ),
            "default"
        );
    }
}
