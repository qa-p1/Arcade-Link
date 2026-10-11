//! Typed results for optional v0.3 methods. Protocol version remains 1.
use crate::manifest::Action;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Description {
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Missing on v0.2 peers means no advertised optional methods.
    #[serde(default)]
    pub methods: Vec<String>,
}
impl Description {
    pub fn supports(&self, method: &str) -> bool {
        self.methods.iter().any(|m| m == method)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShortcutVia {
    Native,
    Portal,
    HyprlandRuntime,
    Manual,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutSetResult {
    pub applied: bool,
    pub via: ShortcutVia,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: bool,
    pub restart_required: bool,
}
