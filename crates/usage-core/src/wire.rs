//! Sparse v1 archives retain their public field presence and unknown extension
//! fields. Only differences from the typed schema are stored, not a second copy
//! of the timeline. This compatibility data is absent for local sessions.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Shape {
    missing: Vec<String>,
    nulls: Vec<String>,
    extras: Map<String, Value>,
    children: BTreeMap<String, Shape>,
    elements: BTreeMap<usize, Shape>,
    missing_as_null: bool,
}
impl Shape {
    pub fn capture(value: &Value, schema: &Value, missing_as_null: bool) -> Self {
        let mut shape = Self {
            missing_as_null,
            ..Self::default()
        };
        if let (Some(value), Some(schema)) = (value.as_object(), schema.as_object()) {
            for key in schema.keys() {
                if !value.contains_key(key) {
                    shape.missing.push(key.clone());
                }
            }
            for (key, value) in value {
                if !schema.contains_key(key) {
                    shape.extras.insert(key.clone(), value.clone());
                    continue;
                }
                if value.is_null() {
                    shape.nulls.push(key.clone());
                    continue;
                }
                // Usage objects are explicitly normalized to the six-field v1
                // representation, including the cache-write alias migration.
                if [
                    "total_token_usage",
                    "last_token_usage",
                    "branch_total_token_usage",
                    "inherited_token_usage",
                ]
                .contains(&key.as_str())
                    && value.is_object()
                {
                    continue;
                }
                let child = Self::capture(value, &schema[key], false);
                if !child.is_empty() {
                    shape.children.insert(key.clone(), child);
                }
            }
        } else if let (Some(values), Some(schema)) = (value.as_array(), schema.as_array())
            && let Some(template) = schema.first()
        {
            for (i, value) in values.iter().enumerate() {
                let child = Self::capture(value, template, false);
                if !child.is_empty() {
                    shape.elements.insert(i, child);
                }
            }
        }
        shape
    }
    fn is_empty(&self) -> bool {
        self.missing.is_empty()
            && self.nulls.is_empty()
            && self.extras.is_empty()
            && self.children.is_empty()
            && self.elements.is_empty()
    }
    pub fn include(&mut self, keys: &[&str]) {
        self.missing.retain(|k| !keys.contains(&k.as_str()));
        self.nulls.retain(|k| !keys.contains(&k.as_str()));
        for key in keys {
            self.children.remove(*key);
            self.extras.remove(*key);
        }
    }
    pub fn replace(&mut self, key: &str, value: Value) {
        self.include(&[key]);
        self.extras.insert(key.into(), value);
    }
    pub fn apply(&self, value: &mut Value) {
        if let Some(object) = value.as_object_mut() {
            for key in &self.missing {
                if self.missing_as_null {
                    object.insert(key.clone(), Value::Null);
                } else {
                    object.remove(key);
                }
            }
            for key in &self.nulls {
                if object.contains_key(key) {
                    object.insert(key.clone(), Value::Null);
                }
            }
            for (key, child) in &self.children {
                if let Some(value) = object.get_mut(key) {
                    child.apply(value);
                }
            }
            for (key, extra) in &self.extras {
                object.insert(key.clone(), extra.clone());
            }
        }
        if let Some(values) = value.as_array_mut() {
            for (i, child) in &self.elements {
                if let Some(value) = values.get_mut(*i) {
                    child.apply(value);
                }
            }
        }
    }
    /// Preserve sparse event/task fields while remapping a period's source rows.
    pub fn retain_array(&mut self, key: &str, indices: &[usize], changed: &[&str]) {
        self.missing.retain(|k| k != key);
        self.nulls.retain(|k| k != key);
        if let Some(array) = self.children.get_mut(key) {
            array.elements = indices
                .iter()
                .enumerate()
                .filter_map(|(to, from)| {
                    let mut shape = array.elements.get(from)?.clone();
                    shape.include(changed);
                    Some((to, shape))
                })
                .collect();
        }
    }
    pub fn apply_event(&self, index: usize, value: &mut Value) {
        if let Some(shape) = self
            .children
            .get("timeline")
            .and_then(|s| s.elements.get(&index))
        {
            shape.apply(value);
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWire {
    pub summary: Shape,
    pub detail: Shape,
}
pub fn detail_schema() -> Value {
    let mut schema = serde_json::to_value(crate::parser::SessionDetail::default()).unwrap();
    schema["estimated_speed"] = serde_json::json!(crate::speed::SpeedTotals::default());
    schema["timeline"] = serde_json::json!([crate::RawEvent::default()]);
    schema["timeline"][0]["request_duration_ms"] = Value::Null;
    schema["timeline"][0]["generation_duration_ms"] = Value::Null;
    schema["tasks"] = serde_json::json!([crate::parser::Task::default()]);
    schema["applied_price_segments"] = serde_json::json!([crate::pricing::PriceSegment::default()]);
    schema["estimated_cost_breakdown_usd"] =
        serde_json::to_value(crate::pricing::CostParts::default()).unwrap();
    schema
}
pub fn summary_schema() -> Value {
    let mut schema = serde_json::to_value(crate::query::SessionSummary::default()).unwrap();
    schema["estimated_speed"] = serde_json::json!(crate::speed::SpeedTotals::default());
    schema["estimated_cost_breakdown_usd"] =
        serde_json::to_value(crate::pricing::CostParts::default()).unwrap();
    schema
}
