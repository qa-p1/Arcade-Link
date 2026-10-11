//! Verbatim shared dialog strings. The caller supplies UI layout and consent.
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

pub const JSON: &str = include_str!("../../../../assets/strings/install.json");
#[derive(Debug, Deserialize)]
pub struct Dialog {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    pub buttons: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Strings {
    pub schema: u32,
    pub not_installed: Dialog,
    pub older: Dialog,
    pub same_or_newer: Dialog,
    pub managed_by_tools: String,
    pub installed_here: String,
    pub macos: Dialog,
    pub path_hint: String,
    pub errors: BTreeMap<String, String>,
}
pub fn get() -> &'static Strings {
    static STRINGS: OnceLock<Strings> = OnceLock::new();
    STRINGS.get_or_init(|| serde_json::from_str(JSON).expect("bundled install strings"))
}
/// Substitute only the four contract placeholders, in one pass (replacement
/// values containing `{app}` are left literal).
pub fn format(template: &str, app: &str, version: &str, from: &str, to: &str) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        result.push_str(&rest[..i]);
        rest = &rest[i..];
        let replacement = [("{app}", app), ("{version}", version), ("{from}", from), ("{to}", to)].into_iter().find(|(k, _)| rest.starts_with(k));
        if let Some((k, v)) = replacement {
            result.push_str(v);
            rest = &rest[k.len()..];
        } else {
            result.push('{');
            rest = &rest[1..];
        }
    }
    result.push_str(rest);
    result
}
