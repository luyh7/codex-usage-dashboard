//! Immutable, revision-bound queries. No filesystem, database or host calls.
use crate::{
    Error, PriceSchedule, RawEvent, Usage,
    index::{IndexedSession, Snapshot},
    parser::SessionDetail,
    query::{self, DailyUsage, Period, SessionSummary, Summary},
    timestamp,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub enum QueryError {
    Stale,
    Missing,
    Invalid(String),
    Core(Error),
}
impl From<Error> for QueryError {
    fn from(error: Error) -> Self {
        Self::Core(error)
    }
}
impl From<serde_json::Error> for QueryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub search: String,
    pub model: String,
    pub environment: String,
    pub source: String,
    pub project: String,
    /// all, main, or agents; agent usage is already exclusive of inherited usage.
    pub agents: String,
    /// tokens, cost, updated, title; ties retain the publication order.
    pub sort: String,
    pub ascending: bool,
}
struct Selection {
    indices: Vec<usize>,
    summary: Summary,
}
struct Entry {
    indexed: IndexedSession,
    summary: SessionSummary,
    search: String,
    event_order: Vec<usize>,
}
pub struct View {
    pub revision: String,
    pub generated_at: String,
    pub period: Period,
    pub daily_usage: Vec<DailyUsage>,
    entries: Vec<Entry>,
    by_uid: HashMap<String, usize>,
    selections: Mutex<HashMap<String, Arc<Selection>>>,
}
#[derive(Serialize)]
pub struct Page<'a, T: Serialize> {
    pub query_token: String,
    pub revision: &'a str,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub summary: Summary,
    pub items: Vec<T>,
}
#[derive(Serialize)]
pub struct ProjectRow {
    pub key: String,
    pub label: String,
    pub session_count: usize,
    pub usage: Usage,
    pub estimated_cost_usd: f64,
    pub estimated_cost_known_count: usize,
    pub environments: Vec<String>,
    #[serde(skip)]
    sort_time: i64,
}
#[derive(Serialize)]
pub struct Events<'a> {
    pub revision: &'a str,
    pub id: &'a str,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub next_cursor: Option<String>,
    pub previous_cursor: Option<String>,
    pub events: Vec<&'a RawEvent>,
}
#[derive(Serialize)]
pub struct ChartPoint<'a> {
    pub timestamp: &'a Option<String>,
    pub total_token_usage: ChartUsage,
}
#[derive(Serialize)]
pub struct ChartUsage {
    pub total_tokens: i64,
}
#[derive(Serialize)]
pub struct Chart<'a> {
    pub revision: &'a str,
    pub id: &'a str,
    pub timeline: Vec<ChartPoint<'a>>,
}
#[derive(Serialize)]
pub struct TaskGroup<'a> {
    pub identity: String,
    pub sessions: Vec<&'a SessionSummary>,
    pub summary: Summary,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    version: u8,
    revision: String,
    binding: String,
    limit: usize,
    offset: usize,
}
fn hash(value: &impl Serialize) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn cursor(revision: &str, binding: &str, limit: usize, offset: usize) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&Cursor {
            version: 1,
            revision: revision.into(),
            binding: binding.into(),
            limit,
            offset,
        })
        .unwrap(),
    )
}
fn offset(
    value: &str,
    revision: &str,
    binding: &str,
    limit: usize,
    total: usize,
) -> Result<usize, QueryError> {
    if !(1..=100).contains(&limit) {
        return Err(QueryError::Invalid(
            "limit must be between 1 and 100".into(),
        ));
    }
    if value.is_empty() {
        return Ok(0);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| QueryError::Invalid("invalid cursor".into()))?;
    let c: Cursor = serde_json::from_slice(&decoded)
        .map_err(|_| QueryError::Invalid("invalid cursor".into()))?;
    if c.revision != revision {
        return Err(QueryError::Stale);
    }
    if c.version != 1
        || c.binding != binding
        || c.limit != limit
        || !c.offset.is_multiple_of(limit)
        || c.offset >= total
    {
        return Err(QueryError::Invalid("cursor does not match query".into()));
    }
    Ok(c.offset)
}
fn neighbors(
    revision: &str,
    binding: &str,
    limit: usize,
    offset: usize,
    total: usize,
) -> (Option<String>, Option<String>) {
    (
        (offset + limit < total).then(|| cursor(revision, binding, limit, offset + limit)),
        (offset > 0).then(|| cursor(revision, binding, limit, offset.saturating_sub(limit))),
    )
}
impl View {
    pub fn new(
        snapshot: &Snapshot,
        period: Period,
        zone: Tz,
        prices: &PriceSchedule,
    ) -> Result<Self, Error> {
        let revision = hash(&(
            snapshot.revision,
            &snapshot.generated_at,
            &period,
            zone.name(),
        ))?;
        let mut entries = Vec::new();
        let mut days = BTreeMap::<String, Usage>::new();
        for indexed in &snapshot.sessions {
            let indexed = if let Some((start, end)) = period.range() {
                if !indexed.has_detail {
                    continue;
                }
                let detail = query::detail_for_period_indexed(indexed, start, end, prices)?;
                if detail.token_event_count <= 0 && detail.total_token_usage.total_tokens <= 0 {
                    continue;
                }
                let mut ranged = IndexedSession::from_detail(Arc::new(detail))?;
                if let Some(wire) = &indexed.wire {
                    let mut detail_shape = wire.detail.clone();
                    detail_shape.include(&[
                        "period_start_at",
                        "period_end_at",
                        "total_token_usage",
                        "last_token_usage",
                        "model",
                        "service_tier",
                        "models",
                        "service_tiers",
                        "estimated_cost_usd",
                        "estimated_cost_breakdown_usd",
                        "price_model_known",
                        "applied_price_segments",
                        "cached_input_percent",
                        "token_event_count",
                        "turn_count",
                        "completed_turn_count",
                        "duration_ms_total",
                        "duration_ms_avg",
                        "time_to_first_token_ms_avg",
                        "start_at",
                        "end_at",
                        "model_context_window",
                        "latest_rate_limits",
                    ]);
                    let event_indices: Vec<_> = indexed
                        .events
                        .iter()
                        .filter(|e| {
                            e.timestamp_us >= start.timestamp_micros()
                                && e.timestamp_us <= end.timestamp_micros()
                        })
                        .map(|e| e.source_order)
                        .collect();
                    let task_indices: Vec<_> = indexed
                        .detail
                        .tasks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| {
                            t.timestamp
                                .as_deref()
                                .and_then(timestamp::parse)
                                .is_some_and(|t| t >= start && t <= end)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    detail_shape.retain_array(
                        "timeline",
                        &event_indices,
                        &["total_token_usage", "last_token_usage"],
                    );
                    detail_shape.retain_array("tasks", &task_indices, &[]);
                    let mut metadata = serde_json::to_value(ranged.detail.metadata())?;
                    detail_shape.apply(&mut metadata);
                    let schema = crate::wire::summary_schema();
                    let row: serde_json::Map<String, serde_json::Value> = schema
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(|k| (k.clone(), metadata.get(k).cloned().unwrap_or_default()))
                        .collect();
                    ranged.wire = Some(Arc::new(crate::wire::RemoteWire {
                        summary: crate::wire::Shape::capture(
                            &serde_json::Value::Object(row),
                            &schema,
                            true,
                        ),
                        detail: detail_shape,
                    }));
                }
                ranged
            } else {
                indexed.clone()
            };
            for event in indexed.events.iter() {
                if event.delta.total_tokens <= 0 {
                    continue;
                }
                let Some(time) = DateTime::<Utc>::from_timestamp_micros(event.timestamp_us) else {
                    continue;
                };
                let date = time.with_timezone(&zone).date_naive().to_string();
                let total = days.entry(date).or_default();
                *total = total.checked_add(event.delta)?;
            }
            let detail = &indexed.detail;
            let summary = indexed
                .summary
                .as_ref()
                .map(|s| s.as_ref().clone())
                .unwrap_or_else(|| SessionSummary::from(detail.as_ref()));
            let models = if summary.models.is_empty() {
                vec![if summary.model.is_empty() {
                    "unknown".into()
                } else {
                    summary.model.clone()
                }]
            } else {
                summary
                    .models
                    .iter()
                    .map(|s| {
                        if s.is_empty() {
                            "unknown".into()
                        } else {
                            s.clone()
                        }
                    })
                    .collect()
            };
            let search = [
                summary.title.as_str(),
                &summary.session_id,
                &models.join(", "),
                &summary.project,
                &summary.project_root,
                &summary.workspace_root,
                &summary.project_branch,
                &summary.cwd,
                &summary.path,
                &summary.source,
                &summary.environment,
            ]
            .join(" ")
            .to_lowercase();
            let mut event_order: Vec<_> = (0..detail.timeline.len()).collect();
            // Cache the complete order, including malformed timestamps omitted by
            // the delta index. Stable ties keep the original event order.
            event_order.sort_by_cached_key(|i| {
                Reverse(
                    detail.timeline[*i]
                        .timestamp
                        .as_deref()
                        .and_then(timestamp::parse),
                )
            });
            entries.push(Entry {
                indexed,
                summary,
                search,
                event_order,
            });
        }
        entries.sort_by_key(|e| Reverse(e.summary.total_token_usage.total_tokens));
        let by_uid = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.summary.uid.clone(), i))
            .collect();
        Ok(Self {
            revision,
            generated_at: snapshot.generated_at.clone(),
            period,
            daily_usage: days
                .into_iter()
                .map(|(date, usage)| DailyUsage { date, usage })
                .collect(),
            entries,
            by_uid,
            selections: Mutex::new(HashMap::new()),
        })
    }
    pub fn wire_summaries(&self) -> Result<Vec<serde_json::Value>, serde_json::Error> {
        self.entries
            .iter()
            .map(|entry| {
                let mut value = serde_json::to_value(&entry.summary)?;
                if let Some(wire) = &entry.indexed.wire {
                    wire.summary.apply(&mut value);
                }
                Ok(value)
            })
            .collect()
    }
    pub fn legacy_detail(&self, id: &str) -> Result<Option<serde_json::Value>, QueryError> {
        let entry = self.entry(id)?;
        let Some(wire) = &entry.indexed.wire else {
            return Ok(None);
        };
        let mut value = serde_json::to_value(entry.indexed.detail.as_ref())?;
        wire.detail.apply(&mut value);
        Ok(Some(value))
    }
    pub fn legacy_events(
        &self,
        id: &str,
        cursor: &str,
        limit: usize,
    ) -> Result<Option<serde_json::Value>, QueryError> {
        let entry = self.entry(id)?;
        let Some(wire) = &entry.indexed.wire else {
            return Ok(None);
        };
        let page = self.query_events(id, cursor, limit)?;
        let start = page.offset;
        let mut value = serde_json::to_value(page)?;
        for (i, row) in value["events"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            wire.detail.apply_event(entry.event_order[start + i], row);
        }
        Ok(Some(value))
    }
    pub fn details(&self) -> impl Iterator<Item = &SessionDetail> {
        self.entries
            .iter()
            .filter(|e| e.indexed.has_detail)
            .map(|e| e.indexed.detail.as_ref())
    }
    pub fn summaries(&self) -> impl Iterator<Item = &SessionSummary> {
        self.entries.iter().map(|e| &e.summary)
    }
    pub fn summary(&self) -> Result<Summary, QueryError> {
        Ok(self.selection(&Filter::default())?.summary.clone())
    }
    fn selection(&self, query: &Filter) -> Result<Arc<Selection>, QueryError> {
        let key = serde_json::to_string(query)?;
        if let Some(hit) = self.selections.lock().unwrap().get(&key) {
            return Ok(hit.clone());
        }
        if !["", "all", "main", "agents"].contains(&query.agents.as_str())
            || !["", "tokens", "cost", "updated", "title"].contains(&query.sort.as_str())
        {
            return Err(QueryError::Invalid("invalid sort or agent filter".into()));
        }
        let search = query.search.trim().to_lowercase();
        let mut indices: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let s = &entry.summary;
                (search.is_empty() || entry.search.contains(&search))
                    && (query.model.is_empty()
                        || s.model == query.model
                        || s.models.contains(&query.model))
                    && (query.environment.is_empty() || s.environment_id == query.environment)
                    && (query.source.is_empty() || s.source == query.source)
                    && (query.project.is_empty() || project_key(s) == query.project)
                    && match query.agents.as_str() {
                        "main" => !s.is_subagent,
                        "agents" => s.is_subagent,
                        _ => true,
                    }
            })
            .map(|(i, _)| i)
            .collect();
        indices.sort_by(|a, b| {
            let a = &self.entries[*a].summary;
            let b = &self.entries[*b].summary;
            let cmp = match query.sort.as_str() {
                "cost" => a
                    .estimated_cost_usd
                    .partial_cmp(&b.estimated_cost_usd)
                    .unwrap_or(std::cmp::Ordering::Equal),
                "updated" => row_time(a).cmp(&row_time(b)),
                "title" => a.title.cmp(&b.title),
                _ => a
                    .total_token_usage
                    .total_tokens
                    .cmp(&b.total_token_usage.total_tokens),
            };
            if query.ascending { cmp } else { cmp.reverse() }
        });
        let summary = query::summarize(indices.iter().map(|i| &self.entries[*i].summary))?;
        let result = Arc::new(Selection { indices, summary });
        let mut cache = self.selections.lock().unwrap();
        if cache.len() >= 32 {
            cache.clear();
        }
        cache.insert(key, result.clone());
        Ok(result)
    }
    pub fn query_sessions(
        &self,
        query: &Filter,
        page_cursor: &str,
        limit: usize,
    ) -> Result<Page<'_, &SessionSummary>, QueryError> {
        let selection = self.selection(query)?;
        let total = selection.indices.len();
        let binding = hash(&("sessions", query))?;
        let start = offset(page_cursor, &self.revision, &binding, limit, total)?;
        let (next_cursor, previous_cursor) =
            neighbors(&self.revision, &binding, limit, start, total);
        Ok(Page {
            query_token: hash(&(&self.revision, &binding, limit, start))?,
            revision: &self.revision,
            total,
            offset: start,
            limit,
            next_cursor,
            previous_cursor,
            summary: selection.summary.clone(),
            items: selection
                .indices
                .iter()
                .skip(start)
                .take(limit)
                .map(|i| &self.entries[*i].summary)
                .collect(),
        })
    }
    pub fn query_projects(
        &self,
        query: &Filter,
        page_cursor: &str,
        limit: usize,
    ) -> Result<Page<'_, ProjectRow>, QueryError> {
        let selection = self.selection(query)?;
        let mut rows = Vec::<ProjectRow>::new();
        let mut keys = HashMap::new();
        for i in &selection.indices {
            let session = &self.entries[*i].summary;
            let key = project_key(session);
            let next = rows.len();
            let index = *keys.entry(key).or_insert(next);
            if index == next {
                rows.push(ProjectRow {
                    key: key.into(),
                    label: session.project.clone(),
                    session_count: 0,
                    usage: Usage::default(),
                    estimated_cost_usd: 0.,
                    estimated_cost_known_count: 0,
                    environments: Vec::new(),
                    sort_time: 0,
                });
            }
            let row = &mut rows[index];
            row.session_count += 1;
            row.sort_time = row.sort_time.max(row_time(session));
            row.usage = row.usage.checked_add(session.total_token_usage)?;
            if let Some(cost) = session.estimated_cost_usd {
                row.estimated_cost_usd += cost;
                row.estimated_cost_known_count += 1;
            }
            if !row.environments.contains(&session.environment) {
                row.environments.push(session.environment.clone());
            }
        }
        rows.sort_by(|a, b| {
            let cmp = match query.sort.as_str() {
                "title" => a.label.cmp(&b.label),
                "updated" => a.sort_time.cmp(&b.sort_time),
                "cost" => a.estimated_cost_usd.total_cmp(&b.estimated_cost_usd),
                _ => a.usage.total_tokens.cmp(&b.usage.total_tokens),
            };
            if query.ascending { cmp } else { cmp.reverse() }
        });
        let total = rows.len();
        let binding = hash(&("projects", query))?;
        let start = offset(page_cursor, &self.revision, &binding, limit, total)?;
        let (next_cursor, previous_cursor) =
            neighbors(&self.revision, &binding, limit, start, total);
        Ok(Page {
            query_token: hash(&(&self.revision, &binding, limit, start))?,
            revision: &self.revision,
            total,
            offset: start,
            limit,
            next_cursor,
            previous_cursor,
            summary: selection.summary.clone(),
            items: rows.into_iter().skip(start).take(limit).collect(),
        })
    }
    pub fn query_project_tasks(
        &self,
        query: &Filter,
        project: &str,
        page_cursor: &str,
        limit: usize,
    ) -> Result<Page<'_, TaskGroup<'_>>, QueryError> {
        let mut query = query.clone();
        query.project = project.into();
        let selection = self.selection(&query)?;
        let mut groups = Vec::<(String, Vec<&SessionSummary>)>::new();
        let mut keys = HashMap::new();
        for i in &selection.indices {
            let s = &self.entries[*i].summary;
            let root = if s.root_session_id.is_empty() {
                &s.session_id
            } else {
                &s.root_session_id
            };
            let key = serde_json::to_string(&(&s.environment_id, root))?;
            let next = groups.len();
            let index = *keys.entry(key.clone()).or_insert(next);
            if index == next {
                groups.push((key, Vec::new()));
            }
            groups[index].1.push(s);
        }
        let mut sortable = Vec::new();
        for (identity, sessions) in groups {
            let root = sessions
                .iter()
                .find(|s| !s.is_subagent)
                .unwrap_or(&sessions[0]);
            let title = root.title.clone();
            let tokens = sessions.iter().try_fold(0i64, |sum, s| {
                sum.checked_add(s.total_token_usage.total_tokens)
                    .ok_or(Error::CounterOverflow)
            })?;
            let cost = if sessions
                .iter()
                .all(|s| s.price_model_known && s.estimated_cost_usd.is_some())
            {
                sessions
                    .iter()
                    .map(|s| s.estimated_cost_usd.unwrap())
                    .sum::<f64>()
            } else {
                0.
            };
            let time = sessions.iter().map(|s| row_time(s)).max().unwrap_or(0);
            sortable.push((identity, sessions, tokens, cost, time, title));
        }
        sortable.sort_by(|a, b| {
            let cmp = match query.sort.as_str() {
                "title" => a.5.cmp(&b.5),
                "cost" => a.3.total_cmp(&b.3),
                "updated" => a.4.cmp(&b.4),
                _ => a.2.cmp(&b.2),
            };
            if query.ascending { cmp } else { cmp.reverse() }
        });
        let groups: Vec<_> = sortable
            .into_iter()
            .map(|(id, rows, _, _, _, _)| (id, rows))
            .collect();
        let total = groups.len();
        let binding = hash(&("tasks", &query, project))?;
        let start = offset(page_cursor, &self.revision, &binding, limit, total)?;
        let (next_cursor, previous_cursor) =
            neighbors(&self.revision, &binding, limit, start, total);
        let items = groups
            .into_iter()
            .skip(start)
            .take(limit)
            .map(|(identity, sessions)| {
                let summary = query::summarize(sessions.iter().copied())?;
                Ok(TaskGroup {
                    identity,
                    sessions,
                    summary,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Page {
            query_token: hash(&(&self.revision, &binding, limit, start))?,
            revision: &self.revision,
            total,
            offset: start,
            limit,
            next_cursor,
            previous_cursor,
            summary: selection.summary.clone(),
            items,
        })
    }
    fn entry(&self, id: &str) -> Result<&Entry, QueryError> {
        self.by_uid
            .get(id)
            .map(|i| &self.entries[*i])
            .filter(|entry| entry.indexed.has_detail)
            .ok_or(QueryError::Missing)
    }
    pub fn query_session_detail(&self, id: &str) -> Result<&SessionDetail, QueryError> {
        Ok(&self.entry(id)?.indexed.detail)
    }
    pub fn query_events<'a>(
        &'a self,
        id: &'a str,
        page_cursor: &str,
        limit: usize,
    ) -> Result<Events<'a>, QueryError> {
        let entry = self.entry(id)?;
        let total = entry.event_order.len();
        let binding = hash(&("events", id))?;
        let start = offset(page_cursor, &self.revision, &binding, limit, total)?;
        let (next_cursor, previous_cursor) =
            neighbors(&self.revision, &binding, limit, start, total);
        Ok(Events {
            revision: &self.revision,
            id,
            total,
            offset: start,
            limit,
            next_cursor,
            previous_cursor,
            events: entry
                .event_order
                .iter()
                .skip(start)
                .take(limit)
                .map(|i| &entry.indexed.detail.timeline[*i])
                .collect(),
        })
    }
    pub fn event_anchor(
        &self,
        id: &str,
        requested: usize,
        limit: usize,
    ) -> Result<String, QueryError> {
        let page = self.query_events(id, "", limit)?;
        let offset = requested.min(page.total.saturating_sub(1)) / limit * limit;
        let binding = hash(&("events", id))?;
        Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Cursor {
            version: 1,
            revision: self.revision.clone(),
            binding,
            limit,
            offset,
        })?))
    }
    pub fn query_chart<'a>(&'a self, id: &'a str) -> Result<Chart<'a>, QueryError> {
        Ok(Chart {
            revision: &self.revision,
            id,
            timeline: self
                .entry(id)?
                .indexed
                .detail
                .timeline
                .iter()
                .map(|e| ChartPoint {
                    timestamp: &e.timestamp,
                    total_token_usage: ChartUsage {
                        total_tokens: e.total_token_usage.total_tokens,
                    },
                })
                .collect(),
        })
    }
}
fn row_time(row: &SessionSummary) -> i64 {
    [
        &row.end_at,
        row.updated_at.as_ref().unwrap_or(&String::new()),
        &row.start_at,
    ]
    .into_iter()
    .find(|s| !s.is_empty())
    .and_then(|s| timestamp::parse(s))
    .map(|t| t.timestamp_millis())
    .unwrap_or(0)
}
/// Re-anchor a current publication at a previous page offset. Cursors returned
/// by queries remain bound to the exact revision/filter/page size.
pub fn anchor_cursor<T: Serialize>(page: &Page<'_, T>, offset: usize) -> Option<String> {
    if page.total == 0 {
        return None;
    }
    let offset = offset.min(page.total - 1) / page.limit * page.limit;
    if offset == 0 {
        return None;
    }
    let encoded = page
        .next_cursor
        .as_ref()
        .or(page.previous_cursor.as_ref())?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let mut value: Cursor = serde_json::from_slice(&bytes).ok()?;
    value.offset = offset;
    Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).ok()?))
}
pub fn project_key(session: &SessionSummary) -> &str {
    [
        &session.project_root,
        &session.cwd,
        &session.project,
        &session.path,
        &session.session_id,
    ]
    .into_iter()
    .find(|s| !s.is_empty())
    .map(String::as_str)
    .unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::SessionIdentity;
    fn view(count: usize, generation: u64) -> View {
        let events = (0..count)
            .map(|i| RawEvent {
                timestamp: Some(timestamp::format(
                    DateTime::from_timestamp(1_790_812_800 + (i / 2) as i64, 0).unwrap(),
                )),
                model: "gpt-5".into(),
                total_token_usage: Usage {
                    total_tokens: (i + 1) as i64,
                    ..Usage::default()
                },
                ..RawEvent::default()
            })
            .collect();
        let detail = SessionDetail {
            identity: SessionIdentity {
                uid: "one".into(),
                session_id: "same-id".into(),
                root_session_id: "same-id".into(),
                ..SessionIdentity::default()
            },
            title: "Find ME".into(),
            timeline: events,
            total_token_usage: Usage {
                total_tokens: count as i64,
                ..Usage::default()
            },
            ..SessionDetail::default()
        };
        let mut other = detail.metadata();
        other.identity.uid = "two".into();
        other.title = "Other".into();
        other.environment_id = "remote-device".into();
        let snapshot = Snapshot {
            revision: generation,
            generated_at: "2026-10-01T01:00:00Z".into(),
            sessions: vec![
                IndexedSession::from_detail(Arc::new(detail)).unwrap(),
                IndexedSession::from_detail(Arc::new(other)).unwrap(),
            ],
        };
        View::new(
            &snapshot,
            Period::bounds(
                "all",
                None,
                None,
                chrono_tz::UTC,
                timestamp::parse(&snapshot.generated_at).unwrap(),
            )
            .unwrap(),
            chrono_tz::UTC,
            &PriceSchedule::bundled(),
        )
        .unwrap()
    }
    #[test]
    fn all_events_page_once_with_stable_ties_and_bound_cursors() {
        let view = view(10003, 1);
        let mut next = String::new();
        let mut seen = Vec::new();
        let mut first_cursor = String::new();
        loop {
            let page = view.query_events("one", &next, 100).unwrap();
            if let Some(previous) = &page.previous_cursor {
                let back = view.query_events("one", previous, 100).unwrap();
                assert_eq!(back.offset + 100, page.offset);
            }
            seen.extend(page.events.iter().map(|e| e.total_token_usage.total_tokens));
            if let Some(cursor) = page.next_cursor {
                if first_cursor.is_empty() {
                    first_cursor = cursor.clone();
                }
                next = cursor;
            } else {
                break;
            }
        }
        assert_eq!(seen.len(), 10003);
        let mut sorted = seen.clone();
        sorted.sort();
        assert_eq!(sorted, (1..=10003).collect::<Vec<_>>());
        assert_eq!(&seen[..3], &[10003, 10001, 10002]);
        assert!(matches!(
            view.query_events("two", &first_cursor, 100),
            Err(QueryError::Invalid(_))
        ));
        assert!(matches!(
            view.query_events("one", &first_cursor, 50),
            Err(QueryError::Invalid(_))
        ));
        let changed = super::tests::view(10004, 2);
        assert!(matches!(
            changed.query_events("one", &first_cursor, 100),
            Err(QueryError::Stale)
        ));
        assert_eq!(view.query_chart("one").unwrap().timeline.len(), 10003);
    }
    #[test]
    fn metrics_cover_whole_filter_and_cross_source_tasks_stay_separate() {
        let view = view(10, 1);
        let all = view.query_sessions(&Filter::default(), "", 1).unwrap();
        assert_eq!(all.total, 2);
        assert_eq!(all.summary.usage.total_tokens, 20);
        let query = Filter {
            search: "find me".into(),
            ..Filter::default()
        };
        let found = view.query_sessions(&query, "", 100).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.summary.usage.total_tokens, 10);
        assert!(matches!(
            view.query_sessions(&query, all.next_cursor.as_deref().unwrap(), 1),
            Err(QueryError::Invalid(_))
        ));
        let groups = view
            .query_project_tasks(&Filter::default(), "same-id", "", 1)
            .unwrap();
        assert_eq!(groups.total, 2);
        assert_eq!(groups.items[0].sessions.len(), 1);
    }
    #[test]
    fn task_pages_sort_aggregate_usage_without_splitting_children() {
        let make = |id: &str, root: &str, amount: i64, end: &str| {
            let detail = SessionDetail {
                identity: SessionIdentity {
                    uid: id.into(),
                    session_id: id.into(),
                    root_session_id: root.into(),
                    is_subagent: id != root,
                    ..SessionIdentity::default()
                },
                project_root: "/p".into(),
                title: id.into(),
                end_at: end.into(),
                total_token_usage: Usage {
                    total_tokens: amount,
                    ..Usage::default()
                },
                ..SessionDetail::default()
            };
            IndexedSession::from_detail(Arc::new(detail)).unwrap()
        };
        let snapshot = Snapshot {
            revision: 1,
            generated_at: "2026-10-02T00:00:00Z".into(),
            sessions: vec![
                make("root", "root", 10, "2026-10-01T23:30:00+09:00"),
                make("child", "root", 10, "2026-10-01T23:30:00+09:00"),
                make("other", "other", 15, "2026-10-01T15:00:00Z"),
            ],
        };
        let period = Period::bounds(
            "all",
            None,
            None,
            chrono_tz::UTC,
            timestamp::parse(&snapshot.generated_at).unwrap(),
        )
        .unwrap();
        let view = View::new(&snapshot, period, chrono_tz::UTC, &PriceSchedule::bundled()).unwrap();
        let first = view
            .query_project_tasks(&Filter::default(), "/p", "", 1)
            .unwrap();
        assert_eq!(first.items[0].sessions.len(), 2);
        assert_eq!(first.items[0].summary.usage.total_tokens, 20);
        assert_eq!(first.summary.usage.total_tokens, 35);
        let next = view
            .query_project_tasks(
                &Filter::default(),
                "/p",
                first.next_cursor.as_deref().unwrap(),
                1,
            )
            .unwrap();
        assert_eq!(next.items[0].sessions[0].uid, "other");
        let updated = Filter {
            sort: "updated".into(),
            ..Filter::default()
        };
        let sorted = view.query_sessions(&updated, "", 1).unwrap();
        assert_eq!(sorted.items[0].uid, "other");
        let cursor = anchor_cursor(&sorted, 2).unwrap();
        let anchored = view.query_sessions(&updated, &cursor, 1).unwrap();
        assert_eq!(anchored.offset, 2);
        assert_eq!(anchored.total, 3);
    }
}
