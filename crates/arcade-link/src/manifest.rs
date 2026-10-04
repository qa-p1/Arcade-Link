//! App manifests (`<registry>/<arcade-id>.json`, SPEC §3).

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

use crate::content::{self, Content};
use crate::paths::{self, Locations};

pub const MANIFEST_SCHEMA: u32 = 1;

/// Canonical Arcade IDs. Separate from the platform bundle IDs, which never change.
pub mod ids {
    pub const BOX: &str = "arcade.box";
    pub const LENS: &str = "arcade.lens";
    pub const LOOK: &str = "arcade.look";
    pub const WHEEL: &str = "arcade.wheel";
    pub const CLIPBOARD: &str = "arcade.clipboard";
    pub const TOOLS: &str = "arcade.tools";
    /// The five apps, in the order the Connected apps page lists them.
    pub const APPS: [&str; 5] = [BOX, LENS, LOOK, WHEEL, CLIPBOARD];
}

/// The display name for a canonical ID.
pub fn app_name(id: &str) -> &str {
    match id {
        ids::BOX => "Arcade Box",
        ids::LENS => "Arcade Lens",
        ids::LOOK => "Arcade Look",
        ids::WHEEL => "Arcade Wheel",
        ids::CLIPBOARD => "Arcade Clipboard",
        ids::TOOLS => "Arcade Tools",
        other => other,
    }
}

/// One line describing what a peer adds, for the Connected apps page.
pub fn app_pitch(id: &str) -> &'static str {
    match id {
        ids::BOX => "Convert, compress and transform files with one click.",
        ids::LENS => "Recognize text, codes and colors on screen, and pick a region.",
        ids::LOOK => "Preview any file instantly.",
        ids::WHEEL => "Put any action on a one-gesture radial launcher.",
        ids::CLIPBOARD => "Send content to all your devices, end-to-end encrypted.",
        ids::TOOLS => "Install and update the Arcade apps.",
        _ => "",
    }
}

/// Where to get an app when Arcade Tools isn't installed.
pub fn releases_url(id: &str) -> &'static str {
    match id {
        ids::BOX => "https://github.com/qa-p1/Arcade-box/releases",
        ids::LENS => "https://github.com/qa-p1/Arcade-lens/releases",
        ids::LOOK => "https://github.com/qa-p1/arcade-look/releases",
        ids::WHEEL => "https://github.com/qa-p1/Arcade-wheel/releases",
        ids::CLIPBOARD => "https://github.com/qa-p1/Arcade-clipboard/releases",
        _ => "https://github.com/qa-p1/Arcade-tools/releases",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub link: LinkInfo,
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub launch: Launch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub shortcuts: Vec<Shortcut>,
    #[serde(default)]
    pub settings: ManifestSettings,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub written_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkInfo {
    #[serde(default)]
    pub protocol: Vec<u32>,
}

impl Default for LinkInfo {
    fn default() -> Self {
        LinkInfo {
            protocol: crate::wire::SUPPORTED_PROTOCOLS.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Launch {
    #[serde(default)]
    pub background: Vec<String>,
    /// Omitted when the app has no headless (one-shot) actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    pub id: String,
    pub accelerator: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSettings {
    #[serde(default = "yes")]
    pub link_enabled: bool,
}

fn yes() -> bool {
    true
}

impl Default for ManifestSettings {
    fn default() -> Self {
        ManifestSettings { link_enabled: true }
    }
}

/// One action an app exposes. The same shape is returned by `describe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    #[serde(default = "one")]
    pub version: u32,
    pub title: String,
    #[serde(default)]
    pub verb: String,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub produces: Vec<String>,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default = "local")]
    pub privacy: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default = "yes")]
    pub available: bool,
    /// Why the action is unavailable, e.g. "FFmpeg isn't installed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The tool preset this action runs (Box), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Types for which peers should show this action inline (Box's `link.featuredFor`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub featured_for: Vec<String>,
    /// The largest input the action takes, so callers can disable it with the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// A short grouping label for long lists ("Image", "Pipelines").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

fn one() -> u32 {
    1
}

fn local() -> String {
    "local".into()
}

impl Action {
    pub fn new(id: &str, title: &str, verb: &str) -> Action {
        Action {
            id: id.into(),
            version: 1,
            title: title.into(),
            verb: verb.into(),
            privacy: "local".into(),
            platforms: vec!["linux".into(), "windows".into(), "macos".into()],
            available: true,
            ..Default::default()
        }
    }

    pub fn accepts(mut self, types: &[&str]) -> Action {
        self.accepts = types.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn produces(mut self, types: &[&str]) -> Action {
        self.produces = types.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn effects(mut self, effects: &[&str]) -> Action {
        self.effects = effects.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn interactive(mut self, yes: bool) -> Action {
        self.interactive = yes;
        self
    }

    pub fn platforms(mut self, platforms: &[&str]) -> Action {
        self.platforms = platforms.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn unavailable(mut self, reason: impl Into<String>) -> Action {
        self.available = false;
        self.reason = Some(reason.into());
        self
    }

    pub fn has_effect(&self, effect: &str) -> bool {
        self.effects.iter().any(|e| e == effect)
    }

    /// Supported on this OS (an empty list means everywhere).
    pub fn on_this_platform(&self) -> bool {
        self.platforms.is_empty()
            || self
                .platforms
                .iter()
                .any(|p| p == paths::current_platform())
    }

    /// Takes no input at all.
    pub fn takes_no_input(&self) -> bool {
        self.accepts.is_empty()
    }

    /// Shown for `content`: available here, and accepts it. This is the
    /// ecosystem's "never show a broken entry" rule in one place.
    pub fn offer_for(&self, content: &Content) -> bool {
        self.available
            && self.on_this_platform()
            && content::accepts_content(&self.accepts, content)
    }

    pub fn offer_for_type(&self, kind: &str) -> bool {
        self.available && self.on_this_platform() && content::accepts_type(&self.accepts, kind)
    }
}

impl Manifest {
    pub fn new(id: &str, version: &str, executable: &str) -> Manifest {
        Manifest {
            schema: MANIFEST_SCHEMA,
            id: id.into(),
            name: app_name(id).into(),
            version: version.into(),
            link: LinkInfo::default(),
            executable: executable.into(),
            launch: Launch {
                background: vec!["--background".into()],
                invoke: None,
            },
            icon: None,
            shortcuts: Vec::new(),
            settings: ManifestSettings::default(),
            actions: Vec::new(),
            written_at: String::new(),
        }
    }

    pub fn action(&self, id: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.id == id)
    }

    /// The actions a caller may show: link enabled and the action offered here.
    pub fn usable_actions(&self) -> impl Iterator<Item = &Action> {
        let enabled = self.settings.link_enabled;
        self.actions
            .iter()
            .filter(move |a| enabled && a.available && a.on_this_platform())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("manifest serializes")
    }

    pub fn from_json(text: &str) -> Result<Manifest, String> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if m.schema < 1 {
            return Err("manifest schema must be 1 or later".into());
        }
        if m.id.is_empty() {
            return Err("manifest has no id".into());
        }
        Ok(m)
    }

    /// The manifest's executable exists (readers ignore manifests whose doesn't).
    pub fn executable_exists(&self) -> bool {
        !self.executable.is_empty() && Path::new(&self.executable).is_file()
    }
}

/// The current UTC time as RFC 3339, without a date-time dependency.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil-from-days (Howard Hinnant), valid for the Unix era.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Writes `manifest` atomically, only if it changed (ignoring `writtenAt`).
/// Returns whether the file was rewritten.
pub fn write_manifest(locations: &Locations, manifest: &Manifest) -> io::Result<bool> {
    let path = locations.manifest(&manifest.id);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if let Ok(mut old) = Manifest::from_json(&existing) {
            old.written_at = manifest.written_at.clone();
            if &old == manifest {
                return Ok(false);
            }
        }
    }
    let mut m = manifest.clone();
    m.written_at = now_rfc3339();
    paths::write_atomic(&path, m.to_json().as_bytes(), false)?;
    Ok(true)
}

/// Removes an app's manifest (uninstallers and Arcade Tools).
pub fn remove_manifest(locations: &Locations, app_id: &str) -> io::Result<()> {
    match std::fs::remove_file(locations.manifest(app_id)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The path to advertise as this app's executable: `$APPIMAGE` for an
/// AppImage (never the temporary mount), otherwise the current executable.
pub fn current_executable() -> String {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        return appimage.to_string_lossy().into_owned();
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_shape() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 20);
        assert!(t.starts_with("20") && t.ends_with('Z'));
    }

    #[test]
    fn rewrite_only_when_changed() {
        let dir = std::env::temp_dir().join(format!("arcade-link-manifest-{}", std::process::id()));
        let loc = Locations::under(&dir);
        let mut m = Manifest::new("arcade.test", "1.0.0", "/bin/sh");
        assert!(write_manifest(&loc, &m).unwrap());
        assert!(!write_manifest(&loc, &m).unwrap());
        m.version = "1.0.1".into();
        assert!(write_manifest(&loc, &m).unwrap());
        std::fs::remove_dir_all(dir).ok();
    }
}
