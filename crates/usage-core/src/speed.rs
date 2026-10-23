//! Conservative request timing reconstructed from rollout events, not turn duration.
use crate::{Error, NormalizedEvent, Usage, timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeedTotals {
    pub output_tokens: i64,
    pub duration_ms: i64,
    pub request_count: i64,
    #[serde(default)]
    pub first_token_request_count: i64,
}

impl SpeedTotals {
    pub fn checked_add(&mut self, other: &Self) -> Result<(), Error> {
        self.output_tokens = self
            .output_tokens
            .checked_add(other.output_tokens)
            .ok_or(Error::CounterOverflow)?;
        self.duration_ms = self
            .duration_ms
            .checked_add(other.duration_ms)
            .ok_or(Error::CounterOverflow)?;
        self.request_count = self
            .request_count
            .checked_add(other.request_count)
            .ok_or(Error::CounterOverflow)?;
        self.first_token_request_count = self
            .first_token_request_count
            .checked_add(other.first_token_request_count)
            .ok_or(Error::CounterOverflow)?;
        Ok(())
    }

    pub fn from_events(events: &[NormalizedEvent<'_>]) -> Result<Option<Self>, Error> {
        let mut total = Self::default();
        for event in events {
            if event.delta.output_tokens <= 0 {
                continue;
            }
            let (duration_ms, first_token) = match event.source.generation_duration_ms {
                Some(ms) => (Some(ms).filter(|ms| (100..=3_600_000).contains(ms)), true),
                None => (
                    event
                        .source
                        .request_duration_ms
                        .filter(|ms| (1_000..=3_600_000).contains(ms)),
                    false,
                ),
            };
            let Some(duration_ms) = duration_ms else {
                continue;
            };
            total.checked_add(&Self {
                output_tokens: event.delta.output_tokens,
                duration_ms,
                request_count: 1,
                first_token_request_count: i64::from(first_token),
            })?;
        }
        Ok((total.request_count > 0).then_some(total))
    }
}

/// Serialized with the parser cursor so an append can finish an in-flight request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RequestTimer {
    boundary: Option<i64>,
    previous_count: Option<i64>,
    start: Option<i64>,
    output_end: Option<i64>,
    tool_end: Option<i64>,
    usage_end: Option<(i64, Usage)>,
    first_output_start: Option<i64>,
    first_output_end: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestTiming {
    pub duration_ms: Option<i64>,
    pub generation_ms: Option<i64>,
}

impl RequestTimer {
    pub fn observe(&mut self, item: &Value) {
        let Some(at) = item["timestamp"]
            .as_str()
            .and_then(timestamp::parse)
            .map(|t| t.timestamp_millis())
        else {
            return;
        };
        let p = &item["payload"];
        let kind = p["type"].as_str().unwrap_or_default();
        match item["type"].as_str().unwrap_or_default() {
            "turn_context" => self.boundary = self.boundary.max(Some(at)),
            "token_usage_record" => {
                if let Some(usage) = p
                    .get("usage")
                    .filter(|u| u.is_object())
                    .and_then(|u| serde_json::from_value(u.clone()).ok())
                {
                    self.usage_end = Some((at, usage));
                }
            }
            "event_msg" if matches!(kind, "task_started" | "turn_aborted" | "task_complete") => {
                *self = Self {
                    boundary: self.boundary.max(Some(at)),
                    previous_count: self.previous_count,
                    ..Self::default()
                };
            }
            "event_msg" if matches!(kind, "user_message" | "user_input" | "human_message") => {
                self.boundary = self.boundary.max(Some(at));
            }
            "event_msg"
                if kind == "item_completed"
                    && matches!(
                        p["item"]["type"].as_str(),
                        Some("Reasoning" | "AgentMessage")
                    ) =>
            {
                if self.note_output(at)
                    && let Some(start) = p["started_at_ms"].as_i64().filter(|start| {
                        *start <= at && at.checked_sub(*start).is_some_and(|ms| ms <= 3_600_000)
                    })
                {
                    self.first_output_start =
                        Some(self.first_output_start.map_or(start, |old| old.min(start)));
                }
            }
            "response_item" => {
                if kind.ends_with("_output") {
                    self.boundary = self.boundary.max(Some(at));
                    if self.output_end.is_some() {
                        self.tool_end = Some(at);
                    }
                } else if kind == "message" && p["role"] != "assistant" {
                    self.boundary = self.boundary.max(Some(at));
                } else if kind == "reasoning"
                    || kind.ends_with("_call")
                    || (kind == "message" && p["role"] == "assistant")
                {
                    self.note_output(at);
                }
            }
            _ => {}
        }
    }

    fn note_output(&mut self, at: i64) -> bool {
        // Older logs flush the preceding response's items just after token_count.
        if self.start.is_none()
            && self
                .previous_count
                .is_some_and(|t| (0..=100).contains(&(at - t)))
        {
            return false;
        }
        if self.output_end.is_none() {
            self.start = self.boundary.filter(|t| *t <= at);
            self.first_output_end = Some(at);
        }
        self.output_end = Some(at);
        self.tool_end = None;
        true
    }

    pub fn finish(&mut self, at: Option<&str>, usage: Usage) -> RequestTiming {
        let end = at.and_then(timestamp::parse).map(|t| t.timestamp_millis());
        let start = self.start;
        let output_end = self.output_end;
        // A later text item cannot provide the first token of an earlier tool-only output.
        let first = self
            .first_output_start
            .filter(|first| self.first_output_end.is_some_and(|end| *first <= end));
        let recorded_end = self
            .usage_end
            .filter(|(_, u)| {
                u.input_tokens == usage.input_tokens
                    && u.cached_input_tokens == usage.cached_input_tokens
                    && u.output_tokens == usage.output_tokens
            })
            .map(|(t, _)| t);
        let waited_for_tool = self
            .tool_end
            .zip(end)
            .is_some_and(|(tool, end)| (0..=100).contains(&(end - tool)));
        let chosen_end = recorded_end.or(if waited_for_tool { output_end } else { end });
        *self = Self {
            boundary: self.boundary.max(end),
            previous_count: end,
            ..Self::default()
        };
        let duration_ms = start
            .zip(chosen_end)
            .and_then(|(start, end)| end.checked_sub(start))
            .filter(|ms| output_end.is_some() && (100..=3_600_000).contains(ms));
        let generation_ms = duration_ms.and_then(|_| {
            first
                .filter(|first| start.is_some_and(|start| *first >= start))
                .zip(chosen_end)
                .and_then(|(first, end)| end.checked_sub(first))
        });
        RequestTiming {
            duration_ms,
            generation_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RawEvent,
        parser::{ParsedFile, ParserState, SessionDetail},
        query, usage,
    };
    use serde_json::json;
    use std::io::Write;

    fn at(ms: i64) -> String {
        timestamp::format(
            timestamp::parse("2026-10-23T00:00:00Z").unwrap() + chrono::Duration::milliseconds(ms),
        )
    }
    fn line(ms: i64, kind: &str, payload: Value) -> Value {
        json!({"timestamp":at(ms), "type":kind, "payload":payload})
    }
    fn output(ms: i64) -> Value {
        line(
            ms,
            "response_item",
            json!({"type":"message","role":"assistant"}),
        )
    }
    fn boundary(ms: i64) -> Value {
        line(ms, "turn_context", json!({"model":"gpt-6.1-sol"}))
    }
    fn tool(ms: i64) -> Value {
        line(ms, "response_item", json!({"type":"function_call_output"}))
    }
    fn tokens(output: i64) -> Usage {
        Usage {
            output_tokens: output,
            total_tokens: output,
            ..Usage::default()
        }
    }
    fn timed_output(start: i64, end: i64) -> Value {
        line(
            end,
            "event_msg",
            json!({"type":"item_completed", "item":{"type":"Reasoning"}, "started_at_ms": timestamp::parse(&at(start)).unwrap().timestamp_millis()}),
        )
    }

    #[test]
    fn first_token_window_includes_small_outputs_and_excludes_tool_and_first_token_wait() {
        let mut timer = RequestTimer::default();
        timer.observe(&boundary(0));
        timer.observe(&timed_output(2_000, 3_000));
        timer.observe(&output(3_000));
        timer.observe(&line(
            4_000,
            "token_usage_record",
            json!({"usage":tokens(20)}),
        ));
        timer.observe(&tool(64_000));
        let timing = timer.finish(Some(&at(64_002)), tokens(20));
        assert_eq!(timing.duration_ms, Some(4_000));
        assert_eq!(timing.generation_ms, Some(2_000));
        let events = vec![RawEvent {
            timestamp: Some(at(64_002)),
            total_token_usage: tokens(20),
            last_token_usage: Some(tokens(20)),
            request_duration_ms: timing.duration_ms,
            generation_duration_ms: timing.generation_ms,
            ..Default::default()
        }];
        assert_eq!(
            SpeedTotals::from_events(&usage::normalize(&events).unwrap()).unwrap(),
            Some(SpeedTotals {
                output_tokens: 20,
                duration_ms: 2_000,
                request_count: 1,
                first_token_request_count: 1
            })
        );
    }

    #[test]
    fn old_logs_cut_off_delayed_tool_time_and_start_next_request_after_tool() {
        let mut timer = RequestTimer::default();
        timer.observe(&boundary(0));
        timer.observe(&output(5_000));
        timer.observe(&tool(65_000));
        let first = timer.finish(Some(&at(65_001)), tokens(400));
        assert_eq!(first.duration_ms, Some(5_000));
        assert_eq!(first.generation_ms, None);
        timer.observe(&output(68_000));
        assert_eq!(
            timer.finish(Some(&at(70_001)), tokens(600)).duration_ms,
            Some(5_000)
        );
    }

    #[test]
    fn later_item_start_does_not_claim_to_be_first_token_of_earlier_tool_call() {
        let mut timer = RequestTimer::default();
        timer.observe(&boundary(0));
        timer.observe(&line(
            2_000,
            "response_item",
            json!({"type":"function_call"}),
        ));
        timer.observe(&timed_output(3_000, 4_000));
        let timing = timer.finish(Some(&at(5_000)), tokens(100));
        assert_eq!(timing.duration_ms, Some(5_000));
        assert_eq!(timing.generation_ms, None);
    }

    #[test]
    fn unknown_short_batched_and_long_windows_do_not_become_speed_samples() {
        let mut timer = RequestTimer::default();
        timer.observe(&output(5_000));
        assert_eq!(
            timer.finish(Some(&at(6_000)), tokens(100)).duration_ms,
            None
        );
        let events: Vec<_> = [
            (Some(500), None),
            (Some(5_000), Some(1)),
            (Some(3_600_001), None),
            (None, None),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (duration, generation))| RawEvent {
            timestamp: Some(at(i as i64 * 10_000)),
            total_token_usage: tokens((i as i64 + 1) * 100),
            request_duration_ms: duration,
            generation_duration_ms: generation,
            ..Default::default()
        })
        .collect();
        assert_eq!(
            SpeedTotals::from_events(&usage::normalize(&events).unwrap()).unwrap(),
            None
        );
    }

    #[test]
    fn copied_flush_and_aborted_request_do_not_pin_the_next_start_to_old_history() {
        let mut timer = RequestTimer::default();
        timer.observe(&boundary(0));
        timer.observe(&output(2_000));
        timer.finish(Some(&at(3_000)), tokens(50));
        timer.observe(&output(3_001));
        timer.observe(&boundary(300_000));
        timer.observe(&output(305_000));
        assert_eq!(
            timer.finish(Some(&at(306_000)), tokens(50)).duration_ms,
            Some(6_000)
        );
        timer.observe(&boundary(310_000));
        timer.observe(&output(311_000));
        timer.observe(&line(312_000, "event_msg", json!({"type":"turn_aborted"})));
        timer.observe(&line(400_000, "event_msg", json!({"type":"task_started"})));
        timer.observe(&output(402_000));
        assert_eq!(
            timer.finish(Some(&at(403_000)), tokens(50)).duration_ms,
            Some(3_000)
        );
    }

    #[test]
    fn parser_resume_keeps_timing_and_duplicate_usage_does_not_close_pending_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        let count = |ms, total, last| {
            line(
                ms,
                "event_msg",
                json!({"type":"token_count","info":{"total_token_usage":tokens(total),"last_token_usage":tokens(last)}}),
            )
        };
        let records = [
            boundary(0),
            timed_output(2_000, 3_000),
            output(3_000),
            count(4_000, 20, 20),
            boundary(10_000),
            timed_output(12_000, 13_000),
            output(13_000),
            count(13_500, 20, 20),
        ];
        let mut file = std::fs::File::create(&path).unwrap();
        for record in records {
            writeln!(file, "{record}").unwrap();
        }
        let source = crate::parser::LogSource {
            id: "local".into(),
            label: "Local".into(),
            home: dir.path().into(),
        };
        let prices = crate::PriceSchedule::bundled();
        let parsed =
            crate::parser::parse_file(&path, "active", &source, &prices, None, None).unwrap();
        let restored: ParsedFile =
            serde_json::from_value(serde_json::to_value(parsed).unwrap()).unwrap();
        let mut append = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(append, "{}", count(15_000, 50, 30)).unwrap();
        let resumed =
            crate::parser::parse_file(&path, "active", &source, &prices, Some(restored), None)
                .unwrap();
        let full =
            crate::parser::parse_file(&path, "active", &source, &prices, None, None).unwrap();
        assert_eq!(resumed.detail.estimated_speed, full.detail.estimated_speed);
        assert_eq!(
            resumed.detail.estimated_speed,
            Some(SpeedTotals {
                output_tokens: 50,
                duration_ms: 5_000,
                request_count: 2,
                first_token_request_count: 2
            })
        );
        assert_eq!(resumed.detail.timeline[1].generation_duration_ms, None);
    }

    #[test]
    fn weighted_totals_follow_period_and_model_filters_and_ignore_duplicate_snapshots() {
        let events: Vec<_> = [
            (1_000, "a", 100, Some(1_000)),
            (10_000, "b", 400, Some(6_000)),
            (11_000, "b", 400, None),
        ]
        .into_iter()
        .map(|(ms, model, total, duration)| RawEvent {
            timestamp: Some(at(ms)),
            model: model.into(),
            total_token_usage: tokens(total),
            request_duration_ms: duration,
            ..Default::default()
        })
        .collect();
        let speed = SpeedTotals::from_events(&usage::normalize(&events).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            speed,
            SpeedTotals {
                output_tokens: 400,
                duration_ms: 7_000,
                request_count: 2,
                first_token_request_count: 0
            }
        );
        let detail = SessionDetail {
            timeline: events,
            total_token_usage: tokens(400),
            estimated_speed: Some(speed.clone()),
            ..Default::default()
        };
        let prices = crate::PriceSchedule::bundled();
        let ranged = query::detail_for_period(
            &detail,
            timestamp::parse(&at(5_000)).unwrap(),
            timestamp::parse(&at(12_000)).unwrap(),
            &prices,
        )
        .unwrap();
        assert_eq!(ranged.estimated_speed.as_ref().unwrap().output_tokens, 300);
        assert_eq!(ranged.estimated_speed.as_ref().unwrap().duration_ms, 6_000);
        let indexed =
            crate::index::IndexedSession::from_detail(std::sync::Arc::new(detail)).unwrap();
        let selected = query::detail_for_models_indexed(&indexed, &["a".into()], &prices).unwrap();
        assert_eq!(
            selected.estimated_speed.as_ref().unwrap().output_tokens,
            100
        );
        let rows = [
            query::SessionSummary::from(&selected),
            query::SessionSummary::from(&ranged),
        ];
        assert_eq!(
            query::summarize(&rows).unwrap().estimated_speed,
            Some(speed)
        );
    }

    #[test]
    fn inherited_prefix_is_removed_from_speed_along_with_usage() {
        let row = RawEvent {
            timestamp: Some(at(1_000)),
            total_token_usage: tokens(100),
            last_token_usage: Some(tokens(100)),
            request_duration_ms: Some(1_000),
            ..Default::default()
        };
        let mut parent = SessionDetail {
            timeline: vec![row.clone()],
            total_token_usage: tokens(100),
            ..Default::default()
        };
        parent.identity.session_id = "parent".into();
        let mut child = SessionDetail {
            timeline: vec![
                row,
                RawEvent {
                    timestamp: Some(at(5_000)),
                    total_token_usage: tokens(300),
                    last_token_usage: Some(tokens(200)),
                    request_duration_ms: Some(4_000),
                    ..Default::default()
                },
            ],
            total_token_usage: tokens(300),
            ..Default::default()
        };
        child.identity.session_id = "child".into();
        child.identity.parent_thread_id = "parent".into();
        child.identity.is_subagent = true;
        let result = crate::normalize::normalize_sessions(
            vec![
                ParsedFile {
                    detail: parent,
                    state: ParserState::default(),
                },
                ParsedFile {
                    detail: child,
                    state: ParserState::default(),
                },
            ],
            &crate::PriceSchedule::bundled(),
        )
        .unwrap();
        let child = &result
            .iter()
            .find(|f| f.detail.identity.session_id == "child")
            .unwrap()
            .detail;
        assert_eq!(
            child.estimated_speed,
            Some(SpeedTotals {
                output_tokens: 200,
                duration_ms: 4_000,
                request_count: 1,
                first_token_request_count: 0
            })
        );
    }
}
