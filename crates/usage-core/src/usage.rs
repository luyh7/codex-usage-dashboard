use crate::{Error, timestamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// Numeric compatibility at the ingestion seam: Python accepts bool and finite
/// float counters, truncating floats toward zero. Strings and null become zero.
fn counter<'de, D: Deserializer<'de>>(de: D) -> Result<i64, D::Error> {
    use serde::de::Error as _;
    let value = serde_json::Value::deserialize(de)?;
    match value {
        serde_json::Value::Bool(v) => Ok(i64::from(v)),
        serde_json::Value::Number(v) => {
            if let Some(n) = v.as_i64() {
                return Ok(n);
            }
            if v.is_u64() {
                return Err(D::Error::custom("counter overflow"));
            }
            let n = v
                .as_f64()
                .ok_or_else(|| D::Error::custom("invalid counter"))?;
            if !n.is_finite() || n < i64::MIN as f64 || n >= 9_223_372_036_854_775_808.0 {
                return Err(D::Error::custom("counter overflow"));
            }
            Ok(n as i64)
        }
        _ => Ok(0),
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "serde_json::Value")]
pub struct Usage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}
impl TryFrom<serde_json::Value> for Usage {
    type Error = serde_json::Error;
    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let serde_json::Value::Object(mut values) = value else {
            return Ok(Self::default());
        };
        let keys = [
            "input_tokens",
            "cached_input_tokens",
            "cache_write_tokens",
            "output_tokens",
            "reasoning_output_tokens",
            "total_tokens",
        ];
        let mut result = [0; 6];
        for (i, key) in keys.into_iter().enumerate() {
            let value = values
                .remove(key)
                .or_else(|| {
                    if key == "cache_write_tokens" {
                        values.remove("cache_write_input_tokens")
                    } else {
                        None
                    }
                })
                .unwrap_or(serde_json::Value::Null);
            result[i] = counter(value)?;
        }
        Ok(Self::from_values(result))
    }
}
impl Usage {
    pub fn values(self) -> [i64; 6] {
        [
            self.input_tokens,
            self.cached_input_tokens,
            self.cache_write_tokens,
            self.output_tokens,
            self.reasoning_output_tokens,
            self.total_tokens,
        ]
    }
    pub fn from_values(v: [i64; 6]) -> Self {
        Self {
            input_tokens: v[0],
            cached_input_tokens: v[1],
            cache_write_tokens: v[2],
            output_tokens: v[3],
            reasoning_output_tokens: v[4],
            total_tokens: v[5],
        }
    }
    pub fn checked_add(self, rhs: Self) -> Result<Self, Error> {
        let mut out = [0; 6];
        for ((dst, a), b) in out.iter_mut().zip(self.values()).zip(rhs.values()) {
            *dst = a.checked_add(b).ok_or(Error::CounterOverflow)?;
        }
        Ok(Self::from_values(out))
    }
    pub fn subtract(self, rhs: Self) -> Result<Self, Error> {
        let mut out = [0; 6];
        for ((dst, a), b) in out.iter_mut().zip(self.values()).zip(rhs.values()) {
            // Widen before subtraction; clamping negatives must not hide a
            // positive result that cannot be represented by a counter.
            *dst = i64::try_from((i128::from(a) - i128::from(b)).max(0))
                .map_err(|_| Error::CounterOverflow)?;
        }
        Ok(Self::from_values(out))
    }
    pub fn has_tokens(self) -> bool {
        self.values().iter().any(|v| *v > 0)
    }
    pub fn delta(self, previous: Self) -> Result<Self, Error> {
        let reset = if self.total_tokens != 0 || previous.total_tokens != 0 {
            self.total_tokens < previous.total_tokens
        } else {
            [0, 1, 2, 3, 5]
                .iter()
                .any(|i| self.values()[*i] < previous.values()[*i])
        };
        if reset {
            Ok(self)
        } else {
            self.subtract(previous)
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RawEvent {
    pub timestamp: Option<String>,
    pub model: String,
    pub service_tier: String,
    pub total_token_usage: Usage,
    pub last_token_usage: Option<Usage>,
    pub model_context_window: Option<i64>,
    pub rate_limits: Option<serde_json::Value>,
}
/// Normalization borrows source metadata. Raw cumulative counters, request
/// delta and subsequent inherited-prefix normalization remain distinct values.
#[derive(Debug)]
pub struct NormalizedEvent<'a> {
    pub source: &'a RawEvent,
    pub timestamp: DateTime<Utc>,
    pub cumulative: Usage,
    pub delta: Usage,
    pub source_order: usize,
}

pub fn normalize(events: &[RawEvent]) -> Result<Vec<NormalizedEvent<'_>>, Error> {
    let mut ordered: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| timestamp::parse(e.timestamp.as_deref()?).map(|t| (i, e, t)))
        .collect();
    if ordered.windows(2).any(|pair| pair[0].2 > pair[1].2) {
        ordered.sort_by_key(|e| e.2);
    }
    let mut previous = Usage::default();
    let mut result = Vec::with_capacity(ordered.len());
    for (index, (source_order, source, timestamp)) in ordered.into_iter().enumerate() {
        let cumulative = source.total_token_usage;
        if index == 0
            && let Some(last) = source.last_token_usage
        {
            let inferred = cumulative.subtract(last)?;
            if last.has_tokens() && inferred.checked_add(last)? == cumulative {
                previous = inferred;
            }
        }
        let delta = cumulative.delta(previous)?;
        previous = cumulative;
        result.push(NormalizedEvent {
            source,
            timestamp,
            cumulative,
            delta,
            source_order,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_reset_and_overflow_are_explicit() {
        let a = Usage {
            input_tokens: 100,
            total_tokens: 100,
            ..Default::default()
        };
        let b = Usage {
            input_tokens: 20,
            total_tokens: 20,
            ..Default::default()
        };
        assert_eq!(b.delta(a).unwrap(), b);
        assert!(
            Usage {
                total_tokens: i64::MAX,
                ..Default::default()
            }
            .checked_add(a)
            .is_err()
        );
        assert!(serde_json::from_str::<Usage>(r#"{"total_tokens":9223372036854775808}"#).is_err());
    }
    #[test]
    fn input_types_match_legacy() {
        let v: Usage = serde_json::from_str(r#"{"input_tokens":12.9,"output_tokens":true,"total_tokens":"100","cache_write_input_tokens":7}"#).unwrap();
        assert_eq!(v.values(), [12, 0, 7, 1, 0, 0]);
    }
}
