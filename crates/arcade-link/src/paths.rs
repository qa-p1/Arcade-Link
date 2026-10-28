//! Where the registry, endpoints and handoff files live (SPEC §2).
//!
//! `ARCADE_HOME`, if set, replaces every root: `$ARCADE_HOME/apps`,
//! `$ARCADE_HOME/run` and `$ARCADE_HOME/handoff`.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn arcade_home() -> Option<PathBuf> {
    env::var_os("ARCADE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute())
}

fn home() -> PathBuf {
    env_dir(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_else(env::temp_dir)
}

#[cfg(windows)]
fn local_app_data() -> PathBuf {
    env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local"))
}

/// The directory holding one manifest per installed app.
pub fn registry_dir() -> PathBuf {
    if let Some(h) = arcade_home() {
        return h.join("apps");
    }
    #[cfg(target_os = "linux")]
    {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("arcade/apps")
    }
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Application Support/Arcade/apps")
    }
    #[cfg(windows)]
    {
        local_app_data().join("Arcade").join("apps")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        home().join(".local/share/arcade/apps")
    }
}

/// The private directory holding endpoint files (and sockets on Unix).
pub fn runtime_dir() -> PathBuf {
    if let Some(h) = arcade_home() {
        return h.join("run");
    }
    #[cfg(target_os = "linux")]
    {
        match env_dir("XDG_RUNTIME_DIR") {
            Some(d) => d.join("arcade"),
            None => env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache")).join("arcade/run"),
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Sockets under ~/Library/Application Support can exceed macOS's
        // 104-byte socket path limit; $TMPDIR is per user and short.
        env::temp_dir().join("arcade")
    }
    #[cfg(windows)]
    {
        local_app_data().join("Arcade").join("run")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        home().join(".cache/arcade/run")
    }
}

/// Where in-memory content is written to cross between apps. On disk, not
/// tmpfs: a large video must not end up in RAM.
pub fn handoff_dir() -> PathBuf {
    if let Some(h) = arcade_home() {
        return h.join("handoff");
    }
    #[cfg(target_os = "linux")]
    {
        env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache")).join("arcade/handoff")
    }
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Caches/Arcade/handoff")
    }
    #[cfg(windows)]
    {
        local_app_data().join("Arcade").join("handoff")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        home().join(".cache/arcade/handoff")
    }
}

/// The three roots, resolved once. Tests use [`Locations::under`] instead of
/// changing the process environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    pub registry: PathBuf,
    pub runtime: PathBuf,
    pub handoff: PathBuf,
}

impl Locations {
    /// The platform locations, honoring `ARCADE_HOME`.
    pub fn discover() -> Locations {
        Locations { registry: registry_dir(), runtime: runtime_dir(), handoff: handoff_dir() }
    }

    /// Every root under `root`, the same layout as `ARCADE_HOME`.
    pub fn under(root: &Path) -> Locations {
        Locations { registry: root.join("apps"), runtime: root.join("run"), handoff: root.join("handoff") }
    }

    pub fn manifest(&self, app_id: &str) -> PathBuf {
        self.registry.join(format!("{app_id}.json"))
    }

    pub fn endpoint(&self, app_id: &str) -> PathBuf {
        self.runtime.join(format!("{app_id}.endpoint"))
    }
}

/// "linux", "windows" or "macos": the names used in manifests' `platforms`.
pub fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Creates `dir` (and parents) and makes it private to the current user.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let meta = fs::symlink_metadata(dir)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} is not a real directory", dir.display())));
        }
        // SAFETY: geteuid has no preconditions.
        let uid = unsafe { geteuid() };
        if meta.uid() != uid {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{} belongs to another user", dir.display())));
        }
        if meta.mode() & 0o777 != 0o700 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
extern "C" {
    fn geteuid() -> u32;
}

/// The current user's id on Unix (0 elsewhere); part of fallback socket names.
pub fn user_id() -> u32 {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        unsafe { geteuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Runs a file operation that Windows can briefly refuse with "access denied"
/// while another handle replaces or reads the same file (concurrent atomic
/// writers). Other platforms and other errors return immediately.
pub(crate) fn retry_sharing<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    #[cfg(windows)]
    {
        let mut delay = 1;
        for _ in 0..40 {
            match op() {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    delay = (delay * 2).min(25);
                }
                result => return result,
            }
        }
    }
    op()
}

/// Writes `bytes` to `path` through a temporary file and an atomic rename.
/// The file is readable only by the current user on Unix.
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    use std::io::Write;
    let dir = path.parent().ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let tmp = dir.join(format!(".{name}.{suffix}.tmp"));
    let result = (|| {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(if private { 0o600 } else { 0o644 });
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        retry_sharing(|| fs::rename(&tmp, path))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_matches_the_arcade_home_layout() {
        let dir = env::temp_dir().join("arcade-link-paths-test");
        let l = Locations::under(&dir);
        assert_eq!(l.manifest("arcade.box"), dir.join("apps").join("arcade.box.json"));
        assert_eq!(l.endpoint("arcade.box"), dir.join("run").join("arcade.box.endpoint"));
        assert_eq!(l.handoff, dir.join("handoff"));
    }

    #[test]
    fn private_dir_is_0700() {
        let dir = env::temp_dir().join(format!("arcade-link-private-{}", std::process::id()));
        ensure_private_dir(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        fs::remove_dir_all(dir).ok();
    }
}

// ---- App folders (STANDARD.md §10) ------------------------------------------

#[rustfmt::skip]
mod legacy_dirs;

use crate::accelerator::Platform;

/// What an app folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Config,
    Data,
    Cache,
}

/// The per-user base folder a legacy location is relative to: XDG on Linux,
/// `%APPDATA%`/`%LOCALAPPDATA%` on Windows, `~/Library/…` on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    Config,
    Data,
    Cache,
    Roaming,
    Local,
    Support,
    Caches,
    Preferences,
    Webkit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyEntry {
    pub kind: Kind,
    pub base: Base,
    pub path: &'static str,
    /// Subfolder of the new folder it moves into ("" = the folder itself).
    pub into: &'static str,
}

/// The current user's base folders. [`UserDirs::under`] roots them for
/// isolated runs and tests, with the same layout as Arcade Tools' isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDirs {
    pub home: PathBuf,
    /// Linux `XDG_CONFIG_HOME`.
    pub config: PathBuf,
    /// Linux `XDG_DATA_HOME`.
    pub data: PathBuf,
    /// Linux `XDG_CACHE_HOME`.
    pub cache: PathBuf,
    /// Windows `%APPDATA%`.
    pub roaming: PathBuf,
    /// Windows `%LOCALAPPDATA%`.
    pub local: PathBuf,
}

impl UserDirs {
    pub fn discover() -> UserDirs {
        let home = home();
        UserDirs {
            config: env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            data: env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
            cache: env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")),
            roaming: env_dir("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming")),
            local: env_dir("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local")),
            home,
        }
    }

    pub fn under(root: &Path) -> UserDirs {
        UserDirs {
            home: root.into(),
            config: root.join(".config"),
            data: root.join(".local/share"),
            cache: root.join(".cache"),
            roaming: root.join("AppData/Roaming"),
            local: root.join("AppData/Local"),
        }
    }

    fn base(&self, base: Base) -> PathBuf {
        let library = self.home.join("Library");
        match base {
            Base::Config => self.config.clone(),
            Base::Data => self.data.clone(),
            Base::Cache => self.cache.clone(),
            Base::Roaming => self.roaming.clone(),
            Base::Local => self.local.clone(),
            Base::Support => library.join("Application Support"),
            Base::Caches => library.join("Caches"),
            Base::Preferences => library.join("Preferences"),
            Base::Webkit => library.join("WebKit"),
        }
    }
}

/// The platform this build runs on.
pub fn current_os() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Linux
    }
}

/// An app's own folders: `~/.config/arcade/<app>`, `~/.local/share/arcade/<app>`
/// and `~/.cache/arcade/<app>` on Linux; `%APPDATA%\Arcade\<App>` (cache in
/// `%LOCALAPPDATA%\Arcade\<App>`) on Windows; `~/Library/Application
/// Support/Arcade/<App>` (cache in `~/Library/Caches/Arcade/<App>`) on macOS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl AppDirs {
    pub fn get(&self, kind: Kind) -> &Path {
        match kind {
            Kind::Config => &self.config,
            Kind::Data => &self.data,
            Kind::Cache => &self.cache,
        }
    }

    /// `config/settings.json`, the standard settings file.
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }
}

fn table(app_id: &str) -> (String, String, Option<&'static legacy_dirs::AppTable>) {
    match legacy_dirs::APPS.iter().find(|row| row.id == app_id) {
        Some(row) => (row.folder.into(), row.name.into(), Some(row)),
        None => {
            let folder = app_id.strip_prefix("arcade.").unwrap_or(app_id).to_string();
            let mut name = folder.clone();
            if let Some(first) = name.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            (folder, name, None)
        }
    }
}

/// [`app_dirs_in`] for the current user and platform.
pub fn app_dirs(app_id: &str) -> AppDirs {
    app_dirs_in(&UserDirs::discover(), current_os(), app_id)
}

pub fn app_dirs_in(user: &UserDirs, platform: Platform, app_id: &str) -> AppDirs {
    let (folder, name, _) = table(app_id);
    match platform {
        Platform::Linux => AppDirs {
            config: user.config.join("arcade").join(&folder),
            data: user.data.join("arcade").join(&folder),
            cache: user.cache.join("arcade").join(&folder),
        },
        Platform::Windows => {
            let roaming = user.roaming.join("Arcade").join(&name);
            AppDirs { config: roaming.clone(), data: roaming, cache: user.local.join("Arcade").join(&name) }
        }
        Platform::Macos => {
            let support = user.base(Base::Support).join("Arcade").join(&name);
            AppDirs { config: support.clone(), data: support, cache: user.base(Base::Caches).join("Arcade").join(&name) }
        }
    }
}

/// An older location of an app's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDir {
    pub kind: Kind,
    pub path: PathBuf,
    /// Where its files go, relative to the new `kind` folder; empty for the
    /// folder itself. A Tauri app's old webview storage, for example, moves
    /// into a `webview` subfolder.
    pub into: PathBuf,
}

impl LegacyDir {
    /// The folder this location's files move into.
    pub fn target(&self, dirs: &AppDirs) -> PathBuf {
        let base = dirs.get(self.kind);
        if self.into.as_os_str().is_empty() {
            base.to_path_buf()
        } else {
            base.join(&self.into)
        }
    }
}

/// [`legacy_dirs_in`] for the current user and platform.
pub fn legacy_dirs(app_id: &str) -> Vec<LegacyDir> {
    legacy_dirs_in(&UserDirs::discover(), current_os(), app_id)
}

/// Every location `app_id` used before the `arcade/<app>` layout, existing or
/// not. Arcade Tools' backup, restore and uninstall read these too.
pub fn legacy_dirs_in(user: &UserDirs, platform: Platform, app_id: &str) -> Vec<LegacyDir> {
    let Some(row) = table(app_id).2 else { return Vec::new() };
    let entries = match platform {
        Platform::Linux => row.linux,
        Platform::Windows => row.windows,
        Platform::Macos => row.macos,
    };
    entries.iter().map(|e| LegacyDir { kind: e.kind, path: user.base(e.base).join(e.path), into: PathBuf::from(e.into) }).collect()
}

/// What [`migrate_app_dirs`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Legacy folders copied, verified and removed: (from, to).
    pub moved: Vec<(PathBuf, PathBuf)>,
    /// Legacy folders left in place, with the reason (logged by the app).
    pub kept: Vec<(PathBuf, String)>,
}

impl Migration {
    pub fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.kept.is_empty()
    }
}

/// [`migrate_app_dirs_in`] for the current user and platform.
pub fn migrate_app_dirs(app_id: &str) -> Migration {
    migrate_app_dirs_in(&UserDirs::discover(), current_os(), app_id)
}

/// Moves every existing legacy folder into the app's new folders: copy,
/// verify byte for byte, and only then remove the old folder. A file that
/// already exists in the new folder with different contents, a link, or any
/// I/O error leaves that legacy folder untouched (and removes what this run
/// copied); the reason is in [`Migration::kept`]. Safe to run on every start.
pub fn migrate_app_dirs_in(user: &UserDirs, platform: Platform, app_id: &str) -> Migration {
    let dirs = app_dirs_in(user, platform, app_id);
    let mut report = Migration::default();
    for legacy in legacy_dirs_in(user, platform, app_id) {
        let to = legacy.target(&dirs);
        let from = legacy.path;
        let Ok(meta) = fs::symlink_metadata(&from) else { continue };
        if !meta.is_dir() {
            report.kept.push((from, "not a real folder".into()));
            continue;
        }
        if to.starts_with(&from) || from.starts_with(&to) {
            report.kept.push((from, format!("overlaps {}", to.display())));
            continue;
        }
        match migrate_one(&from, &to) {
            Ok(()) => {
                remove_empty_parents(&from, user, platform);
                report.moved.push((from, to));
            }
            Err(reason) => report.kept.push((from, reason)),
        }
    }
    report
}

fn files_under(root: &Path, out: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root).map_err(|e| format!("can't read {}: {e}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("can't read {}: {e}", root.display()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| format!("can't inspect {}: {e}", path.display()))?;
        if kind.is_symlink() {
            return Err(format!("{} is a link", path.display()));
        } else if kind.is_dir() {
            dirs.push(path.clone());
            files_under(&path, out, dirs)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn same_contents(a: &Path, b: &Path) -> io::Result<bool> {
    let (ma, mb) = (fs::metadata(a)?, fs::metadata(b)?);
    Ok(ma.len() == mb.len() && fs::read(a)? == fs::read(b)?)
}

fn migrate_one(from: &Path, to: &Path) -> Result<(), String> {
    let (mut files, mut folders) = (Vec::new(), Vec::new());
    files_under(from, &mut files, &mut folders)?;
    // 1. Refuse conflicts before writing anything.
    for file in &files {
        let target = to.join(file.strip_prefix(from).expect("under from"));
        if target.exists() && !same_contents(file, &target).unwrap_or(false) {
            return Err(format!("{} already exists with different contents", target.display()));
        }
    }
    // 2. Copy what is missing, keeping modification times.
    let mut copied = Vec::new();
    let result = (|| -> io::Result<()> {
        fs::create_dir_all(to)?;
        for folder in &folders {
            fs::create_dir_all(to.join(folder.strip_prefix(from).expect("under from")))?;
        }
        for file in &files {
            let target = to.join(file.strip_prefix(from).expect("under from"));
            if target.exists() {
                continue;
            }
            fs::copy(file, &target)?;
            copied.push(target.clone());
            if let Ok(modified) = fs::metadata(file).and_then(|m| m.modified()) {
                let _ = fs::File::options().write(true).open(&target).and_then(|f| f.set_modified(modified));
            }
        }
        // 3. Verify every byte before removing anything.
        for file in &files {
            let target = to.join(file.strip_prefix(from).expect("under from"));
            if !same_contents(file, &target)? {
                return Err(io::Error::other(format!("{} differs after copying", target.display())));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        for path in copied {
            let _ = fs::remove_file(path);
        }
        return Err(format!("couldn't copy {} to {}: {e}", from.display(), to.display()));
    }
    // 4. Remove the old folder.
    fs::remove_dir_all(from).map_err(|e| format!("copied, but couldn't remove {}: {e}", from.display()))
}

/// Removes now-empty parents such as `~/.config/Arcade Wheel` after its
/// `Arcade Wheel` child moved, stopping at the platform base folder.
fn remove_empty_parents(path: &Path, user: &UserDirs, platform: Platform) {
    let bases: Vec<PathBuf> = match platform {
        Platform::Linux => vec![user.config.clone(), user.data.clone(), user.cache.clone()],
        Platform::Windows => vec![user.roaming.clone(), user.local.clone()],
        Platform::Macos => [Base::Support, Base::Caches, Base::Preferences, Base::Webkit].iter().map(|b| user.base(*b)).collect(),
    };
    let mut parent = path.parent();
    while let Some(dir) = parent {
        if bases.iter().any(|b| b == dir) || fs::remove_dir(dir).is_err() {
            break;
        }
        parent = dir.parent();
    }
}

#[cfg(test)]
mod app_dir_tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arcade-appdirs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn new_layout_per_platform() {
        let user = UserDirs::under(Path::new("/r"));
        let linux = app_dirs_in(&user, Platform::Linux, "arcade.box");
        assert_eq!(linux.config, Path::new("/r/.config/arcade/box"));
        assert_eq!(linux.data, Path::new("/r/.local/share/arcade/box"));
        assert_eq!(linux.cache, Path::new("/r/.cache/arcade/box"));
        assert_eq!(linux.settings_file(), Path::new("/r/.config/arcade/box/settings.json"));
        let windows = app_dirs_in(&user, Platform::Windows, "arcade.clipboard");
        assert_eq!(windows.data, Path::new("/r/AppData/Roaming/Arcade/Clipboard"));
        assert_eq!(windows.cache, Path::new("/r/AppData/Local/Arcade/Clipboard"));
        let mac = app_dirs_in(&user, Platform::Macos, "arcade.find");
        assert_eq!(mac.config, Path::new("/r/Library/Application Support/Arcade/Find"));
        assert_eq!(mac.cache, Path::new("/r/Library/Caches/Arcade/Find"));
        assert_eq!(app_dirs_in(&user, Platform::Linux, "arcade.new").data, Path::new("/r/.local/share/arcade/new"));
    }

    #[test]
    fn every_app_lists_its_old_locations() {
        let user = UserDirs::under(Path::new("/r"));
        for app in crate::manifest::ids::APPS.iter().chain([&crate::manifest::ids::TOOLS]) {
            for platform in Platform::ALL {
                assert!(!legacy_dirs_in(&user, platform, app).is_empty(), "{app} {platform:?}");
            }
        }
        let wheel = legacy_dirs_in(&user, Platform::Linux, "arcade.wheel");
        assert_eq!(wheel[0], LegacyDir { kind: Kind::Config, path: "/r/.config/Arcade Wheel/Arcade Wheel".into(), into: PathBuf::new() });
    }

    #[test]
    fn migration_copies_verifies_removes_and_is_idempotent() {
        let root = temp("move");
        let user = UserDirs::under(&root);
        let old = root.join(".config/Arcade Wheel/Arcade Wheel");
        fs::create_dir_all(old.join("decks")).unwrap();
        fs::write(old.join("config.json"), b"{\"v\":1}").unwrap();
        fs::write(old.join("decks/main.json"), b"[]").unwrap();
        let report = migrate_app_dirs_in(&user, Platform::Linux, "arcade.wheel");
        assert_eq!(report.moved.len(), 1, "{report:?}");
        let new = root.join(".config/arcade/wheel");
        assert_eq!(fs::read(new.join("config.json")).unwrap(), b"{\"v\":1}");
        assert_eq!(fs::read(new.join("decks/main.json")).unwrap(), b"[]");
        assert!(!old.exists() && !root.join(".config/Arcade Wheel").exists());
        assert!(migrate_app_dirs_in(&user, Platform::Linux, "arcade.wheel").is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn webview_storage_moves_into_its_subfolder() {
        let root = temp("into");
        let user = UserDirs::under(&root);
        let state = root.join(".local/share/arcade-tools");
        let webview = root.join(".local/share/app.arcadetools");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(webview.join("localstorage")).unwrap();
        fs::write(state.join("preferences.json"), b"{}").unwrap();
        fs::write(webview.join("localstorage/file.db"), b"db").unwrap();
        let report = migrate_app_dirs_in(&user, Platform::Linux, "arcade.tools");
        assert_eq!(report.moved.len(), 2, "{report:?}");
        assert_eq!(fs::read(root.join(".local/share/arcade/tools/preferences.json")).unwrap(), b"{}");
        assert_eq!(fs::read(root.join(".cache/arcade/tools/webview/localstorage/file.db")).unwrap(), b"db");
        assert!(!state.exists() && !webview.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_conflict_leaves_everything_in_place() {
        let root = temp("conflict");
        let user = UserDirs::under(&root);
        let old = root.join(".config/arcade-find");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("settings.json"), b"old").unwrap();
        fs::write(old.join("extra.json"), b"x").unwrap();
        let new = root.join(".config/arcade/find");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("settings.json"), b"new").unwrap();
        let report = migrate_app_dirs_in(&user, Platform::Linux, "arcade.find");
        assert!(report.moved.is_empty());
        assert_eq!(report.kept.len(), 1);
        assert_eq!(fs::read(old.join("settings.json")).unwrap(), b"old");
        assert_eq!(fs::read(new.join("settings.json")).unwrap(), b"new");
        assert!(!new.join("extra.json").exists(), "nothing copied on conflict");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identical_files_and_several_sources_merge() {
        let root = temp("merge");
        let user = UserDirs::under(&root);
        for (dir, file) in [("dev.arcadebox.app", "history.json"), ("arcadebox", "presets.json")] {
            let old = root.join(".local/share").join(dir);
            fs::create_dir_all(&old).unwrap();
            fs::write(old.join(file), file.as_bytes()).unwrap();
            fs::write(old.join("same.txt"), b"same").unwrap();
        }
        let report = migrate_app_dirs_in(&user, Platform::Linux, "arcade.box");
        assert_eq!(report.moved.len(), 2, "{report:?}");
        let new = root.join(".local/share/arcade/box");
        for file in ["history.json", "presets.json", "same.txt"] {
            assert!(new.join(file).is_file(), "{file}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
