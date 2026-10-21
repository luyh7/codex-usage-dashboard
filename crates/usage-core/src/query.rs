//! Host-neutral list summaries, calendar aggregation and date-range views.
use crate::{
    Error, PriceSchedule, Usage,
    parser::SessionDetail,
    pricing::{CostParts, round_money},
    timestamp, usage,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionSummary {
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
    pub parse_errors: i64,
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
    pub total_token_usage: Usage,
    pub last_token_usage: Usage,
    pub branch_total_token_usage: Usage,
    pub inherited_token_usage: Usage,
    pub inherited_token_event_count: i64,
    pub fork_usage_resolved: bool,
    pub estimated_cost_usd: Option<f64>,
    pub estimated_cost_breakdown_usd: Option<CostParts>,
    pub price_model_known: bool,
    pub cached_input_percent: Option<f64>,
    pub token_event_count: i64,
    pub turn_count: i64,
    pub completed_turn_count: i64,
    pub duration_ms_total: i64,
    pub time_to_first_token_ms_avg: Option<i64>,
}
impl From<&SessionDetail> for SessionSummary {
    fn from(detail: &SessionDetail) -> Self {
        Self {
            uid: detail.identity.uid.clone(),
            session_id: detail.identity.session_id.clone(),
            root_session_id: detail.identity.root_session_id.clone(),
            parent_thread_id: detail.identity.parent_thread_id.clone(),
            forked_from_id: detail.identity.forked_from_id.clone(),
            thread_source: detail.identity.thread_source.clone(),
            is_subagent: detail.identity.is_subagent,
            agent_path: detail.identity.agent_path.clone(),
            agent_nickname: detail.identity.agent_nickname.clone(),
            agent_role: detail.identity.agent_role.clone(),
            title: detail.title.clone(),
            source: detail.source.clone(),
            environment: detail.environment.clone(),
            environment_id: detail.environment_id.clone(),
            is_remote: detail.is_remote,
            remote_device_short_code: detail.remote_device_short_code.clone(),
            remote_imported_at: detail.remote_imported_at.clone(),
            remote_exported_at: detail.remote_exported_at.clone(),
            codex_home: detail.codex_home.clone(),
            path: detail.path.clone(),
            file_size: detail.file_size,
            parse_errors: detail.parse_errors,
            created_at: detail.created_at.clone(),
            start_at: detail.start_at.clone(),
            end_at: detail.end_at.clone(),
            updated_at: detail.updated_at.clone(),
            cwd: detail.cwd.clone(),
            project: detail.project.clone(),
            project_root: detail.project_root.clone(),
            workspace_root: detail.workspace_root.clone(),
            project_branch: detail.project_branch.clone(),
            is_git_worktree: detail.is_git_worktree,
            model: detail.model.clone(),
            models: detail.models.clone(),
            service_tier: detail.service_tier.clone(),
            service_tiers: detail.service_tiers.clone(),
            effort: detail.effort.clone(),
            total_token_usage: detail.total_token_usage,
            last_token_usage: detail.last_token_usage,
            branch_total_token_usage: detail.branch_total_token_usage,
            inherited_token_usage: detail.inherited_token_usage,
            inherited_token_event_count: detail.inherited_token_event_count,
            fork_usage_resolved: detail.fork_usage_resolved,
            estimated_cost_usd: detail.pricing.estimated_cost_usd,
            estimated_cost_breakdown_usd: detail.pricing.estimated_cost_breakdown_usd.clone(),
            price_model_known: detail.pricing.price_model_known,
            cached_input_percent: detail.cached_input_percent,
            token_event_count: detail.token_event_count,
            turn_count: detail.turn_count,
            completed_turn_count: detail.completed_turn_count,
            duration_ms_total: detail.duration_ms_total,
            time_to_first_token_ms_avg: detail.time_to_first_token_ms_avg,
        }
    }
}

impl SessionDetail {
    pub fn metadata(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            pricing: self.pricing.clone(),
            title: self.title.clone(),
            source: self.source.clone(),
            environment: self.environment.clone(),
            environment_id: self.environment_id.clone(),
            is_remote: self.is_remote,
            remote_device_short_code: self.remote_device_short_code.clone(),
            remote_imported_at: self.remote_imported_at.clone(),
            remote_exported_at: self.remote_exported_at.clone(),
            codex_home: self.codex_home.clone(),
            path: self.path.clone(),
            file_size: self.file_size,
            line_count: self.line_count,
            parse_errors: self.parse_errors,
            fast_skipped_line_count: self.fast_skipped_line_count,
            fast_skipped_bytes: self.fast_skipped_bytes,
            created_at: self.created_at.clone(),
            start_at: self.start_at.clone(),
            end_at: self.end_at.clone(),
            updated_at: self.updated_at.clone(),
            cwd: self.cwd.clone(),
            project: self.project.clone(),
            project_root: self.project_root.clone(),
            workspace_root: self.workspace_root.clone(),
            project_branch: self.project_branch.clone(),
            is_git_worktree: self.is_git_worktree,
            model: self.model.clone(),
            models: self.models.clone(),
            service_tier: self.service_tier.clone(),
            service_tiers: self.service_tiers.clone(),
            effort: self.effort.clone(),
            originator: self.originator.clone(),
            cli_version: self.cli_version.clone(),
            total_token_usage: self.total_token_usage,
            last_token_usage: self.last_token_usage,
            branch_total_token_usage: self.branch_total_token_usage,
            inherited_token_usage: self.inherited_token_usage,
            inherited_token_event_count: self.inherited_token_event_count,
            fork_usage_resolved: self.fork_usage_resolved,
            cached_input_percent: self.cached_input_percent,
            model_context_window: self.model_context_window,
            token_event_count: self.token_event_count,
            turn_count: self.turn_count,
            completed_turn_count: self.completed_turn_count,
            user_message_count: self.user_message_count,
            assistant_message_count: self.assistant_message_count,
            first_user_prompt: self.first_user_prompt.clone(),
            last_agent_preview: self.last_agent_preview.clone(),
            latest_rate_limits: self.latest_rate_limits.clone(),
            latest_plan_type: self.latest_plan_type.clone(),
            latest_rate_limit_reached_type: self.latest_rate_limit_reached_type.clone(),
            tool_counts: self.tool_counts.clone(),
            duration_ms_total: self.duration_ms_total,
            duration_ms_avg: self.duration_ms_avg,
            time_to_first_token_ms_avg: self.time_to_first_token_ms_avg,
            timeline: Vec::new(),
            tasks: Vec::new(),
            period_start_at: self.period_start_at.clone(),
            period_end_at: self.period_end_at.clone(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub session_count: usize,
    pub active_count: usize,
    pub archived_count: usize,
    pub usage: Usage,
    pub estimated_cost_usd: f64,
    pub estimated_cost_known_count: usize,
    pub by_model: Vec<ModelUsage>,
    pub by_project: Vec<ProjectUsage>,
    pub by_environment: Vec<EnvironmentUsage>,
    pub by_day: Vec<DaySessions>,
    pub top_session_uid: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub sessions: usize,
    pub usage: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUsage {
    pub project: String,
    pub project_root: String,
    pub sessions: usize,
    pub usage: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentUsage {
    pub id: String,
    pub label: String,
    pub sessions: usize,
    pub usage: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySessions {
    pub date: String,
    pub sessions: usize,
    pub usage: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub usage: Usage,
}
fn fallback<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|s| !s.is_empty())
        .unwrap_or("unknown")
}

pub fn summarize<'a>(
    sessions: impl IntoIterator<Item = &'a SessionSummary>,
) -> Result<Summary, Error> {
    let mut summary = Summary::default();
    let mut models = HashMap::new();
    let mut projects = HashMap::new();
    let mut envs = HashMap::new();
    let mut days = BTreeMap::<String, DaySessions>::new();
    for session in sessions {
        if summary.session_count == 0 {
            summary.top_session_uid = Some(session.uid.clone());
        }
        summary.session_count += 1;
        summary.active_count += usize::from(session.source == "active");
        summary.archived_count += usize::from(session.source == "archived");
        summary.usage = summary.usage.checked_add(session.total_token_usage)?;
        if let Some(cost) = session.estimated_cost_usd {
            summary.estimated_cost_usd += cost;
            summary.estimated_cost_known_count += 1;
        }
        let model = fallback(&[&session.model]);
        let index = *models.entry(model.to_owned()).or_insert_with(|| {
            summary.by_model.push(ModelUsage {
                model: model.into(),
                sessions: 0,
                usage: Usage::default(),
            });
            summary.by_model.len() - 1
        });
        summary.by_model[index].sessions += 1;
        summary.by_model[index].usage = summary.by_model[index]
            .usage
            .checked_add(session.total_token_usage)?;
        let key = fallback(&[&session.project_root, &session.project]);
        let name = if session.project.is_empty() {
            crate::parser::folder_name(key)
        } else {
            session.project.clone()
        };
        let index = *projects.entry(key.to_owned()).or_insert_with(|| {
            summary.by_project.push(ProjectUsage {
                project: name,
                project_root: key.into(),
                sessions: 0,
                usage: Usage::default(),
            });
            summary.by_project.len() - 1
        });
        summary.by_project[index].sessions += 1;
        summary.by_project[index].usage = summary.by_project[index]
            .usage
            .checked_add(session.total_token_usage)?;
        let id = if session.environment_id.is_empty() {
            "local"
        } else {
            &session.environment_id
        };
        let label = fallback(&[&session.environment, id]);
        let index = *envs.entry(id.to_owned()).or_insert_with(|| {
            summary.by_environment.push(EnvironmentUsage {
                id: id.into(),
                label: label.into(),
                sessions: 0,
                usage: Usage::default(),
            });
            summary.by_environment.len() - 1
        });
        summary.by_environment[index].sessions += 1;
        summary.by_environment[index].usage = summary.by_environment[index]
            .usage
            .checked_add(session.total_token_usage)?;
        let date: String = fallback(&[&session.end_at, &session.start_at])
            .chars()
            .take(10)
            .collect();
        let day = days.entry(date.clone()).or_insert(DaySessions {
            date,
            sessions: 0,
            usage: Usage::default(),
        });
        day.sessions += 1;
        day.usage = day.usage.checked_add(session.total_token_usage)?;
    }
    summary.estimated_cost_usd = round_money(summary.estimated_cost_usd);
    summary
        .by_model
        .sort_by_key(|row| std::cmp::Reverse(row.usage.total_tokens));
    summary
        .by_project
        .sort_by_key(|row| std::cmp::Reverse(row.usage.total_tokens));
    summary.by_project.truncate(20);
    summary
        .by_environment
        .sort_by_key(|row| std::cmp::Reverse(row.usage.total_tokens));
    summary.by_day = days.into_values().collect();
    Ok(summary)
}

pub fn daily_usage<'a>(
    details: impl IntoIterator<Item = &'a SessionDetail>,
    zone: Tz,
) -> Result<Vec<DailyUsage>, Error> {
    let mut days = BTreeMap::<String, Usage>::new();
    for detail in details {
        for event in usage::normalize(&detail.timeline)? {
            if event.delta.total_tokens <= 0 {
                continue;
            }
            let date = event
                .timestamp
                .with_timezone(&zone)
                .date_naive()
                .to_string();
            let total = days.entry(date).or_default();
            *total = total.checked_add(event.delta)?;
        }
    }
    Ok(days
        .into_iter()
        .map(|(date, usage)| DailyUsage { date, usage })
        .collect())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Period {
    pub key: String,
    pub start_at: Option<String>,
    pub end_at: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}
fn day_start(day: NaiveDate, zone: Tz) -> Result<DateTime<Utc>, Error> {
    // Some zones skip midnight during offset changes. Choose the first valid
    // local minute that day; ambiguity picks the earlier instant.
    let midnight = day.and_hms_opt(0, 0, 0).unwrap();
    for minute in 0..=1440 {
        if let Some(time) = zone
            .from_local_datetime(&(midnight + Duration::minutes(minute)))
            .earliest()
        {
            return Ok(time.to_utc());
        }
    }
    Err(Error::InvalidData(
        "local date has no valid boundary".into(),
    ))
}
fn date(text: Option<&str>) -> Option<NaiveDate> {
    text.and_then(timestamp::parse_date)
}
impl Period {
    pub fn bounds(
        key: &str,
        start: Option<&str>,
        end: Option<&str>,
        zone: Tz,
        now: DateTime<Utc>,
    ) -> Result<Self, Error> {
        let today = now.with_timezone(&zone).date_naive();
        if key == "all" {
            return Ok(Self {
                key: key.into(),
                start_at: None,
                end_at: timestamp::format(now),
                start_date: None,
                end_date: None,
            });
        }
        let key = if ["today", "7d", "30d", "week", "month", "custom"].contains(&key) {
            key
        } else {
            "today"
        };
        let (mut first, mut last) = if key == "custom" {
            let first = date(start).unwrap_or(today);
            (first, date(end).unwrap_or(first))
        } else {
            let first = match key {
                "7d" => today - Duration::days(6),
                "30d" => today - Duration::days(29),
                "week" => today - Duration::days(today.weekday().num_days_from_monday() as i64),
                "month" => today.with_day(1).unwrap(),
                _ => today,
            };
            (first, today)
        };
        if first > last {
            std::mem::swap(&mut first, &mut last);
        }
        let start_at = day_start(first, zone)?;
        let end_at = if last >= today {
            now
        } else {
            day_start(
                last.succ_opt()
                    .ok_or_else(|| Error::InvalidData("date overflow".into()))?,
                zone,
            )? - Duration::microseconds(1)
        };
        Ok(Self {
            key: key.into(),
            start_at: Some(timestamp::format(start_at)),
            end_at: timestamp::format(end_at),
            start_date: Some(first.to_string()),
            end_date: Some(last.to_string()),
        })
    }
    pub fn range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        Some((
            timestamp::parse(self.start_at.as_deref()?)?,
            timestamp::parse(&self.end_at)?,
        ))
    }
}

pub fn detail_for_period(
    detail: &SessionDetail,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    pricing: &PriceSchedule,
) -> Result<SessionDetail, Error> {
    let events = usage::normalize(&detail.timeline)?;
    detail_for_events(
        detail,
        events.iter().map(|e| (e.source, e.timestamp, e.delta)),
        start,
        end,
        pricing,
    )
}

pub fn detail_for_period_indexed(
    indexed: &crate::index::IndexedSession,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    pricing: &PriceSchedule,
) -> Result<SessionDetail, Error> {
    detail_for_events(
        &indexed.detail,
        indexed.events.iter().filter_map(|e| {
            Some((
                &indexed.detail.timeline[e.source_order],
                DateTime::from_timestamp_micros(e.timestamp_us)?,
                e.delta,
            ))
        }),
        start,
        end,
        pricing,
    )
}
fn detail_for_events<'a>(
    detail: &SessionDetail,
    events: impl Iterator<Item = (&'a crate::RawEvent, DateTime<Utc>, Usage)>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    pricing: &PriceSchedule,
) -> Result<SessionDetail, Error> {
    let mut ranged = detail.metadata();
    ranged.period_start_at = Some(timestamp::format(start));
    ranged.period_end_at = Some(timestamp::format(end));
    let mut total = Usage::default();
    let mut last = Usage::default();
    for (source, time, delta) in events {
        if time < start {
            continue;
        }
        if time > end {
            break;
        }
        total = total.checked_add(delta)?;
        last = delta;
        let mut row = source.clone();
        row.total_token_usage = total;
        row.last_token_usage = Some(last);
        ranged.timeline.push(row);
    }
    ranged.tasks = detail
        .tasks
        .iter()
        .filter(|task| {
            task.timestamp
                .as_deref()
                .and_then(timestamp::parse)
                .is_some_and(|t| t >= start && t <= end)
        })
        .cloned()
        .collect();
    ranged.total_token_usage = total;
    ranged.last_token_usage = last;
    let mut models = Vec::new();
    for row in &ranged.timeline {
        if !row.model.is_empty() && !models.contains(&row.model) {
            models.push(row.model.clone());
        }
    }
    if let Some(last) = ranged.timeline.last() {
        if !last.model.is_empty() {
            ranged.model = last.model.clone();
        }
        if !last.service_tier.is_empty() {
            ranged.service_tier = last.service_tier.clone();
        }
    }
    if models.is_empty() && !ranged.model.is_empty() {
        models.push(ranged.model.clone());
    }
    if !models.is_empty() {
        ranged.models = models;
    }
    ranged.service_tiers.clear();
    for row in &ranged.timeline {
        if !row.service_tier.is_empty() && !ranged.service_tiers.contains(&row.service_tier) {
            ranged.service_tiers.push(row.service_tier.clone());
        }
    }
    if !ranged.service_tier.is_empty() && !ranged.service_tiers.contains(&ranged.service_tier) {
        ranged.service_tiers.push(ranged.service_tier.clone());
    }
    let normalized = usage::normalize(&ranged.timeline)?;
    let price_time = ranged
        .timeline
        .last()
        .and_then(|e| e.timestamp.as_deref())
        .unwrap_or(ranged.period_end_at.as_deref().unwrap());
    ranged.pricing = pricing.price_events(
        &normalized,
        &ranged.model,
        total,
        Some(price_time),
        &ranged.service_tier,
    )?;
    ranged.cached_input_percent = if total.input_tokens == 0 {
        None
    } else {
        Some(
            format!(
                "{:.1}",
                total.cached_input_tokens as f64 / total.input_tokens as f64 * 100.
            )
            .parse()
            .unwrap(),
        )
    };
    ranged.token_event_count = ranged.timeline.len() as i64;
    ranged.turn_count = if ranged.tasks.is_empty() {
        ranged.token_event_count
    } else {
        ranged.tasks.len() as i64
    };
    ranged.completed_turn_count = ranged.tasks.len() as i64;
    let durations: Vec<_> = ranged.tasks.iter().filter_map(|t| t.duration_ms).collect();
    let ttfs: Vec<_> = ranged
        .tasks
        .iter()
        .filter_map(|t| t.time_to_first_token_ms)
        .collect();
    let sum = |values: &[i64]| {
        values.iter().try_fold(0i64, |sum, v| {
            sum.checked_add(*v).ok_or(Error::CounterOverflow)
        })
    };
    ranged.duration_ms_total = sum(&durations)?;
    ranged.duration_ms_avg = if durations.is_empty() {
        None
    } else {
        Some((ranged.duration_ms_total as f64 / durations.len() as f64) as i64)
    };
    ranged.time_to_first_token_ms_avg = if ttfs.is_empty() {
        None
    } else {
        Some((sum(&ttfs)? as f64 / ttfs.len() as f64) as i64)
    };
    if let Some(first) = ranged.timeline.first()
        && let Some(time) = &first.timestamp
    {
        ranged.start_at = time.clone();
    }
    if let Some(last) = ranged.timeline.last() {
        if let Some(time) = &last.timestamp {
            ranged.end_at = time.clone();
        }
        ranged.model_context_window = last.model_context_window;
        ranged.latest_rate_limits = last.rate_limits.clone();
    }
    Ok(ranged)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_zone_bounds_cover_dst_and_midnight() {
        let now = timestamp::parse("2026-12-01T12:00:00Z").unwrap();
        let zone: Tz = "America/New_York".parse().unwrap();
        for (day, hours) in [("2026-03-08", 23), ("2026-11-01", 25)] {
            let range = Period::bounds("custom", Some(day), Some(day), zone, now)
                .unwrap()
                .range()
                .unwrap();
            assert_eq!(
                (range.1 - range.0 + Duration::microseconds(1)).num_hours(),
                hours
            );
        }
        let range = Period::bounds(
            "custom",
            Some("2026-10-01"),
            Some("2026-10-01"),
            "Asia/Shanghai".parse().unwrap(),
            now,
        )
        .unwrap();
        assert_eq!(range.start_at.as_deref(), Some("2026-09-30T16:00:00Z"));
        assert_eq!(range.end_at, "2026-10-01T15:59:59.999999Z");
    }
}
