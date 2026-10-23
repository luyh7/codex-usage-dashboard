//! Immutable, revision-bound queries. No filesystem, database or host calls.
use crate::{
    Error, PriceSchedule, RawEvent, Usage,
    index::{IndexedSession, Snapshot},
    parser::SessionDetail,
    query::{self, DailyUsage, ModelBreakdown, Period, SessionSummary, Summary},
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
    DetailsMissing,
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
    /// A literal value or a JSON array of alternatives; empty means all.
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
// Keep existing single-value API links while allowing unambiguous alternatives.
fn filter_values(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    if value.starts_with('[')
        && let Ok(values) = serde_json::from_str::<Vec<String>>(value)
    {
        return values
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect();
    }
    vec![value.to_owned()]
}
struct Selection {
    indices: Vec<usize>,
    summary: Summary,
    metric_trend: MetricTrend,
}
struct Entry {
    indexed: IndexedSession,
    summary: SessionSummary,
    search: String,
    event_order: Vec<usize>,
    metric_days: Vec<MetricDay>,
    trend_complete: bool,
    model_breakdown: Vec<ModelBreakdown>,
}

fn add_model_usage(
    rows: &mut BTreeMap<String, ModelBreakdown>,
    model: &str,
    usage: Usage,
    cost: Option<f64>,
    complete: bool,
) -> Result<(), Error> {
    let model = if model.is_empty() { "unknown" } else { model };
    let row = rows.entry(model.into()).or_insert_with(|| ModelBreakdown {
        model: model.into(),
        cost_complete: true,
        ..ModelBreakdown::default()
    });
    row.usage = row.usage.checked_add(usage)?;
    row.estimated_cost_usd = match (row.estimated_cost_usd, cost) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    row.cost_complete &= complete;
    Ok(())
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct MetricTrend {
    pub points: Vec<MetricDay>,
    pub complete: bool,
    pub granularity: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct MetricDay {
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub usage: Usage,
    /// A session is counted once per time bucket in which it has usage.
    pub session_count: usize,
    pub estimated_cost_usd: Option<f64>,
}
pub struct View {
    pub revision: String,
    pub generated_at: String,
    pub period: Period,
    pub daily_usage: Vec<DailyUsage>,
    entries: Vec<Entry>,
    by_uid: HashMap<String, usize>,
    selections: Mutex<HashMap<String, Arc<Selection>>>,
    hourly_bounds: Vec<MetricDay>,
    facets: Option<Summary>,
}
fn hour_bucket(start: DateTime<Utc>, end: DateTime<Utc>, zone: Tz) -> MetricDay {
    MetricDay {
        date: start.with_timezone(&zone).to_rfc3339(),
        end: Some(end.with_timezone(&zone).to_rfc3339()),
        usage: Usage::default(),
        session_count: 0,
        estimated_cost_usd: Some(0.),
    }
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
    pub updated_at: Option<String>,
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
    /// A model-scoped publication keeps list, detail, chart and cursor requests
    /// on the same event-level accounting. Facets retain all available models.
    pub fn with_models(
        snapshot: &Snapshot,
        period: Period,
        zone: Tz,
        prices: &PriceSchedule,
        model_filter: &str,
    ) -> Result<Self, Error> {
        let mut models = filter_values(model_filter);
        let base = Self::new(snapshot, period.clone(), zone, prices)?;
        if models.is_empty() {
            return Ok(base);
        }
        models.sort();
        models.dedup();
        let facets = base.summary().map_err(|error| match error {
            QueryError::Core(error) => error,
            _ => Error::InvalidData("cannot summarize model facets".into()),
        })?;
        let mut sessions = Vec::new();
        for entry in &base.entries {
            if entry.indexed.has_detail && !entry.indexed.events.is_empty() {
                let detail = query::detail_for_models_indexed(&entry.indexed, &models, prices)?;
                if detail.total_token_usage.has_tokens() {
                    // The projected fields supersede optional legacy wire data.
                    sessions.push(IndexedSession::from_detail(Arc::new(detail))?);
                }
            } else {
                // A summary-only mixed-model export cannot be split reliably.
                // Include it only when its entire attribution is known.
                let row = &entry.summary;
                let attribution = if row.models.is_empty() {
                    vec![if row.model.is_empty() {
                        "unknown".into()
                    } else {
                        row.model.clone()
                    }]
                } else {
                    row.models.clone()
                };
                if attribution.len() == 1 && models.contains(&attribution[0]) {
                    sessions.push(entry.indexed.clone());
                }
            }
        }
        let mut view = Self::new(
            &Snapshot {
                revision: snapshot.revision,
                generated_at: snapshot.generated_at.clone(),
                sessions,
            },
            period,
            zone,
            prices,
        )?;
        // The model list describes the conversation. Only the usage timeline
        // and its derived totals are narrowed by the model filter.
        for entry in &mut view.entries {
            let original = &base.entries[base.by_uid[&entry.summary.uid]];
            entry.summary.models = original.summary.models.clone();
            Arc::make_mut(&mut entry.indexed.detail).models = original.summary.models.clone();
            entry.search.push(' ');
            entry
                .search
                .push_str(&original.summary.models.join(" ").to_lowercase());
        }
        view.revision = hash(&(&base.revision, &models))?;
        view.facets = Some(facets);
        Ok(view)
    }
    pub fn facets(&self) -> Result<Summary, QueryError> {
        self.facets
            .clone()
            .map(Ok)
            .unwrap_or_else(|| self.summary())
    }
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
        let hourly_range = period
            .range()
            .filter(|_| period.start_date.is_some() && period.start_date == period.end_date);
        let mut hourly_bounds = Vec::new();
        if let Some((mut start, end)) = hourly_range {
            while start <= end {
                let next = start + chrono::Duration::hours(1);
                // Historical ranges end one microsecond before the next midnight.
                let stop = if end - start < chrono::Duration::hours(1)
                    && end + chrono::Duration::microseconds(1) != next
                {
                    end
                } else {
                    next
                };
                hourly_bounds.push(hour_bucket(start, stop, zone));
                start = next;
            }
        }
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
            let mut metric_days = BTreeMap::<String, MetricDay>::new();
            let mut model_breakdown = BTreeMap::new();
            for event in indexed.events.iter() {
                if event.delta.total_tokens <= 0 {
                    continue;
                }
                let Some(time) = DateTime::<Utc>::from_timestamp_micros(event.timestamp_us) else {
                    continue;
                };
                let date = time.with_timezone(&zone).date_naive().to_string();
                let bucket = if let Some((start, _)) = hourly_range {
                    let index = (time - start).num_hours() as usize;
                    hourly_bounds
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| hour_bucket(start, time, zone))
                } else {
                    MetricDay {
                        date: date.clone(),
                        end: None,
                        usage: Usage::default(),
                        session_count: 0,
                        estimated_cost_usd: Some(0.),
                    }
                };
                let day = metric_days
                    .entry(bucket.date.clone())
                    .or_insert_with(|| MetricDay {
                        session_count: 1,
                        ..bucket
                    });
                day.usage = day.usage.checked_add(event.delta)?;
                let source = &indexed.detail.timeline[event.source_order];
                let model = if source.model.is_empty() {
                    &indexed.detail.model
                } else {
                    &source.model
                };
                let cost = prices
                    .quote(
                        model,
                        source.timestamp.as_deref(),
                        Some(event.delta.input_tokens.max(0)),
                        &source.service_tier,
                    )
                    .map(|quote| {
                        crate::pricing::CostParts::for_usage(event.delta, quote.prices).total()
                    });
                add_model_usage(
                    &mut model_breakdown,
                    model,
                    event.delta,
                    cost,
                    cost.is_some(),
                )?;
                day.estimated_cost_usd = day
                    .estimated_cost_usd
                    .zip(cost)
                    .map(|(total, cost)| total + cost);
                let total = days.entry(date).or_default();
                *total = total.checked_add(event.delta)?;
            }
            let detail = &indexed.detail;
            let mut summary = indexed
                .summary
                .as_ref()
                .map(|s| s.as_ref().clone())
                .unwrap_or_else(|| SessionSummary::from(detail.as_ref()));
            if indexed.has_detail {
                summary.last_usage_at = indexed
                    .events
                    .iter()
                    .filter(|event| event.delta.has_tokens())
                    .map(|event| event.timestamp_us)
                    .max()
                    .and_then(DateTime::<Utc>::from_timestamp_micros)
                    .map(timestamp::format);
            }
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
            let trend_complete = metric_days
                .values()
                .map(|day| i128::from(day.usage.total_tokens))
                .sum::<i128>()
                == i128::from(summary.total_token_usage.total_tokens);
            // Old remote exports may contain only summary data. Never attribute
            // an unresolvable multi-model total to the last model in the session.
            if !trend_complete {
                model_breakdown.clear();
                let model = if summary.models.len() > 1 {
                    "unknown"
                } else {
                    &summary.model
                };
                add_model_usage(
                    &mut model_breakdown,
                    model,
                    summary.total_token_usage,
                    summary.estimated_cost_usd,
                    summary.estimated_cost_usd.is_some(),
                )?;
            }
            entries.push(Entry {
                indexed,
                summary,
                search,
                event_order,
                metric_days: metric_days.into_values().collect(),
                trend_complete,
                model_breakdown: model_breakdown.into_values().collect(),
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
            hourly_bounds,
            facets: None,
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
                value["last_usage_at"] = serde_json::to_value(&entry.summary.last_usage_at)?;
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
    pub fn query_metric_trend(&self, query: &Filter) -> Result<MetricTrend, QueryError> {
        Ok(self.selection(query)?.metric_trend.clone())
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
        let models = filter_values(&query.model);
        let environments = filter_values(&query.environment);
        let mut indices: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let s = &entry.summary;
                (search.is_empty() || entry.search.contains(&search))
                    && (models.is_empty()
                        || models
                            .iter()
                            .any(|model| s.model == *model || s.models.contains(model)))
                    && (environments.is_empty() || environments.contains(&s.environment_id))
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
        let mut summary = query::summarize(indices.iter().map(|i| &self.entries[*i].summary))?;
        let mut models = BTreeMap::new();
        let mut model_sessions = BTreeMap::<String, usize>::new();
        for i in &indices {
            for row in &self.entries[*i].model_breakdown {
                *model_sessions.entry(row.model.clone()).or_default() += 1;
                add_model_usage(
                    &mut models,
                    &row.model,
                    row.usage,
                    row.estimated_cost_usd,
                    row.cost_complete,
                )?;
            }
        }
        summary.model_breakdown = models
            .into_values()
            .map(|mut row| {
                row.estimated_cost_usd = row.estimated_cost_usd.map(crate::pricing::round_money);
                row
            })
            .collect();
        summary.by_model = summary
            .model_breakdown
            .iter()
            .map(|row| query::ModelUsage {
                model: row.model.clone(),
                sessions: model_sessions[&row.model],
                usage: row.usage,
            })
            .collect();
        summary
            .by_model
            .sort_by_key(|row| Reverse(row.usage.total_tokens));
        let mut days: BTreeMap<String, MetricDay> = self
            .hourly_bounds
            .iter()
            .map(|hour| (hour.date.clone(), hour.clone()))
            .collect();
        let mut complete = true;
        for i in &indices {
            let entry = &self.entries[*i];
            complete &= entry.trend_complete;
            for day in &entry.metric_days {
                let total = days.entry(day.date.clone()).or_insert_with(|| MetricDay {
                    date: day.date.clone(),
                    end: day.end.clone(),
                    usage: Usage::default(),
                    session_count: 0,
                    estimated_cost_usd: Some(0.),
                });
                total.usage = total.usage.checked_add(day.usage)?;
                total.session_count += day.session_count;
                total.estimated_cost_usd = total
                    .estimated_cost_usd
                    .zip(day.estimated_cost_usd)
                    .map(|(a, b)| a + b);
            }
        }
        let mut points: Vec<_> = days.into_values().collect();
        if !self.hourly_bounds.is_empty() {
            // Offset strings alone do not sort repeated DST hours chronologically.
            points.sort_by_key(|point| timestamp::parse(&point.date));
        }
        let metric_trend = MetricTrend {
            complete,
            granularity: if self.hourly_bounds.is_empty() {
                "day"
            } else {
                "hour"
            },
            points: points
                .into_iter()
                .map(|mut day| {
                    day.estimated_cost_usd =
                        day.estimated_cost_usd.map(crate::pricing::round_money);
                    day
                })
                .collect(),
        };
        let result = Arc::new(Selection {
            indices,
            summary,
            metric_trend,
        });
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
                    updated_at: None,
                    sort_time: 0,
                });
            }
            let row = &mut rows[index];
            row.session_count += 1;
            let updated = row_time(session);
            if updated >= row.sort_time {
                row.sort_time = updated;
                row.updated_at = DateTime::<Utc>::from_timestamp_millis(updated)
                    .filter(|_| updated > 0)
                    .map(|time| time.to_rfc3339());
            }
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
    /// Checks the complete filtered selection, independent of pagination/grouping.
    pub fn session_matches(&self, query: &Filter, id: &str) -> Result<bool, QueryError> {
        let selection = self.selection(query)?;
        Ok(self
            .by_uid
            .get(id)
            .is_some_and(|i| selection.indices.contains(i)))
    }
    fn entry(&self, id: &str) -> Result<&Entry, QueryError> {
        let entry = self
            .by_uid
            .get(id)
            .map(|i| &self.entries[*i])
            .ok_or(QueryError::Missing)?;
        if !entry.indexed.has_detail {
            return Err(QueryError::DetailsMissing);
        }
        Ok(entry)
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
    row.last_usage_at
        .as_deref()
        .and_then(timestamp::parse)
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
    #[test]
    fn model_breakdown_attributes_switches_tiers_unknowns_and_date_filters() {
        let prices = PriceSchedule::bundled();
        let timeline: Vec<_> = [
            ("2026-10-21T15:00:00Z", "gpt-6.1-sol", "standard", 1000),
            ("2026-10-21T16:30:00Z", "gpt-6-astra", "fast", 3000),
            ("2026-10-21T17:00:00Z", "gpt-6.1-sol", "ultrafast", 6000),
            ("2026-10-21T17:30:00Z", "unpriced-model", "standard", 6100),
        ]
        .into_iter()
        .map(|(time, model, tier, input)| RawEvent {
            timestamp: Some(time.into()),
            model: model.into(),
            service_tier: tier.into(),
            total_token_usage: Usage {
                input_tokens: input,
                output_tokens: input / 10,
                total_tokens: input + input / 10,
                ..Usage::default()
            },
            ..RawEvent::default()
        })
        .collect();
        let usage = timeline.last().unwrap().total_token_usage;
        let pricing = prices
            .price_events(
                &crate::usage::normalize(&timeline).unwrap(),
                "unpriced-model",
                usage,
                None,
                "standard",
            )
            .unwrap();
        let detail = SessionDetail {
            identity: SessionIdentity {
                uid: "mixed".into(),
                session_id: "mixed".into(),
                ..SessionIdentity::default()
            },
            model: "unpriced-model".into(),
            models: vec![
                "gpt-6.1-sol".into(),
                "gpt-6-astra".into(),
                "unpriced-model".into(),
            ],
            title: "Mixed models".into(),
            total_token_usage: usage,
            timeline,
            pricing,
            ..SessionDetail::default()
        };
        let mut metadata = IndexedSession::from_detail(Arc::new(detail.metadata())).unwrap();
        metadata.has_detail = false;
        metadata.detail = Arc::new(SessionDetail {
            identity: SessionIdentity {
                uid: "remote".into(),
                ..metadata.detail.identity.clone()
            },
            ..metadata.detail.as_ref().clone()
        });
        let snapshot = Snapshot {
            revision: 1,
            generated_at: "2026-10-21T18:00:00Z".into(),
            sessions: vec![
                IndexedSession::from_detail(Arc::new(detail)).unwrap(),
                metadata,
            ],
        };
        let zone = chrono_tz::Asia::Shanghai;
        let now = timestamp::parse(&snapshot.generated_at).unwrap();
        let all = View::new(
            &snapshot,
            Period::bounds("all", None, None, zone, now).unwrap(),
            zone,
            &prices,
        )
        .unwrap();
        let filter = Filter {
            search: "Mixed models".into(),
            ..Filter::default()
        };
        let summary = all.query_sessions(&filter, "", 1).unwrap().summary;
        let rows = &summary.model_breakdown;
        assert_eq!(summary.session_count, 2); // Pagination must not truncate totals.
        assert_eq!(
            rows.iter().map(|r| r.usage.total_tokens).sum::<i64>(),
            13420
        );
        let sol = rows.iter().find(|r| r.model == "gpt-6.1-sol").unwrap();
        assert_eq!(sol.usage.total_tokens, 4400);
        let expected = |model, time, tier, input| {
            let quote = prices.quote(model, Some(time), Some(input), tier).unwrap();
            crate::pricing::CostParts::for_usage(
                Usage {
                    input_tokens: input,
                    output_tokens: input / 10,
                    ..Usage::default()
                },
                quote.prices,
            )
            .total()
        };
        let sol_cost = expected("gpt-6.1-sol", "2026-10-21T15:00:00Z", "standard", 1000)
            + expected("gpt-6.1-sol", "2026-10-21T17:00:00Z", "ultrafast", 3000);
        assert!((sol.estimated_cost_usd.unwrap() - sol_cost).abs() < 0.000001);
        assert!(sol.cost_complete);
        let period = Period::bounds("all", None, None, zone, now).unwrap();
        let scoped =
            View::with_models(&snapshot, period.clone(), zone, &prices, "gpt-6.1-sol").unwrap();
        let selected = scoped.query_sessions(&Filter::default(), "", 100).unwrap();
        assert_ne!(scoped.revision, all.revision);
        assert_eq!(scoped.facets().unwrap().session_count, 2);
        assert_eq!(
            scoped
                .facets()
                .unwrap()
                .by_model
                .iter()
                .find(|row| row.model == "gpt-6.1-sol")
                .unwrap()
                .usage
                .total_tokens,
            4400
        );
        assert_eq!(selected.summary.session_count, 1);
        assert_eq!(selected.summary.usage.total_tokens, 4400);
        assert_eq!(selected.summary.usage.output_tokens, 400);
        assert!((selected.summary.estimated_cost_usd - sol_cost).abs() < 0.000001);
        assert_eq!(
            selected.items[0].models,
            all.query_session_detail("mixed").unwrap().models
        );
        assert_eq!(selected.summary.model_breakdown.len(), 1);
        assert_eq!(selected.summary.model_breakdown[0].model, "gpt-6.1-sol");
        assert_eq!(selected.items[0].total_token_usage.total_tokens, 4400);
        assert!(selected.items[0].price_model_known); // Unselected unknown prices do not leak.
        let detail = scoped.query_session_detail("mixed").unwrap();
        assert_eq!(detail.models, selected.items[0].models);
        assert!(
            detail
                .timeline
                .iter()
                .all(|event| event.model == "gpt-6.1-sol")
        );
        let searched = scoped
            .query_sessions(
                &Filter {
                    search: "gpt-6-astra".into(),
                    ..Filter::default()
                },
                "",
                100,
            )
            .unwrap();
        assert_eq!(searched.summary.usage.total_tokens, 4400);
        assert_eq!(
            detail
                .timeline
                .iter()
                .map(|e| e.total_token_usage.total_tokens)
                .collect::<Vec<_>>(),
            vec![1100, 4400]
        );
        assert_eq!(
            detail.timeline[1].last_token_usage.unwrap().total_tokens,
            3300
        );
        assert_eq!(scoped.query_events("mixed", "", 100).unwrap().total, 2);
        assert_eq!(
            scoped
                .query_chart("mixed")
                .unwrap()
                .timeline
                .last()
                .unwrap()
                .total_token_usage
                .total_tokens,
            4400
        );
        let trend = scoped.query_metric_trend(&Filter::default()).unwrap();
        assert!(trend.complete);
        assert_eq!(
            trend
                .points
                .iter()
                .map(|p| p.usage.total_tokens)
                .sum::<i64>(),
            4400
        );
        assert_eq!(
            scoped
                .query_projects(&Filter::default(), "", 100)
                .unwrap()
                .items[0]
                .usage
                .total_tokens,
            4400
        );
        assert_eq!(
            scoped
                .query_project_tasks(&Filter::default(), "mixed", "", 100)
                .unwrap()
                .items[0]
                .summary
                .usage
                .total_tokens,
            4400
        );
        let multi = View::with_models(
            &snapshot,
            period.clone(),
            zone,
            &prices,
            r#"["gpt-6-astra","gpt-6.1-sol","gpt-6-astra"]"#,
        )
        .unwrap();
        assert_eq!(multi.summary().unwrap().usage.total_tokens, 6600);
        assert_eq!(
            multi.query_session_detail("mixed").unwrap().timeline.len(),
            3
        );
        let reverse = View::with_models(
            &snapshot,
            period.clone(),
            zone,
            &prices,
            r#"["gpt-6.1-sol","gpt-6-astra"]"#,
        )
        .unwrap();
        assert_eq!(multi.revision, reverse.revision);
        let today_scoped = View::with_models(
            &snapshot,
            Period::bounds("today", None, None, zone, now).unwrap(),
            zone,
            &prices,
            "gpt-6.1-sol",
        )
        .unwrap();
        assert_eq!(today_scoped.summary().unwrap().usage.total_tokens, 3300);
        assert_eq!(
            today_scoped
                .query_metric_trend(&Filter::default())
                .unwrap()
                .granularity,
            "hour"
        );
        let empty = View::with_models(&snapshot, period.clone(), zone, &prices, "absent").unwrap();
        assert_eq!(empty.summary().unwrap().session_count, 0);
        let reset = View::with_models(&snapshot, period, zone, &prices, "[]").unwrap();
        assert_eq!(reset.revision, all.revision);
        assert_eq!(reset.summary().unwrap().usage.total_tokens, 13420);
        assert_eq!(
            rows.iter()
                .find(|r| r.model == "unpriced-model")
                .unwrap()
                .estimated_cost_usd,
            None
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.model == "unknown")
                .unwrap()
                .usage
                .total_tokens,
            6710
        );
        let today = View::new(
            &snapshot,
            Period::bounds("today", None, None, zone, now).unwrap(),
            zone,
            &prices,
        )
        .unwrap();
        let summary = today.summary().unwrap();
        assert_eq!(
            summary
                .model_breakdown
                .iter()
                .map(|r| r.usage.total_tokens)
                .sum::<i64>(),
            5610
        );
        assert_eq!(
            summary
                .model_breakdown
                .iter()
                .find(|r| r.model == "gpt-6.1-sol")
                .unwrap()
                .usage
                .total_tokens,
            3300
        );
        assert_eq!(
            today
                .query_sessions(
                    &Filter {
                        search: "no-match".into(),
                        ..Filter::default()
                    },
                    "",
                    1
                )
                .unwrap()
                .summary
                .model_breakdown
                .len(),
            0
        );
        let mut combined = BTreeMap::new();
        add_model_usage(&mut combined, "mixed", Usage::default(), Some(1.), true).unwrap();
        add_model_usage(&mut combined, "mixed", Usage::default(), None, false).unwrap();
        assert_eq!(combined["mixed"].estimated_cost_usd, Some(1.));
        assert!(!combined["mixed"].cost_complete);
    }
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
    fn selected_session_checks_all_filtered_rows_and_distinguishes_missing_details() {
        let mut view = view(5, 1);
        let filter = Filter {
            search: "other".into(),
            ..Default::default()
        };
        assert!(!view.session_matches(&filter, "one").unwrap());
        assert!(view.session_matches(&filter, "two").unwrap());
        let filter = Filter::default();
        let first = view.query_sessions(&filter, "", 1).unwrap().items[0]
            .uid
            .clone();
        let other = if first == "one" { "two" } else { "one" };
        assert!(view.session_matches(&filter, other).unwrap());
        assert!(!view.session_matches(&filter, "absent").unwrap());
        view.entries[*view.by_uid.get("one").unwrap()]
            .indexed
            .has_detail = false;
        assert!(view.session_matches(&filter, "one").unwrap());
        assert!(matches!(
            view.query_session_detail("one"),
            Err(QueryError::DetailsMissing)
        ));
        assert!(matches!(
            view.query_session_detail("absent"),
            Err(QueryError::Missing)
        ));
    }
    #[test]
    fn multi_select_filters_union_within_groups_intersect_across_groups() {
        let prices = PriceSchedule::bundled();
        let snapshot = Snapshot {
            revision: 1,
            generated_at: "2026-10-01T01:00:00Z".into(),
            sessions: [
                ("a", "local", "gpt-5", 100),
                ("b", "wsl", "gpt-6", 200),
                ("c", "remote", "gpt-6", 400),
                ("d", "wsl", "other", 800),
            ]
            .into_iter()
            .map(|(id, environment, model, tokens)| {
                let usage = Usage {
                    total_tokens: tokens,
                    ..Usage::default()
                };
                IndexedSession::from_detail(Arc::new(SessionDetail {
                    identity: SessionIdentity {
                        uid: id.into(),
                        session_id: id.into(),
                        ..SessionIdentity::default()
                    },
                    model: model.into(),
                    environment_id: environment.into(),
                    cwd: id.into(),
                    total_token_usage: usage,
                    timeline: vec![RawEvent {
                        timestamp: Some("2026-10-01T00:00:00Z".into()),
                        model: model.into(),
                        total_token_usage: usage,
                        ..RawEvent::default()
                    }],
                    ..SessionDetail::default()
                }))
                .unwrap()
            })
            .collect(),
        };
        let view = View::new(
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
            &prices,
        )
        .unwrap();
        let filter = Filter {
            model: r#"["gpt-5","gpt-6"]"#.into(),
            environment: r#"["local","wsl"]"#.into(),
            ..Filter::default()
        };
        let sessions = view.query_sessions(&filter, "", 100).unwrap();
        assert_eq!(sessions.total, 2);
        assert_eq!(sessions.summary.usage.total_tokens, 300);
        assert_eq!(
            view.query_projects(&filter, "", 100)
                .unwrap()
                .summary
                .usage
                .total_tokens,
            300
        );
        assert_eq!(
            view.query_metric_trend(&filter)
                .unwrap()
                .points
                .iter()
                .map(|p| p.usage.total_tokens)
                .sum::<i64>(),
            300
        );
        let one = Filter {
            model: "gpt-6".into(),
            environment: "wsl".into(),
            ..Filter::default()
        };
        assert_eq!(
            view.query_sessions(&one, "", 100)
                .unwrap()
                .summary
                .usage
                .total_tokens,
            200
        );
        let empty = Filter {
            model: "[]".into(),
            environment: "[]".into(),
            ..Filter::default()
        };
        assert_eq!(view.query_sessions(&empty, "", 100).unwrap().total, 4);
        assert_eq!(
            filter_values(r#"["model,with,commas","other"]"#),
            vec!["model,with,commas", "other"]
        );
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
    fn metric_trends_follow_events_timezone_filters_and_whole_selection() {
        let prices = PriceSchedule::bundled();
        let make = |id: &str, model: &str, rows: &[(&str, i64)]| {
            let timeline: Vec<_> = rows
                .iter()
                .map(|(stamp, tokens)| RawEvent {
                    timestamp: Some((*stamp).into()),
                    model: model.into(),
                    service_tier: "fast".into(),
                    total_token_usage: Usage {
                        input_tokens: *tokens,
                        total_tokens: *tokens,
                        ..Usage::default()
                    },
                    ..RawEvent::default()
                })
                .collect();
            let usage = timeline.last().unwrap().total_token_usage;
            let pricing = prices
                .price_events(
                    &crate::usage::normalize(&timeline).unwrap(),
                    model,
                    usage,
                    None,
                    "fast",
                )
                .unwrap();
            IndexedSession::from_detail(Arc::new(SessionDetail {
                identity: SessionIdentity {
                    uid: id.into(),
                    session_id: id.into(),
                    ..SessionIdentity::default()
                },
                title: id.into(),
                model: model.into(),
                timeline,
                total_token_usage: usage,
                pricing,
                ..SessionDetail::default()
            }))
            .unwrap()
        };
        let snapshot = Snapshot {
            revision: 1,
            generated_at: "2026-10-21T18:00:00Z".into(),
            sessions: vec![
                make(
                    "Alpha",
                    "gpt-6.1-sol",
                    &[
                        ("2026-10-21T15:30:00Z", 1000),
                        ("2026-10-21T16:30:00Z", 2000),
                        ("2026-10-21T17:30:00Z", 3000),
                    ],
                ),
                make("Beta", "gpt-6.1-sol", &[("2026-10-21T16:45:00Z", 500)]),
                make(
                    "Unknown",
                    "not-a-priced-model",
                    &[("2026-10-21T16:50:00Z", 100)],
                ),
            ],
        };
        let zone = chrono_tz::Asia::Shanghai;
        let now = timestamp::parse(&snapshot.generated_at).unwrap();
        let view = View::new(
            &snapshot,
            Period::bounds("all", None, None, zone, now).unwrap(),
            zone,
            &prices,
        )
        .unwrap();
        let all = view.query_metric_trend(&Filter::default()).unwrap();
        assert!(all.complete);
        assert_eq!(
            all.points
                .iter()
                .map(|day| day.usage.total_tokens)
                .sum::<i64>(),
            3600
        );
        assert_eq!(all.points[0].date, "2026-10-21");
        assert_eq!(all.points[1].date, "2026-10-22");
        assert_eq!(all.points[1].session_count, 3); // Alpha's two events count as one active session.
        assert_eq!(all.points[1].estimated_cost_usd, None);
        let filter = Filter {
            search: "Alpha".into(),
            ..Filter::default()
        };
        let alpha = view.query_metric_trend(&filter).unwrap();
        assert_eq!(alpha.points[1].usage.total_tokens, 2000);
        assert_eq!(alpha.points[1].session_count, 1);
        let total = alpha
            .points
            .iter()
            .map(|day| day.estimated_cost_usd.unwrap())
            .sum::<f64>();
        assert!(
            (total
                - view
                    .query_sessions(&filter, "", 1)
                    .unwrap()
                    .summary
                    .estimated_cost_usd)
                .abs()
                < 0.000001
        );
        let today = View::new(
            &snapshot,
            Period::bounds("today", None, None, zone, now).unwrap(),
            zone,
            &prices,
        )
        .unwrap();
        let daily = today.query_metric_trend(&Filter::default()).unwrap();
        assert_eq!(daily.granularity, "hour");
        assert_eq!(daily.points.len(), 3);
        assert_eq!(daily.points[0].date, "2026-10-22T00:00:00+08:00");
        assert_eq!(
            daily.points[0].end.as_deref(),
            Some("2026-10-22T01:00:00+08:00")
        );
        assert_eq!(daily.points[0].usage.total_tokens, 1600);
        assert_eq!(daily.points[0].session_count, 3);
        assert_eq!(daily.points[0].estimated_cost_usd, None);
        assert_eq!(daily.points[1].usage.total_tokens, 1000);
        assert_eq!(daily.points[2].usage.total_tokens, 0);
        let alpha_hourly = today.query_metric_trend(&filter).unwrap();
        assert_eq!(
            alpha_hourly
                .points
                .iter()
                .map(|hour| hour.usage.total_tokens)
                .sum::<i64>(),
            2000
        );
        assert_eq!(alpha_hourly.points[0].session_count, 1);
        assert!(
            alpha_hourly
                .points
                .iter()
                .all(|hour| hour.estimated_cost_usd.is_some())
        );
        let history = View::new(
            &snapshot,
            Period::bounds("custom", Some("2026-10-21"), Some("2026-10-21"), zone, now).unwrap(),
            zone,
            &prices,
        )
        .unwrap();
        let history = history.query_metric_trend(&Filter::default()).unwrap();
        assert_eq!(history.points.len(), 24);
        assert_eq!(history.points[23].usage.total_tokens, 1000);
        assert_eq!(
            history.points[23].end.as_deref(),
            Some("2026-10-22T00:00:00+08:00")
        );
        assert_eq!(all.granularity, "day");
        // A repeated local hour must retain separate, chronologically ordered buckets.
        let dst_snapshot = Snapshot {
            revision: 2,
            generated_at: "2026-10-26T12:00:00Z".into(),
            sessions: vec![make(
                "DST",
                "gpt-6.1-sol",
                &[("2026-10-25T00:30:00Z", 100), ("2026-10-25T01:30:00Z", 300)],
            )],
        };
        for (zone, date, hours) in [
            (chrono_tz::Europe::Berlin, "2026-10-25", 25),
            (chrono_tz::Europe::Berlin, "2026-03-29", 23),
            (chrono_tz::Asia::Kolkata, "2026-10-25", 24),
        ] {
            let now = timestamp::parse(&dst_snapshot.generated_at).unwrap();
            let dst = View::new(
                &dst_snapshot,
                Period::bounds("custom", Some(date), Some(date), zone, now).unwrap(),
                zone,
                &prices,
            )
            .unwrap();
            let trend = dst.query_metric_trend(&Filter::default()).unwrap();
            assert_eq!(trend.points.len(), hours);
            assert!(
                trend
                    .points
                    .windows(2)
                    .all(|rows| timestamp::parse(&rows[0].date) < timestamp::parse(&rows[1].date))
            );
            if hours == 25 {
                assert_eq!(trend.points[2].usage.total_tokens, 100);
                assert_eq!(trend.points[3].usage.total_tokens, 200);
                assert_eq!(trend.points[2].date, "2026-10-25T02:00:00+02:00");
                assert_eq!(trend.points[3].date, "2026-10-25T02:00:00+01:00");
            }
        }
        let missing = view
            .query_metric_trend(&Filter {
                search: "no match".into(),
                ..Filter::default()
            })
            .unwrap();
        assert!(missing.points.is_empty());
    }
    #[test]
    fn metrics_cover_whole_filter_and_cross_source_tasks_stay_separate() {
        let view = view(10, 1);
        let all = view.query_sessions(&Filter::default(), "", 1).unwrap();
        assert_eq!(all.total, 2);
        assert_eq!(all.summary.usage.total_tokens, 20);
        assert_eq!(all.items.len(), 1);
        assert_eq!(all.summary.main_count, 2);
        assert_eq!(all.summary.agent_count, 0);
        let query = Filter {
            search: "find me".into(),
            ..Filter::default()
        };
        let found = view.query_sessions(&query, "", 100).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.summary.usage.total_tokens, 10);
        assert_eq!(found.summary.main_count, 1);
        assert_eq!(found.summary.agent_count, 0);
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
    fn recent_usage_ignores_log_updates_and_zero_deltas_in_every_period() {
        let make = |id: &str, project: &str, time: &str| {
            let usage = Usage {
                total_tokens: 100,
                ..Usage::default()
            };
            IndexedSession::from_detail(Arc::new(SessionDetail {
                identity: SessionIdentity {
                    uid: id.into(),
                    session_id: id.into(),
                    root_session_id: id.into(),
                    ..SessionIdentity::default()
                },
                project: project.into(),
                project_root: project.into(),
                // A settings update after the last usage must never affect recency.
                end_at: "2026-10-22T06:36:38Z".into(),
                updated_at: Some("2026-10-22T06:36:38Z".into()),
                total_token_usage: usage,
                timeline: vec![
                    RawEvent {
                        timestamp: Some(time.into()),
                        total_token_usage: usage,
                        ..RawEvent::default()
                    },
                    RawEvent {
                        timestamp: Some("2026-10-22T06:00:00Z".into()),
                        total_token_usage: usage,
                        ..RawEvent::default()
                    },
                ],
                ..SessionDetail::default()
            }))
            .unwrap()
        };
        let mut empty = make("empty", "/empty", "2026-10-22T07:00:00Z");
        empty.detail = Arc::new(SessionDetail {
            timeline: Vec::new(),
            total_token_usage: Usage::default(),
            ..empty.detail.as_ref().clone()
        });
        empty.events = Arc::new(Vec::new());
        let snapshot = Snapshot {
            revision: 1,
            generated_at: "2026-10-22T07:20:00Z".into(),
            sessions: vec![
                make("old", "/old", "2026-10-21T15:00:00Z"),
                make("example", "/example", "2026-10-21T16:42:35.828Z"),
                make("newer", "/newer", "2026-10-22T01:00:00+08:00"),
                empty,
            ],
        };
        let filter = Filter {
            sort: "updated".into(),
            ..Filter::default()
        };
        let now = timestamp::parse(&snapshot.generated_at).unwrap();
        let zone = chrono_tz::Asia::Shanghai;
        for period in ["all", "today"] {
            let view = View::new(
                &snapshot,
                Period::bounds(period, None, None, zone, now).unwrap(),
                zone,
                &PriceSchedule::bundled(),
            )
            .unwrap();
            let sessions = view.query_sessions(&filter, "", 100).unwrap();
            assert_eq!(sessions.items[0].uid, "newer");
            let example = sessions.items.iter().find(|s| s.uid == "example").unwrap();
            assert_eq!(
                example.last_usage_at.as_deref().and_then(timestamp::parse),
                timestamp::parse("2026-10-21T16:42:35.828Z")
            );
            assert_eq!(example.total_token_usage.total_tokens, 100);
            let projects = view.query_projects(&filter, "", 100).unwrap();
            assert_eq!(projects.items[0].key, "/newer");
            let project = projects.items.iter().find(|p| p.key == "/example").unwrap();
            assert_eq!(
                timestamp::parse(project.updated_at.as_deref().unwrap()),
                timestamp::parse(example.last_usage_at.as_deref().unwrap())
            );
            let tasks = view
                .query_project_tasks(&filter, "/example", "", 100)
                .unwrap();
            assert_eq!(
                tasks.items[0].sessions[0].last_usage_at,
                example.last_usage_at
            );
            let wire = view.wire_summaries().unwrap();
            assert_eq!(
                wire.iter().find(|s| s["uid"] == "example").unwrap()["last_usage_at"],
                example.last_usage_at.as_deref().unwrap()
            );
            let old = sessions.items.iter().find(|s| s.uid == "old");
            if period == "all" {
                assert_eq!(
                    old.unwrap().last_usage_at.as_deref(),
                    Some("2026-10-21T15:00:00Z")
                );
                assert!(
                    sessions
                        .items
                        .iter()
                        .find(|s| s.uid == "empty")
                        .unwrap()
                        .last_usage_at
                        .is_none()
                );
            } else {
                // A repeated usage counter today does not constitute new usage.
                assert!(old.unwrap().last_usage_at.is_none());
            }
        }
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
                timeline: vec![RawEvent {
                    timestamp: Some(end.into()),
                    total_token_usage: Usage {
                        total_tokens: amount,
                        ..Usage::default()
                    },
                    ..RawEvent::default()
                }],
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
        let projects = view.query_projects(&updated, "", 10).unwrap();
        let latest = projects.items[0].updated_at.as_deref().unwrap();
        assert_eq!(
            timestamp::parse(latest),
            timestamp::parse("2026-10-01T15:00:00Z")
        );
        assert_eq!(
            serde_json::to_value(&projects.items[0]).unwrap()["updated_at"],
            latest
        );
        let cursor = anchor_cursor(&sorted, 2).unwrap();
        let anchored = view.query_sessions(&updated, &cursor, 1).unwrap();
        assert_eq!(anchored.offset, 2);
        assert_eq!(anchored.total, 3);
    }
}
