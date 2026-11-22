//! The menu entries and browser host files the helper writes (PLAN §4.5), per user and without admin rights.
//!
//! Every writer is a pure function from "what to show" to a [`Plan`]; [`apply`] makes the disk match it, writing
//! only files whose content changed, and remembers what it wrote (`integrations.json`) so a later plan, or
//! [`remove_all`] at uninstall, removes exactly our entries and nothing else.

pub mod browsers;
pub mod linux;
pub mod windows;

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// One action with its own menu entry ("pinned"), or the "Arcade…" picker when `reference` is empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub reference: String,
    pub title: String,
    pub accepts: Vec<String>,
}

impl Entry {
    pub fn picker() -> Entry {
        Entry { reference: String::new(), title: "Arcade…".into(), accepts: vec!["*".into(), "folder/reference".into()] }
    }

    pub fn is_picker(&self) -> bool {
        self.reference.is_empty()
    }

    /// The helper arguments for this entry, before the selected files.
    pub fn args(&self) -> Vec<String> {
        if self.is_picker() {
            vec!["menu".into()]
        } else {
            vec!["run".into(), self.reference.clone()]
        }
    }
}

/// Where per-user files live. Tests point every root at a temporary folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    pub home: PathBuf,
    /// `$XDG_DATA_HOME` (Linux).
    pub data: PathBuf,
    /// `$XDG_CONFIG_HOME` (Linux).
    pub config: PathBuf,
    /// `%LOCALAPPDATA%` (Windows), `~/Library/Application Support` (macOS); unused on Linux.
    pub app_data: PathBuf,
}

impl Home {
    pub fn discover() -> Home {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from).filter(|p| p.is_absolute());
        let home = env(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_else(std::env::temp_dir);
        Home {
            data: env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
            config: env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            app_data: if cfg!(windows) { env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local")) } else { home.join("Library/Application Support") },
            home,
        }
    }

    pub fn under(root: &Path) -> Home {
        Home { home: root.to_path_buf(), data: root.join(".local/share"), config: root.join(".config"), app_data: root.join("AppData") }
    }
}

/// A file to write.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub path: PathBuf,
    pub contents: String,
    pub executable: bool,
}

/// A registry key with string values (Windows), relative to `HKEY_CURRENT_USER`. `""` is the default value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegKey {
    pub path: String,
    pub values: Vec<(String, String)>,
}

/// Everything one platform's integration consists of.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub files: Vec<File>,
    /// Thunar's shared `uca.xml`: our entries are merged into the user's file, never replacing theirs.
    pub thunar: Option<(PathBuf, Vec<linux::ThunarAction>)>,
    pub registry: Vec<RegKey>,
    /// Registry trees we own as a whole (removed before rewriting, and at uninstall).
    pub registry_roots: Vec<String>,
}

/// What a previous [`apply`] wrote.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub files: Vec<PathBuf>,
    pub thunar: Option<PathBuf>,
    pub registry_roots: Vec<String>,
}

impl State {
    pub fn load(path: &Path) -> State {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
}

/// What changed on disk.
#[derive(Debug, Default, PartialEq)]
pub struct Applied {
    pub written: usize,
    pub removed: usize,
}

fn write_if_changed(file: &File) -> io::Result<bool> {
    if std::fs::read_to_string(&file.path).is_ok_and(|old| old == file.contents) && (!file.executable || is_executable(&file.path)) {
        return Ok(false);
    }
    if let Some(dir) = file.path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.path.with_extension("arcade-tmp");
    std::fs::write(&tmp, &file.contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = if file.executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    }
    std::fs::rename(&tmp, &file.path)?;
    Ok(true)
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// Makes the disk match `plan`; `state_path` records what we own.
pub fn apply(plan: &Plan, state_path: &Path) -> io::Result<Applied> {
    let old = State::load(state_path);
    let mut done = Applied::default();
    for f in &plan.files {
        if write_if_changed(f)? {
            done.written += 1;
        }
    }
    for stale in old.files.iter().filter(|p| !plan.files.iter().any(|f| &f.path == *p)) {
        if std::fs::remove_file(stale).is_ok() {
            done.removed += 1;
            remove_empty_parent(stale);
        }
    }
    match &plan.thunar {
        Some((path, actions)) => {
            if linux::merge_thunar(path, actions)? {
                done.written += 1;
            }
        }
        None => {
            if let Some(path) = &old.thunar {
                if linux::merge_thunar(path, &[])? {
                    done.removed += 1;
                }
            }
        }
    }
    if !plan.registry.is_empty() || !old.registry_roots.is_empty() {
        windows::apply(&old.registry_roots, &plan.registry_roots, &plan.registry)?;
    }
    let state = State {
        files: plan.files.iter().map(|f| f.path.clone()).collect(),
        thunar: plan.thunar.as_ref().map(|(p, _)| p.clone()),
        registry_roots: plan.registry_roots.clone(),
    };
    if let Some(dir) = state_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(&state).map_err(io::Error::other)?;
    if std::fs::read_to_string(state_path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(state_path, text)?;
    }
    Ok(done)
}

/// Removes every entry a previous [`apply`] wrote (Tools' uninstall).
pub fn remove_all(state_path: &Path) -> io::Result<Applied> {
    let done = apply(&Plan::default(), state_path)?;
    std::fs::remove_file(state_path).ok();
    Ok(done)
}

/// Removes our own folders once they're empty (`nautilus/scripts/Arcade`), never anything else.
fn remove_empty_parent(path: &Path) {
    if let Some(dir) = path.parent() {
        if dir.file_name().is_some_and(|n| n == "Arcade") {
            std::fs::remove_dir(dir).ok();
        }
    }
}

/// The whole plan for this computer.
pub fn plan_for_this_os(exe: &Path, entries: &[Entry], home: &Home) -> Plan {
    let mut plan = if cfg!(windows) {
        windows::plan(exe, entries)
    } else if cfg!(target_os = "macos") {
        // Finder's entries are Services declared by the Arcade Tools app bundle; nothing per user to write.
        Plan::default()
    } else {
        linux::plan(exe, entries, home)
    };
    let browsers = browsers::plan(exe, home);
    plan.files.extend(browsers.files);
    plan.registry.extend(browsers.registry);
    plan.registry_roots.extend(browsers.registry_roots);
    plan
}
