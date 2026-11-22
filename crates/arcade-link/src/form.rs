//! An action's options form (SPEC §3, `options`): the same shape Arcade Box's catalog uses for its tools, so any
//! caller (an app, the helper's picker, the browser extension) can draw the form of any action without knowing it.
//! A form describes what the user may set; the owning app still validates every request (SPEC §1.5).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The form of an action's options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Form {
    #[serde(default = "one")]
    pub version: u32,
    /// What the action takes as input, for callers that collect it (a picker asking for files, a text box).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<FormInput>,
    #[serde(default)]
    pub controls: Vec<Control>,
}

/// The input part of a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FormInput {
    /// `file`, `files`, `folder`, `text`, `url` or `none`.
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sortable: bool,
}

/// One option the user can set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub key: String,
    pub label: String,
    /// `text`, `number`, `select`, `toggle`, `password`, `directory`, `file` or `color`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Shown only when another control has a value (`{key, equals}` or `{key, oneOf}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_when: Option<ShowWhen>,
    /// Collapsed under "More options" by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub advanced: bool,
}

/// One value of a `select`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Choice {
    pub value: Value,
    pub label: String,
}

/// When a control is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShowWhen {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Value>>,
}

/// The control types a caller can draw. Unknown types are skipped by callers (and left to the app's defaults).
pub const CONTROL_TYPES: [&str; 8] = ["text", "number", "select", "toggle", "password", "directory", "file", "color"];

fn one() -> u32 {
    1
}

impl ShowWhen {
    fn holds(&self, values: &Map<String, Value>) -> bool {
        let v = values.get(&self.key).unwrap_or(&Value::Null);
        if let Some(e) = &self.equals {
            return v == e;
        }
        self.one_of.as_ref().is_none_or(|list| list.contains(v))
    }
}

impl Control {
    /// Shown for these values (a control whose condition names another control follows that control's value).
    pub fn visible(&self, values: &Map<String, Value>) -> bool {
        self.show_when.as_ref().is_none_or(|w| w.holds(values))
    }

    /// `value` as this control accepts it, or why not.
    pub fn check(&self, value: &Value) -> Result<Value, String> {
        let bad = |what: &str| Err(format!("{}: {what}", self.label));
        match self.kind.as_str() {
            "number" => {
                let Some(n) = value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().parse().ok())) else {
                    return bad("enter a number");
                };
                if self.minimum.is_some_and(|m| n < m) || self.maximum.is_some_and(|m| n > m) {
                    let (lo, hi) = (self.minimum.map_or("…".into(), fmt), self.maximum.map_or("…".into(), fmt));
                    return bad(&format!("choose a value from {lo} to {hi}"));
                }
                Ok(serde_json::Number::from_f64(n).map_or(Value::Null, |num| {
                    if n.fract() == 0.0 && n.abs() < 9e15 {
                        Value::from(n as i64)
                    } else {
                        Value::Number(num)
                    }
                }))
            }
            "toggle" => match value {
                Value::Bool(_) => Ok(value.clone()),
                Value::String(s) if s == "true" || s == "false" => Ok(Value::Bool(s == "true")),
                _ => bad("choose on or off"),
            },
            "select" => {
                if self.choices.iter().any(|c| &c.value == value) {
                    Ok(value.clone())
                } else if let Some(c) = value.as_str().and_then(|s| self.choices.iter().find(|c| c.value.as_str() == Some(s))) {
                    Ok(c.value.clone())
                } else {
                    bad("choose one of the listed values")
                }
            }
            _ => match value {
                Value::String(_) => Ok(value.clone()),
                Value::Number(_) | Value::Bool(_) => Ok(Value::String(value.to_string())),
                _ => bad("enter text"),
            },
        }
    }
}

fn fmt(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl Form {
    /// The values a one-click run uses: every control's default.
    pub fn defaults(&self) -> Map<String, Value> {
        self.controls.iter().filter_map(|c| Some((c.key.clone(), c.default.clone()?))).collect()
    }

    /// `values` merged over the defaults, keeping only keys the form has and checking each visible one. Values of
    /// hidden controls are dropped, so a request never carries an option the user couldn't see.
    pub fn resolve(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, String> {
        let mut merged = self.defaults();
        for c in &self.controls {
            if let Some(v) = values.get(&c.key) {
                merged.insert(c.key.clone(), v.clone());
            }
        }
        let snapshot = merged.clone();
        let mut out = Map::new();
        for c in &self.controls {
            if !c.visible(&snapshot) {
                continue;
            }
            if let Some(v) = snapshot.get(&c.key) {
                out.insert(c.key.clone(), c.check(v)?);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resize() -> Form {
        serde_json::from_value(json!({
            "version": 1,
            "input": {"kind": "file", "label": "Image", "minItems": 1, "maxItems": 1},
            "controls": [
                {"key": "mode", "label": "Resize mode", "type": "select", "default": "fit",
                 "choices": [{"value": "fit", "label": "Fit"}, {"value": "percentage", "label": "Percentage"}]},
                {"key": "width", "label": "Width", "type": "number", "minimum": 1, "maximum": 100000, "step": 1,
                 "showWhen": {"key": "mode", "oneOf": ["fit"]}},
                {"key": "percentage", "label": "Percentage", "type": "number", "default": 100, "minimum": 1, "maximum": 1000,
                 "showWhen": {"key": "mode", "equals": "percentage"}},
                {"key": "keep", "label": "Keep metadata", "type": "toggle", "default": false, "advanced": true}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn boxs_catalog_form_reads_and_round_trips() {
        let f = resize();
        assert_eq!(f.controls.len(), 4);
        assert_eq!(f.input.as_ref().unwrap().max_items, Some(1));
        let again: Form = serde_json::from_value(serde_json::to_value(&f).unwrap()).unwrap();
        assert_eq!(again, f);
    }

    #[test]
    fn defaults_and_visibility() {
        let f = resize();
        let d = f.defaults();
        assert_eq!(d.get("mode"), Some(&json!("fit")));
        assert!(!d.contains_key("width"));
        let r = f.resolve(&Map::new()).unwrap();
        assert!(!r.contains_key("percentage"), "hidden while mode is fit: {r:?}");
        let r = f.resolve(json!({"mode": "percentage", "width": 50}).as_object().unwrap()).unwrap();
        assert_eq!(r.get("percentage"), Some(&json!(100)));
        assert!(!r.contains_key("width"));
    }

    #[test]
    fn values_are_checked_and_unknown_keys_dropped() {
        let f = resize();
        let r = f.resolve(json!({"width": "640", "evil": 1, "keep": "true"}).as_object().unwrap()).unwrap();
        assert_eq!(r.get("width"), Some(&json!(640)));
        assert_eq!(r.get("keep"), Some(&json!(true)));
        assert!(!r.contains_key("evil"));
        assert!(f.resolve(json!({"width": 0}).as_object().unwrap()).unwrap_err().contains("from 1 to 100000"));
        assert!(f.resolve(json!({"mode": "stretch"}).as_object().unwrap()).is_err());
    }
}
