//! User shortcut documents and catalog sheets (SPEC §8.5).
//! Unknown fields are ignored. Effective OS bindings, validation and Markdown
//! are identical to the stdlib-only `tools/validate_shortcuts.py`.

use crate::accelerator::{self, Platform};
use crate::manifest::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    pub id: String,
    pub title: String,
    pub keys: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "optional_bool")]
    pub rebindable: Option<bool>,
}

fn optional_bool<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub title: String,
    pub context: String,
    pub shortcuts: Vec<Shortcut>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema: u32,
    pub app: String,
    pub version: String,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub checked_version: String,
    pub sources: Vec<String>,
    pub r#match: BTreeMap<String, Value>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValidationError {}
fn fail(message: impl Into<String>) -> ValidationError {
    ValidationError(message.into())
}

pub(crate) fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.bytes().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-')
}
fn kebab(id: &str) -> bool {
    !id.is_empty() && id.split('-').all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

fn bindings(value: &Value) -> Result<Vec<&str>, ValidationError> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(s) => Ok(vec![s]),
        Value::Array(a) if !a.is_empty() => a.iter().map(|v| v.as_str().ok_or_else(|| fail("key alternatives must be strings"))).collect(),
        _ => Err(fail("keys must be a canonical string, a nonempty array of strings, or null")),
    }
}

impl Shortcut {
    /// OS override, including an explicit null, otherwise default.
    pub fn effective(&self, os: Platform) -> Result<Vec<&str>, ValidationError> {
        self.keys.get(os.as_str()).or_else(|| self.keys.get("default")).map_or(Ok(Vec::new()), bindings)
    }
}

fn validate_groups(groups: &[Group], sheet: bool, manifest: Option<&Manifest>) -> Result<(), ValidationError> {
    let mut ids = HashSet::new();
    let mut used: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
    for group in groups {
        if group.title.trim().is_empty() || !kebab(&group.context) {
            return Err(fail("group needs a title and a kebab-case context"));
        }
        for shortcut in &group.shortcuts {
            if !identifier(&shortcut.id) || !ids.insert(shortcut.id.clone()) {
                return Err(fail(format!("invalid or duplicate id {:?}", shortcut.id)));
            }
            if shortcut.title.trim().is_empty() || shortcut.title.chars().count() > 80 {
                return Err(fail(format!("{}: title must have 1–80 characters", shortcut.id)));
            }
            if shortcut.rebindable.is_some() && (sheet || group.context != "global") {
                return Err(fail(format!("{}: rebindable is only allowed in app global groups", shortcut.id)));
            }
            if shortcut.rebindable == Some(true) {
                if let Some(m) = manifest {
                    if !m.shortcuts.iter().any(|s| s.id == shortcut.id) {
                        return Err(fail(format!("{}: rebindable id is absent from manifest shortcuts", shortcut.id)));
                    }
                }
            }
            let mut nonnull = false;
            for name in ["default", "linux", "windows", "macos"] {
                if let Some(value) = shortcut.keys.get(name) {
                    for key in bindings(value)? {
                        nonnull = true;
                        if !accelerator::is_canonical(key) {
                            return Err(fail(format!("{}: noncanonical key {key:?}", shortcut.id)));
                        }
                    }
                }
            }
            if !nonnull {
                return Err(fail(format!("{}: keys need at least one non-null binding", shortcut.id)));
            }
            for os in Platform::ALL {
                let used = used.entry((group.context.clone(), os.as_str().into())).or_default();
                for key in shortcut.effective(os)? {
                    for (other_id, other_key) in used.iter() {
                        if accelerator::conflicts(key, other_key).map_err(|e| fail(e.to_string()))? {
                            return Err(fail(format!("{}: {os:?} conflict with {other_id} in {} ({key})", shortcut.id, group.context)));
                        }
                    }
                    used.push((shortcut.id.clone(), key.into()));
                }
            }
        }
    }
    Ok(())
}

impl Document {
    pub fn from_json(text: &str) -> Result<Self, ValidationError> {
        let doc: Self = serde_json::from_str(text).map_err(|e| fail(e.to_string()))?;
        doc.validate(None)?;
        Ok(doc)
    }
    pub fn validate(&self, manifest: Option<&Manifest>) -> Result<(), ValidationError> {
        if self.schema != 1 || !self.app.strip_prefix("arcade.").is_some_and(identifier) || self.version.trim().is_empty() {
            return Err(fail("app shortcut docs require schema 1, an Arcade app id and a version"));
        }
        if manifest.is_some_and(|m| m.id != self.app) {
            return Err(fail("manifest belongs to another app"));
        }
        validate_groups(&self.groups, false, manifest)
    }
    pub fn markdown(&self) -> Result<String, ValidationError> {
        self.validate(None)?;
        markdown(&self.groups)
    }
}

impl Sheet {
    pub fn from_json(text: &str) -> Result<Self, ValidationError> {
        let sheet: Self = serde_json::from_str(text).map_err(|e| fail(e.to_string()))?;
        sheet.validate()?;
        Ok(sheet)
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema != 1 || !kebab(&self.id) || self.name.trim().is_empty() || self.checked_version.trim().is_empty() {
            return Err(fail("sheets require schema 1, a kebab-case id, name and checkedVersion"));
        }
        if self.sources.is_empty()
            || self.sources.iter().any(|s| {
                !s.strip_prefix("https://").is_some_and(|s| !s.split('/').next().unwrap_or_default().is_empty() && !s.chars().any(char::is_whitespace))
            })
        {
            return Err(fail("sheets need at least one https source URL"));
        }
        let mut matched = false;
        for (os, field) in [("linux", "class"), ("windows", "exe"), ("macos", "bundle")] {
            if let Some(value) = self.r#match.get(os) {
                let names = value.get(field).and_then(Value::as_array).ok_or_else(|| fail(format!("match.{os}.{field} must be a nonempty array")))?;
                if names.is_empty() || names.iter().any(|n| !n.as_str().is_some_and(|s| !s.trim().is_empty())) {
                    return Err(fail("match names must be nonempty strings"));
                }
                matched = true;
            }
        }
        if !matched {
            return Err(fail("match needs at least one OS"));
        }
        validate_groups(&self.groups, true, None)
    }
    /// Window class/exe/bundle comparisons are case-insensitive.
    pub fn matches(&self, os: Platform, name: &str) -> bool {
        let field = match os {
            Platform::Linux => "class",
            Platform::Windows => "exe",
            Platform::Macos => "bundle",
        };
        self.r#match
            .get(os.as_str())
            .and_then(|v| v.get(field))
            .and_then(Value::as_array)
            .is_some_and(|names| names.iter().filter_map(Value::as_str).any(|n| n.to_lowercase() == name.to_lowercase()))
    }
    pub fn markdown(&self) -> Result<String, ValidationError> {
        self.validate()?;
        markdown(&self.groups)
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('|', "\\|").replace(['\r', '\n'], " ")
}
fn markdown(groups: &[Group]) -> Result<String, ValidationError> {
    let mut result = String::new();
    for group in groups {
        result.push_str(&format!("## {}\n\n| Action | Linux | Windows | macOS |\n| --- | --- | --- | --- |\n", escape(&group.title)));
        for shortcut in &group.shortcuts {
            let mut cells = vec![escape(&shortcut.title)];
            for os in Platform::ALL {
                let keys = shortcut
                    .effective(os)?
                    .iter()
                    .map(|k| accelerator::display(k, os).map(|s| escape(&s)).map_err(|e| fail(e.to_string())))
                    .collect::<Result<Vec<_>, _>>()?;
                cells.push(if keys.is_empty() { "—".into() } else { keys.join("<br>") });
            }
            result.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
        result.push('\n');
    }
    Ok(result)
}
