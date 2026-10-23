//! Streaming session parser with resumable, owned state. The parser only reads
//! the caller's file and never discovers homes or creates user configuration.
use crate::{Error, PriceSchedule, Pricing, RawEvent, Usage, reader, timestamp, usage};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionIdentity {
    pub uid: String,
    pub session_id: String,
    pub root_session_id: String,
    pub parent_thread_id: String,
    pub forked_from_id: String,
    pub thread_source: String,
    pub is_subagent: bool,
    pub agent_path: String,
    pub agent_nickname: String,
    pub agent_role: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_speed: Option<crate::speed::SpeedTotals>,
    #[serde(flatten)]
    pub identity: SessionIdentity,
    #[serde(flatten)]
    pub pricing: Pricing,
    pub title: String,
    pub source: String,
    pub environment: String,
    pub environment_id: String,
    pub is_remote: bool,
    pub remote_device_short_code: String,
    pub remote_imported_at: String,
    pub remote_exported_at: String,
    pub codex_home: String,
    pub path: String,
    pub file_size: i64,
    pub line_count: i64,
    pub parse_errors: i64,
    pub fast_skipped_line_count: i64,
    pub fast_skipped_bytes: i64,
    pub created_at: String,
    pub start_at: String,
    pub end_at: String,
    pub updated_at: Option<String>,
    pub cwd: String,
    pub project: String,
    pub project_root: String,
    pub workspace_root: String,
    pub project_branch: String,
    pub is_git_worktree: bool,
    pub model: String,
    pub models: Vec<String>,
    pub service_tier: String,
    pub service_tiers: Vec<String>,
    pub effort: String,
    pub originator: String,
    pub cli_version: String,
    pub total_token_usage: Usage,
    pub last_token_usage: Usage,
    pub branch_total_token_usage: Usage,
    pub inherited_token_usage: Usage,
    pub inherited_token_event_count: i64,
    pub fork_usage_resolved: bool,
    pub cached_input_percent: Option<f64>,
    pub model_context_window: Option<i64>,
    pub token_event_count: i64,
    pub turn_count: i64,
    pub completed_turn_count: i64,
    pub user_message_count: i64,
    pub assistant_message_count: i64,
    pub first_user_prompt: String,
    pub last_agent_preview: String,
    pub latest_rate_limits: Option<Value>,
    pub latest_plan_type: Option<String>,
    pub latest_rate_limit_reached_type: Option<String>,
    pub tool_counts: BTreeMap<String, i64>,
    pub duration_ms_total: i64,
    pub duration_ms_avg: Option<i64>,
    pub time_to_first_token_ms_avg: Option<i64>,
    pub timeline: Vec<RawEvent>,
    pub tasks: Vec<Task>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_start_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_end_at: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Task {
    pub timestamp: Option<String>,
    pub turn_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub time_to_first_token_ms: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierEvent {
    pub timestamp: String,
    pub service_tier: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParserState {
    #[serde(default)]
    pub(crate) request_timer: crate::speed::RequestTimer,
    pub session_meta_seen: bool,
    pub raw_created_at: String,
    pub raw_start_at: String,
    pub raw_end_at: String,
    pub turn_ids: BTreeSet<String>,
    pub service_tier_events: Vec<TierEvent>,
    pub parsed_end_offset: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFile {
    pub detail: SessionDetail,
    pub state: ParserState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSource {
    pub id: String,
    pub label: String,
    pub home: PathBuf,
}

pub fn folder_name(text: &str) -> String {
    let text = text.trim().trim_matches('"').trim_end_matches(['\\', '/']);
    text.rsplit(['\\', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(text)
        .to_owned()
}
pub(crate) fn clean(text: &str, limit: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= limit {
        text
    } else {
        format!(
            "{}…",
            text.chars().take(limit - 1).collect::<String>().trim_end()
        )
    }
}
fn user_prompt(message: &str) -> String {
    let mut message = message.trim();
    let mut stripped = false;
    loop {
        if message.starts_with("# AGENTS.md instructions for ") {
            return String::new();
        }
        let Some((opening, body)) = message.strip_prefix('<').and_then(|s| s.split_once('>'))
        else {
            break;
        };
        let name = opening.split_whitespace().next().unwrap_or_default();
        if !matches!(
            name,
            "external_codex_apps_open_page" | "in-app-browser-context" | "environment_context"
        ) {
            break;
        }
        let Some((_, rest)) = body.split_once(&format!("</{name}>")) else {
            break;
        };
        message = rest.trim_start();
        stripped = true;
    }
    if stripped {
        message = message
            .strip_prefix("## My request:")
            .unwrap_or(message)
            .trim_start();
    }
    // Filter complete, known context blocks before truncation, preserving any
    // actual request that follows them. Authored markup is otherwise untouched.
    clean(message, 260)
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
fn first<'a>(values: &[&'a Value]) -> &'a str {
    values
        .iter()
        .map(|v| text(v))
        .find(|v| !v.is_empty())
        .unwrap_or("")
}
fn integer(value: &Value) -> Option<i64> {
    if let Some(v) = value.as_i64() {
        Some(v)
    } else if let Some(v) = value.as_bool() {
        Some(i64::from(v))
    } else {
        value
            .as_f64()
            .filter(|v| v.is_finite() && *v >= i64::MIN as f64 && *v < 9_223_372_036_854_775_808.)
            .map(|v| v as i64)
    }
}
pub fn model_from_payload(payload: &Value) -> String {
    [
        &payload["model"],
        &payload["model_slug"],
        &payload["thread_settings"]["model"],
        &payload["thread_settings"]["model_slug"],
        &payload["thread_settings"]["collaboration_mode"]["settings"]["model"],
        &payload["collaboration_mode"]["settings"]["model"],
    ]
    .iter()
    .filter_map(|v| v.as_str())
    .map(str::trim)
    .find(|s| !s.is_empty())
    .unwrap_or("")
    .to_owned()
}
pub fn service_tier_from_payload(payload: &Value) -> String {
    [
        &payload["service_tier"],
        &payload["thread_settings"]["service_tier"],
    ]
    .iter()
    .filter_map(|v| v.as_str())
    .map(str::trim)
    .find(|s| !s.is_empty())
    .unwrap_or("")
    .to_lowercase()
}
fn content(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    if value.is_object() {
        return ["text", "message", "content"]
            .iter()
            .find_map(|key| value[*key].as_str())
            .unwrap_or("")
            .to_owned();
    }
    let Some(rows) = value.as_array() else {
        return String::new();
    };
    rows.iter()
        .filter_map(|v| {
            if let Some(s) = v.as_str() {
                Some(s)
            } else {
                ["text", "message", "content"]
                    .iter()
                    .find_map(|key| v[*key].as_str())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn unique(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !values.iter().any(|v| v == value) {
        values.push(value.into());
    }
}
impl ParsedFile {
    fn note_model(&mut self, value: &str) {
        let model = value.trim();
        if model.is_empty() {
            return;
        }
        self.detail.model = model.into();
        unique(&mut self.detail.models, model);
    }
    fn note_tier(&mut self, value: &str, time: Option<&str>) {
        let tier = value.trim().to_lowercase();
        if tier.is_empty() {
            return;
        }
        self.detail.service_tier = tier.clone();
        unique(&mut self.detail.service_tiers, &tier);
        if let Some(time) = time.filter(|s| !s.is_empty())
            && self
                .state
                .service_tier_events
                .last()
                .is_none_or(|last| last.service_tier != tier)
        {
            self.state.service_tier_events.push(TierEvent {
                timestamp: time.into(),
                service_tier: tier,
            });
        }
    }
    fn consume(&mut self, item: Value) -> Result<(), Error> {
        self.detail.line_count += 1;
        if item["__parse_error__"].as_bool() == Some(true) {
            self.detail.parse_errors += 1;
            return Ok(());
        }
        let time = item["timestamp"].as_str();
        if let Some(time) = time {
            if self.state.raw_start_at.is_empty() || time < self.state.raw_start_at.as_str() {
                self.state.raw_start_at = time.into();
            }
            if self.state.raw_end_at.is_empty() || time > self.state.raw_end_at.as_str() {
                self.state.raw_end_at = time.into();
            }
        }
        let payload = &item["payload"];
        self.state.request_timer.observe(&item);
        match text(&item["type"]) {
            "session_meta" if !self.state.session_meta_seen => {
                self.state.session_meta_seen = true;
                let id = first(&[&payload["id"], &payload["session_id"]]);
                if !id.is_empty() {
                    self.detail.identity.session_id = id.into();
                }
                self.detail.identity.root_session_id = if text(&payload["session_id"]).is_empty() {
                    self.detail.identity.session_id.clone()
                } else {
                    text(&payload["session_id"]).into()
                };
                for (dst, key) in [
                    (&mut self.detail.cwd, "cwd"),
                    (&mut self.state.raw_created_at, "timestamp"),
                    (&mut self.detail.originator, "originator"),
                    (&mut self.detail.cli_version, "cli_version"),
                ] {
                    let value = text(&payload[key]);
                    if !value.is_empty() {
                        *dst = value.into();
                    }
                }
                self.note_model(&model_from_payload(payload));
                self.note_tier(&service_tier_from_payload(payload), time);
                let identity = &mut self.detail.identity;
                identity.thread_source = text(&payload["thread_source"]).into();
                let sub = &payload["source"]["subagent"];
                let spawn = if sub["thread_spawn"].is_object() {
                    &sub["thread_spawn"]
                } else {
                    sub
                };
                for (dst, key) in [
                    (&mut identity.parent_thread_id, "parent_thread_id"),
                    (&mut identity.forked_from_id, "forked_from_id"),
                    (&mut identity.agent_path, "agent_path"),
                    (&mut identity.agent_nickname, "agent_nickname"),
                    (&mut identity.agent_role, "agent_role"),
                ] {
                    *dst = first(&[&payload[key], &spawn[key], &sub[key]]).into();
                }
                identity.is_subagent = sub.as_object().is_some_and(|v| !v.is_empty())
                    || !identity.parent_thread_id.is_empty()
                    || identity.thread_source.to_lowercase() == "subagent";
                if identity.is_subagent && identity.parent_thread_id.is_empty() {
                    identity.parent_thread_id = if !identity.forked_from_id.is_empty() {
                        identity.forked_from_id.clone()
                    } else if identity.root_session_id != identity.session_id {
                        identity.root_session_id.clone()
                    } else {
                        String::new()
                    };
                }
            }
            "turn_context" => {
                if let Some(id) = payload["turn_id"].as_str() {
                    self.state.turn_ids.insert(id.into());
                }
                if let Some(cwd) = payload["cwd"].as_str().filter(|s| !s.is_empty()) {
                    self.detail.cwd = cwd.into();
                }
                self.note_model(&model_from_payload(payload));
                self.note_tier(&service_tier_from_payload(payload), time);
                let effort = first(&[&payload["effort"], &payload["reasoning_effort"]]);
                if !effort.is_empty() {
                    self.detail.effort = effort.into();
                }
            }
            "response_item" => match text(&payload["type"]) {
                "message" => {
                    let message = content(&payload["content"]);
                    match text(&payload["role"]) {
                        "user" => {
                            self.detail.user_message_count += 1;
                            if self.detail.first_user_prompt.is_empty() {
                                self.detail.first_user_prompt = user_prompt(&message);
                            }
                        }
                        "assistant" => {
                            self.detail.assistant_message_count += 1;
                            if !message.is_empty() {
                                self.detail.last_agent_preview = clean(&message, 260);
                            }
                        }
                        _ => {}
                    }
                }
                "function_call" => {
                    let name = payload["name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("function_call");
                    *self.detail.tool_counts.entry(name.into()).or_default() += 1;
                }
                _ => {}
            },
            "event_msg" => match text(&payload["type"]) {
                "thread_settings_applied" => {
                    self.note_model(&model_from_payload(payload));
                    self.note_tier(&service_tier_from_payload(payload), time);
                    let settings = &payload["thread_settings"];
                    let effort = first(&[
                        &settings["reasoning_effort"],
                        &settings["effort"],
                        &payload["effort"],
                    ]);
                    if !effort.is_empty() {
                        self.detail.effort = effort.into();
                    }
                    let cwd = first(&[&settings["cwd"], &payload["cwd"]]);
                    if !cwd.is_empty() {
                        self.detail.cwd = cwd.into();
                    }
                }
                "token_count" => {
                    self.detail.latest_rate_limits = payload["rate_limits"]
                        .as_object()
                        .map(|_| payload["rate_limits"].clone());
                    if let Some(plan) = payload["plan_type"].as_str() {
                        self.detail.latest_plan_type = Some(plan.into());
                    }
                    if let Some(reached) = payload["rate_limit_reached_type"].as_str() {
                        self.detail.latest_rate_limit_reached_type = Some(reached.into());
                    }
                    let info = &payload["info"];
                    if info.is_object() {
                        self.detail.token_event_count += 1;
                        let model = model_from_payload(info);
                        let payload_model = model_from_payload(payload);
                        self.note_model(if model.is_empty() {
                            &payload_model
                        } else {
                            &model
                        });
                        let tier = service_tier_from_payload(info);
                        let payload_tier = service_tier_from_payload(payload);
                        self.note_tier(
                            if tier.is_empty() {
                                &payload_tier
                            } else {
                                &tier
                            },
                            time,
                        );
                        let previous_usage =
                            self.detail.timeline.last().map(|e| e.total_token_usage);
                        self.detail.total_token_usage =
                            serde_json::from_value(info["total_token_usage"].clone())?;
                        self.detail.last_token_usage =
                            serde_json::from_value(info["last_token_usage"].clone())?;
                        let changed = previous_usage != Some(self.detail.total_token_usage);
                        let request_timing = if changed {
                            let timing = self
                                .state
                                .request_timer
                                .finish(time, self.detail.last_token_usage);
                            let output = previous_usage
                                .map(|p| self.detail.total_token_usage.delta(p))
                                .transpose()?
                                .map(|u| u.output_tokens)
                                .unwrap_or(self.detail.last_token_usage.output_tokens);
                            if output > 0 && output == self.detail.last_token_usage.output_tokens {
                                timing
                            } else {
                                Default::default()
                            }
                        } else {
                            Default::default()
                        };
                        if let Some(window) = integer(&info["model_context_window"]) {
                            self.detail.model_context_window = Some(window);
                        }
                        self.detail.timeline.push(RawEvent {
                            request_duration_ms: request_timing.duration_ms,
                            generation_duration_ms: request_timing.generation_ms,
                            timestamp: time.map(str::to_owned),
                            model: self.detail.model.clone(),
                            service_tier: self.detail.service_tier.clone(),
                            total_token_usage: self.detail.total_token_usage,
                            last_token_usage: Some(self.detail.last_token_usage),
                            model_context_window: self.detail.model_context_window,
                            rate_limits: self.detail.latest_rate_limits.clone(),
                        });
                    }
                }
                "task_complete" => {
                    if let Some(turn) = payload["turn_id"].as_str() {
                        self.state.turn_ids.insert(turn.into());
                    }
                    self.detail.tasks.push(Task {
                        timestamp: time.map(str::to_owned),
                        turn_id: payload["turn_id"].as_str().map(str::to_owned),
                        duration_ms: integer(&payload["duration_ms"]),
                        time_to_first_token_ms: integer(&payload["time_to_first_token_ms"]),
                    });
                    if let Some(message) = payload["last_agent_message"].as_str() {
                        self.detail.last_agent_preview = clean(message, 320);
                    }
                }
                "agent_message" | "assistant_message" => {
                    self.detail.assistant_message_count += 1;
                    if let Some(message) = payload["message"].as_str() {
                        self.detail.last_agent_preview = clean(message, 320);
                    }
                }
                "user_message" | "user_input" | "human_message" => {
                    self.detail.user_message_count += 1;
                    let message =
                        first(&[&payload["message"], &payload["text"], &payload["content"]]);
                    if self.detail.first_user_prompt.is_empty() {
                        self.detail.first_user_prompt = user_prompt(message);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
}
static FALLBACK_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"rollout-[^-]+-[^-]+-(.+?)\.jsonl(?:\.zst)?$").unwrap());

/// Passing an owned previous result resumes from its committed byte offset.
/// File identity/prefix validation is the index writer's responsibility.
pub fn parse_file(
    path: &Path,
    kind: &str,
    source: &LogSource,
    pricing: &PriceSchedule,
    base: Option<ParsedFile>,
    end: Option<u64>,
) -> Result<ParsedFile, Error> {
    let path = path.canonicalize()?;
    // Compression changes the representation, not the conversation identity.
    let identity_path = if reader::is_compressed(&path) {
        path.with_extension("")
    } else {
        path.clone()
    };
    let uid = format!(
        "{:x}",
        Sha1::digest(format!("{}:{}", source.id, identity_path.display()).as_bytes())
    )[..16]
        .to_owned();
    let mut parsed = base.unwrap_or_else(|| ParsedFile {
        detail: SessionDetail {
            identity: SessionIdentity {
                uid,
                ..Default::default()
            },
            ..Default::default()
        },
        state: ParserState::default(),
    });
    let offset = parsed.state.parsed_end_offset;
    let stats = reader::read_rollout(&path, offset, end, |item| parsed.consume(item))?;
    parsed.state.parsed_end_offset = offset + stats.bytes_read;
    let detail = &mut parsed.detail;
    detail.file_size = i64::try_from(if reader::is_compressed(&path) {
        end.unwrap_or(std::fs::metadata(&path)?.len())
    } else {
        parsed.state.parsed_end_offset
    })
    .map_err(|_| Error::CounterOverflow)?;
    detail.fast_skipped_line_count += stats.fast_skipped_line_count as i64;
    detail.fast_skipped_bytes += stats.fast_skipped_bytes as i64;
    detail.source = kind.into();
    detail.environment = source.label.clone();
    detail.environment_id = source.id.clone();
    detail.codex_home = source.home.to_string_lossy().into();
    detail.path = path.to_string_lossy().into();
    let filename = path.file_name().unwrap_or_default().to_string_lossy();
    if detail.identity.session_id.is_empty() {
        detail.identity.session_id = FALLBACK_ID
            .captures(&filename)
            .map(|c| c[1].to_owned())
            .unwrap_or_else(|| detail.identity.uid.clone());
    }
    if detail.identity.root_session_id.is_empty() {
        detail.identity.root_session_id = detail.identity.session_id.clone();
    }
    let leaf = detail
        .identity
        .agent_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let mut labels = Vec::new();
    if !detail.identity.agent_nickname.is_empty() {
        labels.push(detail.identity.agent_nickname.clone());
    }
    if !leaf.is_empty()
        && !labels
            .iter()
            .any(|s| s.to_lowercase() == leaf.to_lowercase())
    {
        labels.push(leaf.into());
    }
    let folder = folder_name(&detail.cwd);
    detail.title = if detail.identity.is_subagent && !labels.is_empty() {
        labels.join(" · ")
    } else if !detail.first_user_prompt.is_empty() {
        detail.first_user_prompt.clone()
    } else if !folder.is_empty() {
        folder.clone()
    } else {
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into()
    };
    detail.updated_at = path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|time| {
            // Python converts st_mtime (float seconds), then rounds to microseconds.
            let seconds = time
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let whole = seconds.floor() as i64;
            let micros = ((seconds - whole as f64) * 1e6).round_ties_even() as i64;
            timestamp::format(
                chrono::DateTime::from_timestamp(
                    whole + micros / 1_000_000,
                    (micros % 1_000_000) as u32 * 1000,
                )
                .unwrap(),
            )
        });
    detail.start_at = if !parsed.state.raw_start_at.is_empty() {
        parsed.state.raw_start_at.clone()
    } else if !parsed.state.raw_created_at.is_empty() {
        parsed.state.raw_created_at.clone()
    } else {
        detail.updated_at.clone().unwrap_or_default()
    };
    detail.end_at = if !parsed.state.raw_end_at.is_empty() {
        parsed.state.raw_end_at.clone()
    } else {
        detail
            .updated_at
            .clone()
            .unwrap_or_else(|| detail.start_at.clone())
    };
    detail.created_at = if parsed.state.raw_created_at.is_empty() {
        detail.start_at.clone()
    } else {
        parsed.state.raw_created_at.clone()
    };
    for event in &detail.timeline {
        unique(&mut detail.models, &event.model);
        unique(&mut detail.service_tiers, &event.service_tier);
    }
    unique(&mut detail.models, &detail.model);
    unique(&mut detail.service_tiers, &detail.service_tier);
    detail.branch_total_token_usage = detail.total_token_usage;
    detail.inherited_token_usage = Usage::default();
    detail.inherited_token_event_count = 0;
    detail.fork_usage_resolved = !detail.identity.is_subagent;
    detail.cached_input_percent = if detail.total_token_usage.input_tokens == 0 {
        None
    } else {
        Some(
            format!(
                "{:.1}",
                detail.total_token_usage.cached_input_tokens as f64
                    / detail.total_token_usage.input_tokens as f64
                    * 100.
            )
            .parse()
            .unwrap(),
        )
    };
    let events = usage::normalize(&detail.timeline)?;
    detail.estimated_speed = crate::speed::SpeedTotals::from_events(&events)?;
    detail.pricing = pricing.price_events(
        &events,
        &detail.model,
        detail.total_token_usage,
        Some(&detail.end_at),
        &detail.service_tier,
    )?;
    detail.project = if folder.is_empty() {
        "unknown".into()
    } else {
        folder
    };
    detail.project_root = detail.cwd.clone();
    detail.workspace_root = detail.cwd.clone();
    detail.completed_turn_count = detail.tasks.len() as i64;
    detail.turn_count = if !parsed.state.turn_ids.is_empty() {
        parsed.state.turn_ids.len() as i64
    } else if !detail.tasks.is_empty() {
        detail.tasks.len() as i64
    } else {
        detail.token_event_count
    };
    let durations: Vec<_> = detail.tasks.iter().filter_map(|t| t.duration_ms).collect();
    let ttfs: Vec<_> = detail
        .tasks
        .iter()
        .filter_map(|t| t.time_to_first_token_ms)
        .collect();
    detail.duration_ms_total = durations.iter().try_fold(0i64, |sum, v| {
        sum.checked_add(*v).ok_or(Error::CounterOverflow)
    })?;
    detail.duration_ms_avg = if durations.is_empty() {
        None
    } else {
        Some((detail.duration_ms_total as f64 / durations.len() as f64) as i64)
    };
    detail.time_to_first_token_ms_avg = if ttfs.is_empty() {
        None
    } else {
        Some(
            (ttfs.iter().try_fold(0i64, |sum, v| {
                sum.checked_add(*v).ok_or(Error::CounterOverflow)
            })? as f64
                / ttfs.len() as f64) as i64,
        )
    };
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed() -> ParsedFile {
        ParsedFile {
            detail: SessionDetail::default(),
            state: ParserState::default(),
        }
    }

    fn message(kind: &str, value: &str) -> Value {
        if kind == "response_item" {
            json!({"type": kind, "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": value}]}})
        } else {
            json!({"type": "event_msg", "payload": {"type": kind, "message": value}})
        }
    }

    #[test]
    fn first_user_prompt_skips_injected_context_in_all_message_formats() {
        for kind in [
            "response_item",
            "user_message",
            "user_input",
            "human_message",
        ] {
            let mut parsed = parsed();
            for context in [
                "# AGENTS.md instructions for /demo\n<INSTRUCTIONS>project rules</INSTRUCTIONS>",
                "<environment_context>\n<cwd>/demo</cwd>\n</environment_context>",
                "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>",
                "<external_codex_apps_open_page>{\"page_id\":\"example\"}</external_codex_apps_open_page>",
                "<in-app-browser-context source=\"ambient-ui-state\">Current URL: http://localhost/</in-app-browser-context>",
            ] {
                parsed.consume(message(kind, context)).unwrap();
                assert!(
                    parsed.detail.first_user_prompt.is_empty(),
                    "{kind}: {context}"
                );
            }
            parsed
                .consume(message(kind, "修复登录流程，并保留错误提示。"))
                .unwrap();
            parsed.consume(message(kind, "这是一条后续请求。")).unwrap();
            assert_eq!(
                parsed.detail.first_user_prompt, "修复登录流程，并保留错误提示。",
                "{kind}"
            );
        }
    }

    #[test]
    fn first_user_prompt_preserves_request_after_long_context_before_truncation() {
        let context = format!(
            "<external_codex_apps_open_page>{{\"page_id\":null}}</external_codex_apps_open_page>\n<in-app-browser-context source=\"ambient-ui-state\">{}</in-app-browser-context>\n<environment_context><cwd>/demo</cwd></environment_context>\n\n## My request:\n",
            "自动附带的浏览器上下文。".repeat(100)
        );
        let request = "调整事件列表的布局，保留完整值提示。";
        for kind in [
            "response_item",
            "user_message",
            "user_input",
            "human_message",
        ] {
            let mut parsed = parsed();
            parsed
                .consume(message(kind, &format!("{context}{request}")))
                .unwrap();
            assert_eq!(parsed.detail.first_user_prompt, request, "{kind}");
        }
        let mut parsed = parsed();
        parsed
            .consume(
                json!({"type":"response_item","payload":{"type":"message","role":"user","content":[
                    {"type":"input_text","text":context}, {"type":"input_text","text":request}
                ]}}),
            )
            .unwrap();
        assert_eq!(parsed.detail.first_user_prompt, request);
    }

    #[test]
    fn first_user_prompt_keeps_authored_markup_and_quoted_context() {
        for request in [
            "<request>请修复布局</request>",
            "<environment_contextual>这是用户正文</environment_contextual>",
            "解释 <external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page> 的含义。",
            "```xml\n<environment_context>example</environment_context>\n```",
            "<in-app-browser-context>尚未闭合的示例，请解释这段文本。",
        ] {
            assert_eq!(user_prompt(request), clean(request, 260));
        }
    }
}
