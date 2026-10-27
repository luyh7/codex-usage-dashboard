//! Versioned historical pricing. Runtime updates can replace this data without
//! reopening logs: normalized deltas retain all inputs needed for repricing.
use crate::{Error, NormalizedEvent, Usage, timestamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Rates = BTreeMap<String, Prices>;
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Prices {
    pub input: f64,
    pub cached_input: f64,
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_input: Option<f64>,
}
impl Prices {
    fn scaled(self, multiplier: f64) -> Self {
        Self {
            input: self.input * multiplier,
            cached_input: self.cached_input * multiplier,
            output: self.output * multiplier,
            cache_write_input: self.cache_write_input.map(|v| v * multiplier),
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceEntry {
    pub model: String,
    pub effective_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_until: Option<String>,
    pub context_tier: Option<String>,
    #[serde(default)]
    pub service_tier: String,
    #[serde(default = "standard_multiplier")]
    pub cost_multiplier: f64,
    pub prices: Prices,
}
fn standard_multiplier() -> f64 {
    1.0
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CostParts {
    pub input_tokens: f64,
    pub cached_input_tokens: f64,
    pub cache_write_tokens: f64,
    pub output_tokens: f64,
    pub reasoning_output_tokens: f64,
}
impl CostParts {
    pub fn for_usage(usage: Usage, price: Prices) -> Self {
        let input = usage.input_tokens.max(0);
        let cached = usage.cached_input_tokens.clamp(0, input);
        let output = usage.output_tokens.max(0);
        let reasoning = usage.reasoning_output_tokens.clamp(0, output);
        Self {
            input_tokens: (input - cached) as f64 * price.input / 1_000_000.,
            cached_input_tokens: cached as f64 * price.cached_input / 1_000_000.,
            cache_write_tokens: 0.,
            output_tokens: output as f64 * price.output / 1_000_000.,
            reasoning_output_tokens: reasoning as f64 * price.output / 1_000_000.,
        }
    }
    pub fn total(&self) -> f64 {
        // Reasoning is a subset of output; never bill it a second time.
        // Compensated sum matches the frozen Python 3.14 baseline. Plain
        // left-to-right addition can cross the sixth-decimal rounding boundary.
        let mut sum = 0.0_f64;
        let mut correction = 0.0_f64;
        for value in [
            self.input_tokens,
            self.cached_input_tokens,
            self.cache_write_tokens,
            self.output_tokens,
        ] {
            let next = sum + value;
            correction += if sum.abs() >= value.abs() {
                (sum - next) + value
            } else {
                (value - next) + sum
            };
            sum = next;
        }
        sum + correction
    }
    fn add(&mut self, rhs: &Self) {
        self.input_tokens += rhs.input_tokens;
        self.cached_input_tokens += rhs.cached_input_tokens;
        self.cache_write_tokens += rhs.cache_write_tokens;
        self.output_tokens += rhs.output_tokens;
        self.reasoning_output_tokens += rhs.reasoning_output_tokens;
    }
    fn rounded(mut self) -> Self {
        self.input_tokens = round_money(self.input_tokens);
        self.cached_input_tokens = round_money(self.cached_input_tokens);
        self.cache_write_tokens = round_money(self.cache_write_tokens);
        self.output_tokens = round_money(self.output_tokens);
        self.reasoning_output_tokens = round_money(self.reasoning_output_tokens);
        self
    }
}
/// Decimal formatting rounds the actual binary float, including ties-to-even,
/// instead of multiplying by 1e6 (which can introduce a second rounding).
pub fn round_money(value: f64) -> f64 {
    format!("{value:.6}").parse().expect("finite cost")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PriceSegment {
    #[serde(flatten)]
    pub entry: PriceEntry,
    pub usage: Usage,
    pub estimated_cost_usd: f64,
    pub estimated_cost_breakdown_usd: CostParts,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pricing {
    pub estimated_cost_usd: Option<f64>,
    pub estimated_cost_breakdown_usd: Option<CostParts>,
    pub price_model_known: bool,
    pub applied_price_segments: Vec<PriceSegment>,
}
#[derive(Deserialize)]
struct Asset {
    version: u32,
    schedules: Vec<(Option<String>, Rates)>,
    fast: BTreeMap<String, f64>,
    ultrafast: BTreeMap<String, f64>,
    aliases: BTreeMap<String, String>,
    families: Vec<FamilyAsset>,
}
#[derive(Deserialize)]
struct FamilyAsset {
    short: Rates,
    threshold: i64,
    #[serde(default)]
    schedules: Vec<(Option<String>, Rates)>,
    #[serde(default)]
    long: Rates,
}
struct Schedule {
    effective_at: Option<String>,
    moment: Option<DateTime<Utc>>,
    rates: Rates,
}
struct Family {
    models: Rates,
    threshold: i64,
    schedules: Vec<Schedule>,
    long: Rates,
}

pub struct PriceSchedule {
    pub version: u32,
    schedules: Vec<Schedule>,
    families: Vec<Family>,
    fast: BTreeMap<String, f64>,
    ultrafast: BTreeMap<String, f64>,
    aliases: BTreeMap<String, String>,
}
fn schedules(rows: Vec<(Option<String>, Rates)>) -> Result<Vec<Schedule>, Error> {
    rows.into_iter()
        .map(|(effective_at, rates)| {
            let moment = match &effective_at {
                Some(s) => Some(
                    timestamp::parse(s)
                        .ok_or_else(|| Error::InvalidData("invalid price effective time".into()))?,
                ),
                None => None,
            };
            if rates.values().any(|v| {
                [
                    v.input,
                    v.cached_input,
                    v.output,
                    v.cache_write_input.unwrap_or(0.),
                ]
                .iter()
                .any(|r| !r.is_finite() || *r < 0.)
            }) {
                return Err(Error::InvalidData("invalid price".into()));
            }
            Ok(Schedule {
                effective_at,
                moment,
                rates,
            })
        })
        .collect()
}
fn lookup<'a>(model: &str, rates: &'a Rates) -> Option<(&'a str, Prices)> {
    if let Some((key, value)) = rates.get_key_value(model) {
        return Some((key, *value));
    }
    rates
        .iter()
        .filter(|(key, _)| {
            model
                .strip_prefix(key.as_str())
                .is_some_and(|s| s.starts_with('-'))
        })
        .max_by_key(|(key, _)| key.len())
        .map(|(key, value)| (key.as_str(), *value))
}
fn effective(
    model: &str,
    selected: Option<&str>,
    prices: Prices,
    schedules: &[Schedule],
) -> Option<String> {
    let index = schedules
        .iter()
        .position(|s| s.effective_at.as_deref() == selected)
        .unwrap_or(schedules.len().saturating_sub(1));
    let mut result = selected.map(str::to_owned);
    for earlier in schedules[..index].iter().rev() {
        if lookup(model, &earlier.rates).map(|(_, p)| p) != Some(prices) {
            break;
        }
        result = earlier.effective_at.clone();
    }
    result
}
fn expires(
    model: &str,
    selected: Option<DateTime<Utc>>,
    prices: Prices,
    schedules: &[Schedule],
) -> Option<String> {
    schedules
        .iter()
        .find(|s| s.moment > selected && lookup(model, &s.rates).map(|(_, p)| p) != Some(prices))
        .and_then(|s| s.effective_at.clone())
}
impl PriceSchedule {
    pub fn bundled() -> Self {
        Self::from_json(include_str!("../data/prices-v1.json")).expect("validated bundled pricing")
    }
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let asset: Asset = serde_json::from_str(json)?;
        let history = schedules(asset.schedules)?;
        if history.is_empty() || history.windows(2).any(|v| v[0].moment >= v[1].moment) {
            return Err(Error::InvalidData(
                "pricing schedules must be ordered".into(),
            ));
        }
        let mut families = Vec::new();
        for f in asset.families {
            families.push(Family {
                models: f.short,
                threshold: f.threshold,
                schedules: schedules(f.schedules)?,
                long: f.long,
            });
        }
        Ok(Self {
            version: asset.version,
            schedules: history,
            families,
            fast: asset.fast,
            ultrafast: asset.ultrafast,
            aliases: asset.aliases,
        })
    }
    pub fn quote(
        &self,
        model: &str,
        time: Option<&str>,
        input: Option<i64>,
        tier: &str,
    ) -> Option<PriceEntry> {
        let model = model.trim().to_lowercase();
        let model = model.rsplit('/').next()?.trim();
        let model = self.aliases.get(model).map(String::as_str).unwrap_or(model);
        let moment = time.and_then(timestamp::parse);
        let schedule = moment
            .and_then(|time| {
                self.schedules
                    .iter()
                    .rev()
                    .find(|s| s.moment.is_none_or(|t| time >= t))
            })
            .unwrap_or_else(|| self.schedules.last().unwrap());
        let (mut canonical, mut prices) = lookup(model, &schedule.rates)?;
        let mut effective_at = effective(
            model,
            schedule.effective_at.as_deref(),
            prices,
            &self.schedules,
        );
        let mut context_tier = None;
        let mut effective_until = expires(model, schedule.moment, prices, &self.schedules);
        if let Some(family) = self
            .families
            .iter()
            .find(|f| f.models.contains_key(canonical))
        {
            context_tier = Some("short".to_owned());
            if input.is_some_and(|n| n > family.threshold) {
                if family.schedules.is_empty() {
                    if let Some((key, price)) = lookup(model, &family.long) {
                        canonical = key;
                        prices = price;
                        context_tier = Some("long".into());
                        effective_until = None;
                    }
                } else {
                    let long = family
                        .schedules
                        .iter()
                        .rev()
                        .find(|s| schedule.moment.is_none() || schedule.moment >= s.moment)
                        .unwrap_or(&family.schedules[0]);
                    if let Some((key, price)) = lookup(model, &long.rates) {
                        canonical = key;
                        prices = price;
                        context_tier = Some("long".into());
                        effective_at = effective(
                            model,
                            long.effective_at.as_deref(),
                            prices,
                            &family.schedules,
                        );
                        effective_until = expires(model, long.moment, prices, &family.schedules);
                    }
                }
            }
        }
        let service_tier = tier.trim().to_lowercase();
        let cost_multiplier = match service_tier.as_str() {
            "priority" | "fast" => *self.fast.get(canonical)?,
            "ultrafast" => *self.ultrafast.get(canonical)?,
            _ => 1., // Freeze legacy non-fast-tier behavior before any policy change.
        };
        Some(PriceEntry {
            model: canonical.into(),
            effective_at,
            effective_until,
            context_tier,
            service_tier,
            cost_multiplier,
            prices: prices.scaled(cost_multiplier),
        })
    }
    pub fn estimate(
        &self,
        usage: Usage,
        model: &str,
        time: Option<&str>,
        tier: &str,
    ) -> Option<f64> {
        self.quote(model, time, Some(usage.input_tokens.max(0)), tier)
            .map(|entry| round_money(CostParts::for_usage(usage, entry.prices).total()))
    }
    /// Events are normalized once by the caller and shared with daily/period
    /// queries. Unknown prices propagate null while known segments are retained.
    pub fn price_events(
        &self,
        events: &[NormalizedEvent<'_>],
        fallback_model: &str,
        fallback_usage: Usage,
        fallback_timestamp: Option<&str>,
        fallback_tier: &str,
    ) -> Result<Pricing, Error> {
        let mut total = CostParts::default();
        let mut known = true;
        let mut segments: Vec<PriceSegment> = Vec::new();
        let mut add = |usage: Usage,
                       model: &str,
                       time: Option<&str>,
                       tier: &str,
                       is_request: bool|
         -> Result<(), Error> {
            let Some(entry) = self.quote(
                model,
                time,
                if is_request {
                    Some(usage.input_tokens.max(0))
                } else {
                    None
                },
                tier,
            ) else {
                known = false;
                return Ok(());
            };
            let parts = CostParts::for_usage(usage, entry.prices);
            total.add(&parts);
            if !usage.has_tokens() {
                return Ok(());
            }
            let index = segments
                .iter()
                .position(|s| s.entry == entry)
                .unwrap_or_else(|| {
                    segments.push(PriceSegment {
                        entry,
                        usage: Usage::default(),
                        estimated_cost_usd: 0.,
                        estimated_cost_breakdown_usd: CostParts::default(),
                    });
                    segments.len() - 1
                });
            let segment = &mut segments[index];
            segment.usage = segment.usage.checked_add(usage)?;
            segment.estimated_cost_usd += parts.total();
            segment.estimated_cost_breakdown_usd.add(&parts);
            Ok(())
        };
        let mut requests = 0;
        for event in events.iter().filter(|event| event.delta.has_tokens()) {
            let model = if event.source.model.is_empty() {
                fallback_model
            } else {
                &event.source.model
            };
            add(
                event.delta,
                model,
                event.source.timestamp.as_deref(),
                &event.source.service_tier,
                true,
            )?;
            requests += 1;
        }
        if requests == 0 {
            add(
                fallback_usage,
                fallback_model,
                fallback_timestamp,
                fallback_tier,
                false,
            )?;
        }
        for segment in &mut segments {
            segment.estimated_cost_usd = round_money(segment.estimated_cost_usd);
            segment.estimated_cost_breakdown_usd =
                std::mem::take(&mut segment.estimated_cost_breakdown_usd).rounded();
        }
        Ok(Pricing {
            estimated_cost_usd: known.then(|| round_money(total.total())),
            estimated_cost_breakdown_usd: known.then(|| total.rounded()),
            price_model_known: known,
            applied_price_segments: segments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn price_validity_tracks_changes_for_each_context_and_model() {
        let schedule = PriceSchedule::bundled();
        for input in [100_000, 300_000] {
            let before = schedule
                .quote("gpt-5.6-sol", Some("2026-08-20T23:59:59Z"), Some(input), "")
                .unwrap();
            assert_eq!(before.effective_at.as_deref(), Some("2026-07-09T17:00:00Z"));
            assert_eq!(
                before.effective_until.as_deref(),
                Some("2026-08-21T00:00:00Z")
            );
            let after = schedule
                .quote(
                    "gpt-5.6-sol",
                    Some("2026-08-21T00:00:00Z"),
                    Some(input),
                    "priority",
                )
                .unwrap();
            assert_eq!(after.effective_at.as_deref(), Some("2026-08-21T00:00:00Z"));
            assert_eq!(after.effective_until, None);
        }
        let terra = schedule
            .quote(
                "gpt-5.6-terra",
                Some("2026-07-29T12:00:00Z"),
                Some(100_000),
                "",
            )
            .unwrap();
        assert_eq!(
            terra.effective_until.as_deref(),
            Some("2026-07-30T00:00:00Z")
        );
        let unchanged = schedule
            .quote("gpt-5.4", Some("2026-07-01T00:00:00Z"), None, "")
            .unwrap();
        assert_eq!(unchanged.effective_at, None);
        assert_eq!(unchanged.effective_until, None);
    }
    #[test]
    fn gpt_6_1_terra_prices_follow_release_date_context_and_fast_mode() {
        let schedule = PriceSchedule::bundled();
        let model = "gpt-6.1-terra-2026-10-26";
        assert!(
            schedule
                .quote(
                    model,
                    Some("2026-10-25T23:59:59Z"),
                    Some(272_000),
                    "standard"
                )
                .is_none()
        );
        let quote =
            |input, tier| schedule.quote(model, Some("2026-10-26T00:00:00Z"), Some(input), tier);
        let short = quote(272_000, "standard").unwrap();
        assert_eq!(short.effective_at.as_deref(), Some("2026-10-26T00:00:00Z"));
        assert_eq!(short.context_tier.as_deref(), Some("short"));
        assert_eq!(
            short.prices,
            Prices {
                input: 1.0,
                cached_input: 0.05,
                output: 5.0,
                cache_write_input: None
            }
        );
        let long = quote(272_001, "standard").unwrap();
        assert_eq!(long.context_tier.as_deref(), Some("long"));
        assert_eq!(
            long.prices,
            Prices {
                input: 2.0,
                cached_input: 0.1,
                output: 7.5,
                cache_write_input: None
            }
        );
        let fast = quote(272_001, "fast").unwrap();
        assert_eq!(fast.cost_multiplier, 2.0);
        assert_eq!(
            fast.prices,
            Prices {
                input: 4.0,
                cached_input: 0.2,
                output: 15.0,
                cache_write_input: None
            }
        );
        assert_eq!(
            quote(272_000, "priority").unwrap().prices,
            short.prices.scaled(2.0)
        );
        assert!(quote(272_000, "ultrafast").is_none());
    }
    #[test]
    fn costs_unknown_and_reasoning_subset() {
        let schedule = PriceSchedule::bundled();
        let usage = Usage {
            input_tokens: 100_000,
            cached_input_tokens: 50_000,
            output_tokens: 25_000,
            reasoning_output_tokens: 25_000,
            ..Default::default()
        };
        assert_eq!(
            schedule.estimate(usage, "grok-4.5", Some("2026-07-24T00:00:00Z"), ""),
            Some(0.265)
        );
        assert_eq!(schedule.estimate(usage, "unknown", None, ""), None);
        assert_eq!(schedule.estimate(usage, "gpt-5", None, "ultrafast"), None);
    }
}
