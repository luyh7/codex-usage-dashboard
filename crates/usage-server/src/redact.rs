//! A deliberately allowlisted sharing format. Never forward arbitrary metadata,
//! message previews, rate-limit payloads, tool names, or future unknown fields.
use hmac::{Hmac, Mac};
use serde_json::{Map, Value, json};
use sha2::Sha256;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub fn load_key(root: &Path) -> io::Result<[u8; 32]> {
    fs::create_dir_all(root)?;
    let path = root.join("export-redaction.key");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            let mut key = [0;32]; getrandom::fill(&mut key).map_err(io::Error::other)?;
            file.write_all(&key)?; file.sync_all()?; Ok(key)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => fs::read(path)?.try_into()
            .map_err(|_| io::Error::other("invalid redaction key; preserve the existing key to keep anonymous identities stable")),
        Err(e) => Err(e),
    }
}

pub struct Redactor<'a>(pub &'a [u8; 32]);

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn keep(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|key| value.get(*key).map(|v| ((*key).into(), v.clone())))
            .collect(),
    )
}
impl Redactor<'_> {
    fn id(&self, domain: &str, value: &str) -> String {
        if value.is_empty() {
            return String::new();
        }
        let mut hmac = Hmac::<Sha256>::new_from_slice(self.0).unwrap();
        hmac.update(domain.as_bytes());
        hmac.update(&[0]);
        hmac.update(value.as_bytes());
        format!(
            "{domain}-{}",
            hmac.finalize().into_bytes()[..16]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }
    fn row(&self, value: &Value, detail: bool) -> Value {
        let mut out = keep(
            value,
            &[
                "is_subagent",
                "is_git_worktree",
                "source",
                "model",
                "models",
                "service_tier",
                "service_tiers",
                "effort",
                "file_size",
                "parse_errors",
                "total_token_usage",
                "last_token_usage",
                "branch_total_token_usage",
                "inherited_token_usage",
                "inherited_token_event_count",
                "fork_usage_resolved",
                "estimated_cost_usd",
                "estimated_cost_breakdown_usd",
                "price_model_known",
                "cached_input_percent",
                "token_event_count",
                "turn_count",
                "completed_turn_count",
                "duration_ms_total",
                "time_to_first_token_ms_avg",
            ],
        );
        for key in [
            "created_at",
            "start_at",
            "end_at",
            "updated_at",
            "last_usage_at",
            "period_start_at",
            "period_end_at",
        ] {
            if let Some(time) = value.get(key) {
                out[key] = if time.is_null() {
                    Value::Null
                } else {
                    usage_core::timestamp::parse(text(time))
                        .map(|t| json!(usage_core::timestamp::format(t)))
                        .unwrap_or(json!(""))
                };
            }
        }
        out["uid"] = json!(self.id("uid", text(&value["uid"])));
        for key in [
            "session_id",
            "root_session_id",
            "parent_thread_id",
            "forked_from_id",
        ] {
            out[key] = json!(self.id("session", text(&value[key])));
        }
        let project = text(&value["project_root"]);
        let project = if project.is_empty() {
            text(&value["project"])
        } else {
            project
        };
        let project = self.id("project", project);
        out["project"] = json!(project);
        out["project_root"] = json!(project);
        out["cwd"] = json!(project);
        out["workspace_root"] = json!(self.id("workspace", text(&value["workspace_root"])));
        out["path"] = json!(self.id("log", text(&value["uid"])));
        out["title"] = json!(format!("Session {}", self.id("uid", text(&value["uid"]))));
        out["environment_id"] = json!(self.id("source", text(&value["environment_id"])));
        out["environment"] = json!("Device");
        out["codex_home"] = json!("");
        if detail {
            for key in [
                "line_count",
                "fast_skipped_line_count",
                "fast_skipped_bytes",
                "model_context_window",
                "user_message_count",
                "assistant_message_count",
                "duration_ms_avg",
            ] {
                if let Some(v) = value.get(key) {
                    out[key] = v.clone();
                }
            }
            out["first_user_prompt"] = json!("");
            out["last_agent_preview"] = json!("");
            out["timeline"] = json!(
                value["timeline"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|event| keep(
                        event,
                        &[
                            "timestamp",
                            "model",
                            "service_tier",
                            "total_token_usage",
                            "last_token_usage",
                            "model_context_window"
                        ]
                    ))
                    .collect::<Vec<_>>()
            );
            out["tasks"] = json!(
                value["tasks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|task| keep(
                        task,
                        &["timestamp", "duration_ms", "time_to_first_token_ms"]
                    ))
                    .collect::<Vec<_>>()
            );
            out["applied_price_segments"] = json!(
                value["applied_price_segments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|segment| keep(
                        segment,
                        &[
                            "model",
                            "effective_at",
                            "effective_until",
                            "context_tier",
                            "service_tier",
                            "cost_multiplier",
                            "prices",
                            "usage",
                            "estimated_cost_usd",
                            "estimated_cost_breakdown_usd",
                            "price_known"
                        ]
                    ))
                    .collect::<Vec<_>>()
            );
        }
        out
    }
    pub fn document(&self, value: &Value) -> Value {
        let data = &value["snapshot"];
        let mut summary = keep(
            &data["summary"],
            &[
                "session_count",
                "main_count",
                "agent_count",
                "active_count",
                "archived_count",
                "usage",
                "estimated_cost_usd",
                "estimated_cost_known_count",
                "by_model",
                "model_breakdown",
                "by_day",
            ],
        );
        summary["top_session_uid"] = if data["summary"]["top_session_uid"].is_null() {
            Value::Null
        } else {
            json!(self.id("uid", text(&data["summary"]["top_session_uid"])))
        };
        summary["by_project"] = json!(
            data["summary"]["by_project"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| {
                    let mut out = keep(p, &["sessions", "usage"]);
                    let root = text(&p["project_root"]);
                    let id = self.id(
                        "project",
                        if root.is_empty() {
                            text(&p["project"])
                        } else {
                            root
                        },
                    );
                    out["project"] = json!(id);
                    out["project_root"] = json!(id);
                    out
                })
                .collect::<Vec<_>>()
        );
        summary["by_environment"] = json!(
            data["summary"]["by_environment"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| {
                    let mut out = keep(p, &["sessions", "usage"]);
                    out["id"] = json!(self.id("source", text(&p["id"])));
                    out["label"] = json!("Device");
                    out
                })
                .collect::<Vec<_>>()
        );
        let details: Map<String, Value> = data["details_by_uid"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(uid, row)| (self.id("uid", uid), self.row(row, true)))
            .collect();
        json!({"schema":usage_core::remote::SCHEMA,"version":1,"redacted":true,"exported_at":value["exported_at"],
            "device":{"short_code":self.id("shared",text(&value["device"]["short_code"])),"label":"Shared statistics","platform":"","hostname":""},
            "snapshot":{"generated_at":data["generated_at"],"codex_home":"","codex_sources":[],"summary":summary,
                "sessions":data["sessions"].as_array().into_iter().flatten().map(|r|self.row(r,false)).collect::<Vec<_>>(),
                "details_by_uid":details,"daily_usage":data["daily_usage"]}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_fields_are_removed_and_relations_and_statistics_survive() {
        let dir = tempfile::tempdir().unwrap();
        let key = load_key(dir.path()).unwrap();
        assert_eq!(load_key(dir.path()).unwrap(), key);
        let private = "PRIVATE_CANARY";
        let mut row = json!({"uid":"parent-uid","session_id":"parent","root_session_id":"parent",
            "total_token_usage":{"total_tokens":120},"estimated_cost_usd":0.5,
            "model":"gpt-5","models":["gpt-5"],"service_tier":"standard",
            "timeline":[{"timestamp":"2026-10-01T00:00:01Z","model":"gpt-5",
                "total_token_usage":{"total_tokens":120},"rate_limits":{"secret":private}}],
            "tasks":[{"timestamp":"2026-10-01T00:00:01Z","duration_ms":100,"secret":private}]});
        for field in [
            "title",
            "path",
            "cwd",
            "project",
            "project_root",
            "workspace_root",
            "project_branch",
            "environment",
            "environment_id",
            "agent_path",
            "agent_nickname",
            "agent_role",
            "first_user_prompt",
            "last_agent_preview",
            "originator",
            "cli_version",
            "future_private_field",
        ] {
            row[field] = json!(private);
        }
        row["tool_counts"] = json!({private:1});
        let mut child = row.clone();
        child["uid"] = json!("child-uid");
        child["session_id"] = json!("child");
        child["parent_thread_id"] = json!("parent");
        child["forked_from_id"] = json!("parent");
        let document = json!({"device":{"short_code":private,"label":private,"hostname":private},
            "snapshot":{"codex_home":private,"codex_sources":[private],"sessions":[row,child],
                "details_by_uid":{"parent-uid":row,"child-uid":child},
                "summary":{"usage":{"total_tokens":240},"by_project":[{"project":private,"project_root":private}],
                    "by_environment":[{"id":private,"label":private}]},"daily_usage":[]}});
        let redacted = Redactor(&key).document(&document);
        assert!(!redacted.to_string().contains(private));
        assert_eq!(
            redacted,
            Redactor(&load_key(dir.path()).unwrap()).document(&document)
        );
        assert_ne!(
            redacted["device"],
            Redactor(&[1; 32]).document(&document)["device"]
        );
        let parent = &redacted["snapshot"]["sessions"][0];
        let child = &redacted["snapshot"]["sessions"][1];
        assert_eq!(child["parent_thread_id"], parent["session_id"]);
        assert_eq!(child["forked_from_id"], parent["session_id"]);
        assert_eq!(parent["total_token_usage"], row["total_token_usage"]);
        assert_eq!(parent["estimated_cost_usd"], row["estimated_cost_usd"]);
        let detail = &redacted["snapshot"]["details_by_uid"][parent["uid"].as_str().unwrap()];
        assert_eq!(detail["uid"], parent["uid"]);
        assert_eq!(
            detail["timeline"][0]["total_token_usage"]["total_tokens"],
            120
        );
        assert_eq!(detail["tasks"][0]["duration_ms"], 100);
        assert_eq!(
            redacted["snapshot"]["summary"]["by_project"][0]["project"],
            parent["project"]
        );
    }
}
