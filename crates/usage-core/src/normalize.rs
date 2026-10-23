//! Cross-file normalization. Call with an entire dependency component: changing
//! a parent invalidates its descendants, not merely the file that was appended.
use crate::{
    Error, NormalizedEvent, PriceSchedule, Pricing, RawEvent, Usage,
    parser::{ParsedFile, SessionDetail, TierEvent},
    timestamp, usage,
};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

type Identity = (String, String);
fn identity(file: &ParsedFile) -> Identity {
    (
        file.detail.environment_id.clone(),
        file.detail.identity.session_id.clone(),
    )
}
fn parent_id(file: &ParsedFile) -> &str {
    if !file.detail.identity.forked_from_id.is_empty() {
        &file.detail.identity.forked_from_id
    } else {
        &file.detail.identity.parent_thread_id
    }
}
fn unique(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !values.iter().any(|v| v == value) {
        values.push(value.into());
    }
}
fn parsed(value: &str) -> DateTime<Utc> {
    timestamp::parse(value).unwrap_or(DateTime::<Utc>::MAX_UTC)
}
fn update_time(current: &mut String, incoming: &str, latest: bool) {
    let Some(moment) = timestamp::parse(incoming) else {
        return;
    };
    if timestamp::parse(current).is_none_or(|previous| {
        if latest {
            moment > previous
        } else {
            moment < previous
        }
    }) {
        *current = incoming.into();
    }
}
fn cache_percent(usage: Usage) -> Option<f64> {
    if usage.input_tokens == 0 {
        None
    } else {
        Some(
            format!(
                "{:.1}",
                usage.cached_input_tokens as f64 / usage.input_tokens as f64 * 100.
            )
            .parse()
            .unwrap(),
        )
    }
}
fn task_totals(detail: &mut SessionDetail) -> Result<(), Error> {
    let durations: Vec<_> = detail
        .tasks
        .iter()
        .filter_map(|task| task.duration_ms)
        .collect();
    let ttfs: Vec<_> = detail
        .tasks
        .iter()
        .filter_map(|task| task.time_to_first_token_ms)
        .collect();
    fn sum(values: &[i64]) -> Result<i64, Error> {
        values.iter().try_fold(0i64, |sum, value| {
            sum.checked_add(*value).ok_or(Error::CounterOverflow)
        })
    }
    detail.duration_ms_total = sum(&durations)?;
    detail.duration_ms_avg = if durations.is_empty() {
        None
    } else {
        Some((detail.duration_ms_total as f64 / durations.len() as f64) as i64)
    };
    detail.time_to_first_token_ms_avg = if ttfs.is_empty() {
        None
    } else {
        Some((sum(&ttfs)? as f64 / ttfs.len() as f64) as i64)
    };
    detail.completed_turn_count = detail.tasks.len() as i64;
    Ok(())
}
pub fn normalize_tiers(events: impl IntoIterator<Item = TierEvent>) -> Vec<TierEvent> {
    let mut seen = HashSet::new();
    let mut rows: Vec<_> = events
        .into_iter()
        .filter_map(|mut event| {
            event.service_tier = event.service_tier.trim().to_lowercase();
            if event.timestamp.is_empty()
                || event.service_tier.is_empty()
                || !seen.insert((event.timestamp.clone(), event.service_tier.clone()))
            {
                None
            } else {
                Some(event)
            }
        })
        .collect();
    rows.sort_by_key(|e| parsed(&e.timestamp));
    rows
}
pub fn apply_runtime_tiers(file: &mut ParsedFile, events: Vec<TierEvent>) {
    if events.is_empty() {
        return;
    }
    let combined = normalize_tiers(
        events
            .into_iter()
            .chain(std::mem::take(&mut file.state.service_tier_events)),
    );
    let mut index = 0;
    let mut active = "";
    for row in &mut file.detail.timeline {
        let moment = row.timestamp.as_deref().and_then(timestamp::parse);
        while index < combined.len() {
            let event = &combined[index];
            let at = timestamp::parse(&event.timestamp);
            if moment.is_none() || at.is_none() || at > moment {
                break;
            }
            active = &event.service_tier;
            index += 1;
        }
        if !active.is_empty() {
            row.service_tier = active.into();
        }
    }
    if let Some(last) = file.detail.timeline.last() {
        file.detail.service_tier = last.service_tier.clone();
    }
    file.detail.service_tiers.clear();
    for event in &file.detail.timeline {
        unique(&mut file.detail.service_tiers, &event.service_tier);
    }
    unique(&mut file.detail.service_tiers, &file.detail.service_tier);
    file.state.service_tier_events = combined;
}

pub fn dependency_components(files: &[&ParsedFile]) -> Vec<Vec<usize>> {
    let mut leaders: Vec<_> = (0..files.len()).collect();
    let mut nodes = HashMap::new();
    fn root(leaders: &mut [usize], mut i: usize) -> usize {
        while leaders[i] != i {
            leaders[i] = leaders[leaders[i]];
            i = leaders[i];
        }
        i
    }
    for (i, file) in files.iter().enumerate() {
        if let Some(previous) = nodes.insert(identity(file), i) {
            let r = root(&mut leaders, previous);
            leaders[i] = r;
        }
    }
    for (i, file) in files.iter().enumerate() {
        if let Some(parent) =
            nodes.get(&(file.detail.environment_id.clone(), parent_id(file).into()))
        {
            let p = root(&mut leaders, *parent);
            let r = root(&mut leaders, i);
            leaders[r] = p;
        }
    }
    let mut groups = Vec::<Vec<usize>>::new();
    let mut positions = HashMap::new();
    for i in 0..files.len() {
        let key = root(&mut leaders, i);
        let index = *positions.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[index].push(i);
    }
    groups
}

fn moment(file: &ParsedFile) -> DateTime<Utc> {
    file.detail
        .timeline
        .iter()
        .filter_map(|e| e.timestamp.as_deref().and_then(timestamp::parse))
        .next()
        .or_else(|| {
            [
                &file.detail.created_at,
                &file.detail.start_at,
                &file.detail.end_at,
            ]
            .iter()
            .find_map(|s| timestamp::parse(s))
        })
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}
fn merge_group(mut files: Vec<ParsedFile>) -> Result<ParsedFile, Error> {
    if files.len() == 1 {
        return Ok(files.remove(0));
    }
    files.sort_by(|a, b| {
        moment(a)
            .cmp(&moment(b))
            .then_with(|| a.detail.path.cmp(&b.detail.path))
    });
    let mut merged = files.remove(0);
    let latest = &files.last().unwrap().detail;
    merged.detail.path = latest.path.clone();
    merged.detail.cwd = latest.cwd.clone();
    merged.detail.project = latest.project.clone();
    merged.detail.project_root = latest.project_root.clone();
    merged.detail.workspace_root = latest.workspace_root.clone();
    merged.detail.project_branch = latest.project_branch.clone();
    merged.detail.is_git_worktree = latest.is_git_worktree;
    merged.detail.model = latest.model.clone();
    merged.detail.service_tier = latest.service_tier.clone();
    merged.detail.effort = latest.effort.clone();
    merged.detail.originator = latest.originator.clone();
    merged.detail.cli_version = latest.cli_version.clone();
    merged.detail.model_context_window = latest.model_context_window;
    merged.detail.latest_rate_limits = latest.latest_rate_limits.clone();
    merged.detail.latest_plan_type = latest.latest_plan_type.clone();
    merged.detail.latest_rate_limit_reached_type = latest.latest_rate_limit_reached_type.clone();
    merged.detail.last_agent_preview = latest.last_agent_preview.clone();
    merged.state.raw_end_at = files.last().unwrap().state.raw_end_at.clone();
    let any_active =
        merged.detail.source == "active" || files.iter().any(|f| f.detail.source == "active");
    merged.detail.source = if any_active {
        "active".into()
    } else {
        latest.source.clone()
    };
    let mut timeline = std::mem::take(&mut merged.detail.timeline);
    let mut tasks = std::mem::take(&mut merged.detail.tasks);
    for file in files {
        let d = file.detail;
        for (dst, value) in [
            (&mut merged.detail.file_size, d.file_size),
            (&mut merged.detail.line_count, d.line_count),
            (&mut merged.detail.parse_errors, d.parse_errors),
            (
                &mut merged.detail.fast_skipped_line_count,
                d.fast_skipped_line_count,
            ),
            (&mut merged.detail.fast_skipped_bytes, d.fast_skipped_bytes),
            (&mut merged.detail.user_message_count, d.user_message_count),
            (
                &mut merged.detail.assistant_message_count,
                d.assistant_message_count,
            ),
        ] {
            *dst = dst.checked_add(value).ok_or(Error::CounterOverflow)?;
        }
        update_time(&mut merged.detail.created_at, &d.created_at, false);
        update_time(&mut merged.detail.start_at, &d.start_at, false);
        update_time(&mut merged.detail.end_at, &d.end_at, true);
        let mut updated = merged.detail.updated_at.take().unwrap_or_default();
        update_time(&mut updated, d.updated_at.as_deref().unwrap_or(""), true);
        merged.detail.updated_at = Some(updated);
        update_time(
            &mut merged.state.raw_created_at,
            &file.state.raw_created_at,
            false,
        );
        update_time(
            &mut merged.state.raw_start_at,
            &file.state.raw_start_at,
            false,
        );
        if merged.detail.first_user_prompt.is_empty() {
            merged.detail.first_user_prompt = d.first_user_prompt;
        }
        for model in d.models {
            unique(&mut merged.detail.models, &model);
        }
        for tier in d.service_tiers {
            unique(&mut merged.detail.service_tiers, &tier);
        }
        for (name, count) in d.tool_counts {
            let value = merged.detail.tool_counts.entry(name).or_default();
            *value = value.checked_add(count).ok_or(Error::CounterOverflow)?;
        }
        merged.state.turn_ids.extend(file.state.turn_ids);
        merged
            .state
            .service_tier_events
            .extend(file.state.service_tier_events);
        timeline.extend(d.timeline);
        tasks.extend(d.tasks);
    }
    let mut seen = HashSet::new();
    timeline.retain(|e| {
        seen.insert((
            e.timestamp.clone(),
            e.model.clone(),
            e.service_tier.clone(),
            e.total_token_usage.values(),
            e.last_token_usage.unwrap_or_default().values(),
        ))
    });
    timeline.sort_by_key(|e| parsed(e.timestamp.as_deref().unwrap_or("")));
    let mut seen_tasks = HashSet::new();
    tasks.retain(|t| {
        seen_tasks.insert((
            t.timestamp.clone(),
            t.turn_id.clone(),
            t.duration_ms,
            t.time_to_first_token_ms,
        ))
    });
    tasks.sort_by_key(|t| parsed(t.timestamp.as_deref().unwrap_or("")));
    merged.detail.timeline = timeline;
    merged.detail.tasks = tasks;
    merged.state.service_tier_events = normalize_tiers(merged.state.service_tier_events);
    let d = &mut merged.detail;
    for event in &d.timeline {
        unique(&mut d.models, &event.model);
        unique(&mut d.service_tiers, &event.service_tier);
    }
    unique(&mut d.models, &d.model);
    unique(&mut d.service_tiers, &d.service_tier);
    d.token_event_count = d.timeline.len() as i64;
    d.turn_count = if !merged.state.turn_ids.is_empty() {
        merged.state.turn_ids.len() as i64
    } else if !d.tasks.is_empty() {
        d.tasks.len() as i64
    } else {
        d.token_event_count
    };
    if let Some(last) = d.timeline.last() {
        d.total_token_usage = last.total_token_usage;
        d.last_token_usage = last.last_token_usage.unwrap_or_default();
        d.branch_total_token_usage = d.total_token_usage;
        if !last.model.is_empty() {
            d.model = last.model.clone();
        }
        if !last.service_tier.is_empty() {
            d.service_tier = last.service_tier.clone();
        }
        d.model_context_window = last.model_context_window;
        d.latest_rate_limits = last.rate_limits.clone();
    }
    task_totals(d)?;
    d.cached_input_percent = cache_percent(d.total_token_usage);
    Ok(merged)
}
fn merge(files: Vec<ParsedFile>) -> Result<Vec<ParsedFile>, Error> {
    let mut positions = HashMap::new();
    let mut groups: Vec<Vec<ParsedFile>> = Vec::new();
    for file in files {
        let key = identity(&file);
        let index = *positions.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[index].push(file);
    }
    groups.into_iter().map(merge_group).collect()
}
fn reprice(
    detail: &mut SessionDetail,
    pricing: &PriceSchedule,
    already_normalized: bool,
) -> Result<(), Error> {
    let events = if already_normalized
        && detail.timeline.iter().all(|e| {
            e.last_token_usage
                .unwrap_or_default()
                .values()
                .iter()
                .all(|v| *v >= 0)
        }) {
        detail
            .timeline
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.timestamp
                    .as_deref()
                    .and_then(timestamp::parse)
                    .map(|timestamp| NormalizedEvent {
                        source: e,
                        timestamp,
                        cumulative: e.total_token_usage,
                        delta: e.last_token_usage.unwrap_or_default(),
                        source_order: i,
                    })
            })
            .collect()
    } else {
        usage::normalize(&detail.timeline)?
    };
    detail.estimated_speed = crate::speed::SpeedTotals::from_events(&events)?;
    detail.pricing = pricing.price_events(
        &events,
        &detail.model,
        detail.total_token_usage,
        detail
            .timeline
            .last()
            .and_then(|event| event.timestamp.as_deref())
            .or(Some(&detail.end_at)),
        &detail.service_tier,
    )?;
    Ok(())
}
fn normalize_main(file: &mut ParsedFile, pricing: &PriceSchedule) -> Result<(), Error> {
    let d = &mut file.detail;
    if d.identity.is_subagent || !d.identity.forked_from_id.is_empty() {
        return Ok(());
    }
    if d.timeline.is_empty() {
        return reprice(d, pricing, false);
    }
    let deltas: Vec<_> = usage::normalize(&d.timeline)?
        .iter()
        .map(|e| (e.source_order, e.delta))
        .collect();
    let mut source: Vec<Option<RawEvent>> = std::mem::take(&mut d.timeline)
        .into_iter()
        .map(Some)
        .collect();
    let mut total = Usage::default();
    let mut last = Usage::default();
    for (index, delta) in deltas {
        total = total.checked_add(delta)?;
        last = delta;
        let mut row = source[index].take().unwrap();
        row.total_token_usage = total;
        row.last_token_usage = Some(delta);
        d.timeline.push(row);
    }
    d.total_token_usage = total;
    d.last_token_usage = last;
    d.branch_total_token_usage = total;
    d.token_event_count = d.timeline.len() as i64;
    if let Some(last) = d.timeline.last() {
        if !last.model.is_empty() {
            d.model = last.model.clone();
        }
        if !last.service_tier.is_empty() {
            d.service_tier = last.service_tier.clone();
        }
    }
    for row in &d.timeline {
        unique(&mut d.models, &row.model);
        unique(&mut d.service_tiers, &row.service_tier);
    }
    unique(&mut d.models, &d.model);
    unique(&mut d.service_tiers, &d.service_tier);
    d.cached_input_percent = cache_percent(total);
    reprice(d, pricing, true)
}
fn same_usage(a: &RawEvent, b: &RawEvent) -> bool {
    a.total_token_usage == b.total_token_usage
        && a.last_token_usage.unwrap_or_default() == b.last_token_usage.unwrap_or_default()
}
/// KMP prefix match preserves legacy longest-prefix semantics in linear time.
fn inherited_prefix(child: &[RawEvent], parent: &[&RawEvent]) -> usize {
    if child.is_empty() {
        return 0;
    }
    let mut failure = vec![0; child.len()];
    let mut matched = 0;
    for i in 1..child.len() {
        while matched > 0 && !same_usage(&child[i], &child[matched]) {
            matched = failure[matched - 1];
        }
        if same_usage(&child[i], &child[matched]) {
            matched += 1;
        }
        failure[i] = matched;
    }
    matched = 0;
    let mut best = 0;
    for row in parent {
        while matched > 0 && !same_usage(row, &child[matched]) {
            matched = failure[matched - 1];
        }
        if same_usage(row, &child[matched]) {
            matched += 1;
        }
        best = best.max(matched);
        if best == child.len() {
            return best;
        }
    }
    best
}
fn tier_at(
    files: &[ParsedFile],
    by_id: &HashMap<Identity, usize>,
    mut index: usize,
    mut time: Option<DateTime<Utc>>,
) -> String {
    let mut visited = HashSet::new();
    while visited.insert(index) {
        let file = &files[index];
        let Some(target) = time else {
            return String::new();
        };
        let events = normalize_tiers(file.state.service_tier_events.clone().into_iter().chain(
            file.detail.timeline.iter().filter_map(|e| {
                e.timestamp.as_ref().map(|t| TierEvent {
                    timestamp: t.clone(),
                    service_tier: e.service_tier.clone(),
                })
            }),
        ));
        let mut tier = String::new();
        for event in events {
            if let Some(at) = timestamp::parse(&event.timestamp) {
                if at > target {
                    break;
                }
                tier = event.service_tier;
            }
        }
        if !tier.is_empty() {
            return tier;
        }
        let Some(parent) = by_id.get(&(file.detail.environment_id.clone(), parent_id(file).into()))
        else {
            return String::new();
        };
        time = timestamp::parse(&file.detail.created_at).or(time);
        index = *parent;
    }
    String::new()
}
struct Inheritance {
    prefix: usize,
    baseline: Usage,
    resolved: bool,
    tier: String,
}
fn inheritance(
    files: &[ParsedFile],
    by_id: &HashMap<Identity, usize>,
    index: usize,
) -> Result<Inheritance, Error> {
    let file = &files[index];
    let child = &file.detail.timeline;
    let parent = by_id
        .get(&(file.detail.environment_id.clone(), parent_id(file).into()))
        .copied();
    let fork = timestamp::parse(&file.detail.created_at);
    let tier = parent
        .map(|p| tier_at(files, by_id, p, fork))
        .unwrap_or_default();
    let rows: Vec<_> = parent
        .map(|p| {
            files[p]
                .detail
                .timeline
                .iter()
                .filter(|e| {
                    fork.is_none()
                        || e.timestamp
                            .as_deref()
                            .and_then(timestamp::parse)
                            .is_some_and(|t| Some(t) <= fork)
                })
                .collect()
        })
        .unwrap_or_default();
    let prefix = inherited_prefix(child, &rows);
    let before = rows.last().map(|e| e.total_token_usage).unwrap_or_default();
    let mut baseline = if prefix > 0 {
        child[prefix - 1].total_token_usage
    } else {
        Usage::default()
    };
    let resolved = if prefix > 0 || file.detail.identity.forked_from_id.is_empty() {
        true
    } else if parent.is_some() {
        if child.is_empty()
            || !before.has_tokens()
            || child[0].total_token_usage == child[0].last_token_usage.unwrap_or_default()
        {
            true
        } else if child[0].total_token_usage.delta(before)?
            == child[0].last_token_usage.unwrap_or_default()
        {
            baseline = before;
            true
        } else {
            false
        }
    } else {
        child.is_empty()
    };
    Ok(Inheritance {
        prefix,
        baseline,
        resolved,
        tier,
    })
}
fn normalize_child(
    file: &mut ParsedFile,
    spec: Inheritance,
    pricing: &PriceSchedule,
) -> Result<(), Error> {
    let d = &mut file.detail;
    d.fork_usage_resolved = spec.resolved;
    if !spec.resolved {
        d.estimated_speed = None;
        d.timeline.clear();
        d.total_token_usage = Usage::default();
        d.last_token_usage = Usage::default();
        d.pricing = Pricing::default();
        d.cached_input_percent = None;
        d.token_event_count = 0;
        return Ok(());
    }
    let cutoff = spec.prefix.checked_sub(1).and_then(|i| {
        d.timeline[i]
            .timestamp
            .as_deref()
            .and_then(timestamp::parse)
    });
    let mut previous = spec.baseline;
    let mut total = Usage::default();
    let mut last = Usage::default();
    d.timeline.drain(..spec.prefix);
    for row in &mut d.timeline {
        let current = row.total_token_usage;
        last = current.delta(previous)?;
        previous = current;
        total = total.checked_add(last)?;
        row.total_token_usage = total;
        row.last_token_usage = Some(last);
        if row.service_tier.is_empty() {
            row.service_tier = spec.tier.clone();
        }
    }
    if d.service_tier.is_empty() {
        d.service_tier = spec.tier;
    }
    d.total_token_usage = total;
    d.last_token_usage = last;
    d.inherited_token_usage = spec.baseline;
    d.inherited_token_event_count = spec.prefix as i64;
    if let Some(cutoff) = cutoff {
        d.tasks.retain(|t| {
            t.timestamp
                .as_deref()
                .and_then(timestamp::parse)
                .is_some_and(|t| t > cutoff)
        });
    }
    task_totals(d)?;
    d.cached_input_percent = cache_percent(total);
    d.token_event_count = d.timeline.len() as i64;
    d.turn_count = if d.tasks.is_empty() {
        d.token_event_count
    } else {
        d.tasks.len() as i64
    };
    if !d.timeline.is_empty() {
        d.models.clear();
    }
    for row in &d.timeline {
        unique(&mut d.models, &row.model);
    }
    if let Some(last) = d.timeline.last() {
        if !last.model.is_empty() {
            d.model = last.model.clone();
        }
        if !last.service_tier.is_empty() {
            d.service_tier = last.service_tier.clone();
        }
    }
    if d.models.is_empty() {
        unique(&mut d.models, &d.model);
    }
    d.service_tiers.clear();
    for row in &d.timeline {
        unique(&mut d.service_tiers, &row.service_tier);
    }
    unique(&mut d.service_tiers, &d.service_tier);
    if let Some(first) = d.timeline.first() {
        d.start_at = first
            .timestamp
            .clone()
            .unwrap_or_else(|| d.created_at.clone());
    }
    if let Some(last) = d.timeline.last() {
        if let Some(time) = &last.timestamp {
            d.end_at = time.clone();
        }
        d.model_context_window = last.model_context_window;
        d.latest_rate_limits = last.rate_limits.clone();
    }
    reprice(d, pricing, true)
}

pub fn normalize_sessions(
    files: Vec<ParsedFile>,
    pricing: &PriceSchedule,
) -> Result<Vec<ParsedFile>, Error> {
    let mut files = merge(files)?;
    let by_id: HashMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(i, file)| (identity(file), i))
        .collect();
    let specs: Vec<_> = (0..files.len())
        .map(|i| {
            if files[i].detail.identity.is_subagent
                || !files[i].detail.identity.forked_from_id.is_empty()
            {
                inheritance(&files, &by_id, i).map(Some)
            } else {
                Ok(None)
            }
        })
        .collect::<Result<_, Error>>()?;
    // Match inherited history against raw parent counters. Normalizing a
    // parent's resets or request deltas first breaks equality with the copied
    // history in its children, including histories rewritten by Codex migration.
    for file in &mut files {
        normalize_main(file, pricing)?;
    }
    for (file, spec) in files.iter_mut().zip(specs) {
        if let Some(spec) = spec {
            normalize_child(file, spec, pricing)?;
        }
    }
    Ok(files)
}
