//! Explicit-path integration, called on a worker thread after user consent.
//! `Environment::under` isolates *every* filesystem write. No profile is edited.
//! App/toolkit code owns dialogs, consent, restart waiting and launch policy.

use crate::{
    manifest,
    paths::Locations,
    receipt::{ManagedBy, Receipt, Store},
};
#[cfg(target_os = "linux")]
use crate::{
    paths,
    receipt::{InstallMethod, Integration, Previous},
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub mod platform;
pub mod strings;

#[derive(Debug, Clone)]
pub struct Environment {
    pub home: PathBuf,
    pub data: PathBuf,
    pub config: PathBuf,
    pub applications: PathBuf,
    pub bin: PathBuf,
    pub local_app_data: PathBuf,
    pub locations: Locations,
    /// The PATH to inspect, not a process environment mutation.
    pub path: Option<std::ffi::OsString>,
    /// HKCU subkey used for Windows PATH operations. Isolated environments
    /// use a test key, never the real HKCU Environment login configuration.
    pub windows_environment_key: String,
}
impl Environment {
    pub fn under(root: &Path) -> Self {
        Self {
            home: root.into(),
            data: root.join(".local/share"),
            config: root.join(".config"),
            applications: root.join("Applications/Arcade"),
            bin: if cfg!(windows) { root.join("bin") } else { root.join(".local/bin") },
            local_app_data: root.join("AppData/Local"),
            locations: Locations::under(root),
            path: None,
            windows_environment_key: format!("Software\\ArcadeLink\\Isolated\\{}", crate::transport::short_hash(&root.to_string_lossy())),
        }
    }
    pub fn discover() -> io::Result<Self> {
        if let Some(root) = env::var_os("ARCADE_HOME").filter(|s| !s.is_empty()) {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return Err(input("ARCADE_HOME must be absolute for installation"));
            }
            let mut e = Self::under(&root);
            e.path = env::var_os("PATH");
            return Ok(e);
        }
        let get = |key| env::var_os(key).map(PathBuf::from).filter(|p| p.is_absolute());
        let home = get(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or_else(|| input("an absolute home is required"))?;
        let mut e = Self::under(&home);
        e.data = get("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        e.config = get("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        e.local_app_data = get("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"));
        if cfg!(windows) {
            e.bin = e.local_app_data.join("Arcade/bin");
        }
        e.locations = Locations::discover();
        e.path = env::var_os("PATH");
        e.windows_environment_key = "Environment".into();
        Ok(e)
    }
    pub fn receipts(&self) -> Store {
        Store::new(&self.locations)
    }
}

#[derive(Debug, Clone)]
pub struct Runtime {
    pub executable: PathBuf,
    pub appimage: Option<PathBuf>,
    pub appdir: Option<PathBuf>,
    pub dev_build: bool,
}
impl Runtime {
    pub fn discover() -> io::Result<Self> {
        Ok(Self {
            executable: env::current_exe()?,
            appimage: env::var_os("APPIMAGE").map(PathBuf::from),
            appdir: env::var_os("APPDIR").map(PathBuf::from),
            dev_build: env::var("ARCADE_DEV_BUILD").as_deref() == Ok("1"),
        })
    }
    pub fn running_from(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        if let Some(bundle) = platform::macos::bundle_for(&self.executable) {
            return bundle;
        }
        if self.is_appimage() {
            self.appimage.clone().unwrap()
        } else {
            self.executable.clone()
        }
    }
    pub fn is_appimage(&self) -> bool {
        self.appimage.as_ref().is_some_and(|p| p.is_absolute() && p.is_file())
            && self.appdir.as_ref().is_some_and(|p| p.is_absolute() && canonical(&self.executable).starts_with(canonical(p)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum State {
    NotInstalled,
    InstalledHere,
    InstalledElsewhere { version: String },
    DevBuild,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub running_from: PathBuf,
    pub is_app_image: bool,
    pub receipt: Option<Receipt>,
    pub installed_path: Option<PathBuf>,
    pub state: State,
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.into())
}
fn input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Source trees use either a .git directory or a worktree .git file. Build
/// trees without git are recognized through their source manifest.
pub fn is_dev_build(executable: &Path) -> bool {
    let executable = canonical(executable);
    executable.ancestors().any(|p| {
        p.join(".git").exists()
            || (p.file_name().is_some_and(|n| n == "target" || n == "build")
                && p.parent().is_some_and(|r| ["Cargo.toml", "CMakeLists.txt", "package.json", "pubspec.yaml"].iter().any(|n| r.join(n).is_file())))
    })
}

pub fn detect(environment: &Environment, id: &str, runtime: &Runtime) -> io::Result<Detection> {
    let running_from = runtime.running_from();
    let receipt = environment.receipts().read(id)?;
    let installed_path = receipt.as_ref().filter(|r| r.path.exists()).map(|r| r.path.clone());
    let state = if runtime.dev_build || is_dev_build(&running_from) {
        State::DevBuild
    } else if let Some(path) = &installed_path {
        if canonical(path) == canonical(&running_from) {
            State::InstalledHere
        } else {
            State::InstalledElsewhere { version: receipt.as_ref().unwrap().version.clone() }
        }
    } else {
        State::NotInstalled
    };
    Ok(Detection { running_from, is_app_image: runtime.is_appimage(), receipt, installed_path, state })
}

/// Suppression is part of the shared contract. Persistent "don't ask again"
/// preferences remain app-owned (the caller passes `dismissed`).
pub fn should_prompt(detection: &Detection, args: &[String], no_prompt: bool, dismissed: bool) -> bool {
    detection.is_app_image
        && !matches!(detection.state, State::DevBuild | State::InstalledHere)
        && !no_prompt
        && env::var("ARCADE_NO_INSTALL_PROMPT").as_deref() != Ok("1")
        && !dismissed
        && !args.iter().any(|s| {
            ["--background", "--arcade-invoke", "--arcade-manifest", "--version", "-V", "--install", "--uninstall", "--repair", "--integration-status"]
                .contains(&s.split('=').next().unwrap_or(s))
        })
}

#[derive(Debug, Clone)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    /// Stable filename, e.g. Arcade-Find.AppImage.
    pub filename: String,
    /// Existing desktop identity, without .desktop.
    pub desktop_id: String,
    pub cli_name: String,
    pub background_args: Vec<String>,
    pub legacy_desktop_ids: Vec<String>,
    /// May differ from the launcher identity (e.g. Wheel).
    pub autostart_id: String,
}
impl AppInfo {
    #[cfg(target_os = "linux")]
    fn validate(&self) -> io::Result<()> {
        if !self.id.strip_prefix("arcade.").is_some_and(crate::shortcuts::identifier) || self.name.trim().is_empty() || self.name.contains(['\n', '\r']) {
            return Err(input("invalid app id or name"));
        }
        for s in [&self.filename, &self.desktop_id, &self.cli_name, &self.autostart_id].into_iter().chain(self.legacy_desktop_ids.iter()) {
            if s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\', '\n', '\r', '\0']) {
                return Err(input("integration names must be single path components"));
            }
        }
        if self.background_args.iter().any(|s| s.contains(['\n', '\r', '\0'])) {
            return Err(input("invalid background argument"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum IconSize {
    Pixels(u16),
    Scalable,
}
#[derive(Debug, Clone)]
pub struct Icon {
    pub source: PathBuf,
    pub size: IconSize,
}
pub struct InstallOptions<'a> {
    pub app: &'a AppInfo,
    pub runtime: &'a Runtime,
    pub version: &'a str,
    pub channel: &'a str,
    pub managed_by: ManagedBy,
    pub icons: &'a [Icon],
    pub start_at_login: bool,
    pub remove_download: bool,
    pub refresh_caches: bool,
}

#[cfg(target_os = "linux")]
fn quote(s: &str) -> String {
    // Desktop Entry string escaping, followed by Exec quoting. '%' is a
    // literal here, never a field code. Never invoke a shell.
    format!("\"{}\"", s.replace('\\', "\\\\\\\\").replace('"', "\\\\\"").replace('`', "\\\\`").replace('$', "\\\\$").replace('%', "%%"))
}
#[cfg(target_os = "linux")]
fn exec(path: &Path, args: &[String]) -> String {
    std::iter::once(quote(&path.to_string_lossy())).chain(args.iter().map(|s| quote(s))).collect::<Vec<_>>().join(" ")
}
#[cfg(target_os = "linux")]
fn desktop(app: &AppInfo, path: &Path) -> String {
    format!("# Generated by Arcade Link\n[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nTerminal=false\nCategories=Utility;\nX-Arcade-Id={}\nActions=Settings;Quit;Uninstall;\n\n[Desktop Action Settings]\nName=Open Settings\nExec={}\n\n[Desktop Action Quit]\nName=Quit\nExec={}\n\n[Desktop Action Uninstall]\nName=Uninstall\nExec={}\n", app.name, exec(path, &[]), app.desktop_id, app.id,
        exec(path, &["--settings".into()]), exec(path, &["--quit".into()]), exec(path, &["--uninstall".into()]))
}
#[cfg(target_os = "linux")]
fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    paths::write_atomic(path, contents, false)
}
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(unix)]
fn symlink(path: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| input("no CLI parent"))?)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.file_type().is_symlink() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "CLI path is an existing regular file"));
        }
    }
    let tmp = path.with_extension(format!("{}.link", crate::endpoint::new_token()?));
    std::os::unix::fs::symlink(target, &tmp)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(target_os = "linux")]
fn repair_autostart(path: &Path, app: &AppInfo, executable: &Path, create: bool) -> io::Result<bool> {
    let old = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound && !create => return Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => format!(
            "# Generated by Arcade Link\n[Desktop Entry]\nType=Application\nName={}\nExec=\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-Arcade-Id={}\n",
            app.name, app.id
        ),
        Err(e) => return Err(e),
    };
    let mut in_entry = false;
    let mut replaced = false;
    let command = format!("Exec={}", exec(executable, &app.background_args));
    let mut lines = Vec::new();
    for line in old.lines() {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        }
        if in_entry && line.starts_with("Exec=") {
            lines.push(command.clone());
            replaced = true;
        } else {
            lines.push(line.into());
        }
    }
    if !replaced {
        return Err(input("autostart entry has no main Exec key"));
    }
    write(path, (lines.join("\n") + "\n").as_bytes())?;
    Ok(true)
}

#[cfg(target_os = "linux")]
fn cleanup_duplicates(environment: &Environment, app: &AppInfo, main: &Path, executable: &Path) -> io::Result<()> {
    let dir = environment.data.join("applications");
    for entry in fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path == main || path.extension().is_none_or(|s| s != "desktop") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let owned = text.lines().any(|line| line == format!("X-Arcade-Id={}", app.id))
            || path.file_stem().is_some_and(|id| app.legacy_desktop_ids.iter().any(|s| id == s.as_str()));
        let wanted = exec(executable, &[]);
        let mut in_entry = false;
        let same = text
            .lines()
            .filter_map(|line| {
                if line.starts_with('[') {
                    in_entry = line == "[Desktop Entry]";
                }
                in_entry.then(|| line.strip_prefix("Exec=")).flatten()
            })
            .any(|s| {
                s == wanted
                    || s.starts_with(&(wanted.clone() + " "))
                    || s == executable.to_string_lossy()
                    || s.starts_with(&format!("{} ", executable.display()))
            });
        if owned && !same {
            remove_file(&path)?;
        }
    }
    Ok(())
}

// Integration files are small; snapshot them before replacing the executable.
// Binary rollback uses renames, so an update never copies the previous AppImage
// into memory. Nothing is persisted outside the explicit environment.
#[cfg(target_os = "linux")]
enum SavedEntry {
    Missing,
    File(Vec<u8>, fs::Permissions),
    Symlink(PathBuf),
}
#[cfg(target_os = "linux")]
struct InstallRollback {
    entries: Vec<(PathBuf, SavedEntry)>,
    dest: PathBuf,
    staged: PathBuf,
    replaced: Option<PathBuf>,
    saved_previous: Option<(PathBuf, PathBuf)>,
    new_binary: bool,
    committed: bool,
}
#[cfg(target_os = "linux")]
impl InstallRollback {
    fn save(&mut self, path: &Path) -> io::Result<()> {
        if self.entries.iter().any(|(p, _)| p == path) {
            return Ok(());
        }
        let saved = match fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => SavedEntry::Symlink(fs::read_link(path)?),
            Ok(m) if m.is_file() => SavedEntry::File(fs::read(path)?, m.permissions()),
            Ok(_) => return Err(input("integration entry must be a file or symlink")),
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => SavedEntry::Missing,
            Err(e) => return Err(e),
        };
        self.entries.push((path.to_path_buf(), saved));
        Ok(())
    }
}
#[cfg(target_os = "linux")]
impl Drop for InstallRollback {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.staged);
        if self.committed {
            if let Some((backup, _)) = &self.saved_previous {
                let _ = fs::remove_file(backup);
            }
            return;
        }
        if self.new_binary {
            let _ = fs::remove_file(&self.dest);
        }
        if let Some(previous) = &self.replaced {
            let _ = fs::rename(previous, &self.dest);
        }
        if let Some((backup, previous)) = &self.saved_previous {
            let _ = fs::rename(backup, previous);
        }
        for (path, saved) in self.entries.iter().rev() {
            match saved {
                SavedEntry::Missing => {
                    let _ = fs::remove_file(path);
                }
                SavedEntry::File(bytes, permissions) => {
                    if write(path, bytes).is_ok() {
                        let _ = fs::set_permissions(path, permissions.clone());
                    }
                }
                SavedEntry::Symlink(target) => {
                    let _ = symlink(path, target);
                }
            }
        }
    }
}

/// Linux self-install. Explicit invocation may install a plain fixture file;
/// first-run UI must additionally use `Detection::is_app_image`.
#[cfg(target_os = "linux")]
pub fn install(environment: &Environment, opts: InstallOptions<'_>) -> io::Result<Receipt> {
    opts.app.validate()?;
    let detected = detect(environment, &opts.app.id, opts.runtime)?;
    if detected.state == State::DevBuild {
        return Err(input("dev builds cannot be installed"));
    }
    if opts.version.trim().is_empty() || opts.channel.trim().is_empty() {
        return Err(input("version and channel are required"));
    }
    let source = detected.running_from;
    if !source.is_absolute() || !source.is_file() {
        return Err(input("source must be an absolute file"));
    }
    let dest = environment.applications.join(&opts.app.filename);
    fs::create_dir_all(&environment.applications)?;
    let old = detected.receipt;
    let same = canonical(&source) == canonical(&dest);
    let mut previous = old.as_ref().and_then(|r| r.previous.clone());
    let mut integration = Integration {
        desktop_entry: Some(environment.data.join("applications").join(format!("{}.desktop", opts.app.desktop_id))),
        cli: Some(environment.bin.join(&opts.app.cli_name)),
        ..Integration::default()
    };
    let mut prepared_icons = Vec::new();
    for icon in opts.icons {
        let size = match icon.size {
            IconSize::Pixels(n) if (16..=512).contains(&n) => format!("{n}x{n}"),
            IconSize::Scalable => "scalable".into(),
            _ => return Err(input("icon size must be 16–512 or scalable")),
        };
        let ext = match icon.size {
            IconSize::Pixels(_) => "png",
            IconSize::Scalable => "svg",
        };
        let target = environment.data.join("icons/hicolor").join(size).join("apps").join(format!("{}.{ext}", opts.app.desktop_id));
        let bytes = fs::read(&icon.source)?;
        prepared_icons.push((target.clone(), bytes));
        integration.icons.push(target);
    }
    if let Some(old) = &old {
        for icon in &old.integration.icons {
            if !integration.icons.contains(icon) {
                integration.icons.push(icon.clone());
            }
        }
    }
    let cli = integration.cli.as_ref().unwrap();
    if fs::symlink_metadata(cli).is_ok_and(|m| !m.file_type().is_symlink()) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "CLI path is an existing regular file"));
    }
    let autostart = environment.config.join("autostart").join(format!("{}.desktop", opts.app.autostart_id));
    let staged = dest.with_extension(format!("{}.stage", crate::endpoint::new_token()?));
    let mut rollback = InstallRollback {
        entries: Vec::new(),
        dest: dest.clone(),
        staged: staged.clone(),
        replaced: None,
        saved_previous: None,
        new_binary: false,
        committed: false,
    };
    for path in prepared_icons.iter().map(|(p, _)| p).chain(integration.desktop_entry.iter()).chain(integration.cli.iter()) {
        rollback.save(path)?;
    }
    if autostart.exists() || opts.start_at_login {
        rollback.save(&autostart)?;
    }
    // Duplicate cleanup is reversible too, including legacy launchers.
    if let Ok(entries) = fs::read_dir(environment.data.join("applications")) {
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "desktop") {
                let text = fs::read_to_string(&path)?;
                let owned = text.lines().any(|line| line == format!("X-Arcade-Id={}", opts.app.id))
                    || path.file_stem().is_some_and(|id| opts.app.legacy_desktop_ids.iter().any(|s| id == s.as_str()));
                if owned {
                    rollback.save(&path)?;
                }
            }
        }
    }
    if !same {
        if dest.exists() && old.as_ref().is_none_or(|r| r.path != dest) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination exists without this app's receipt"));
        }
        use std::io::Write;
        let mut stage = fs::OpenOptions::new().write(true).create_new(true).open(&staged)?;
        io::copy(&mut fs::File::open(&source)?, &mut stage)?;
        stage.flush()?;
        stage.sync_all()?;
        drop(stage);
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        if dest.exists() {
            let prev = environment.applications.join(".previous").join(&opts.app.filename);
            fs::create_dir_all(prev.parent().unwrap())?;
            if prev.exists() {
                let backup = prev.with_extension(format!("{}.backup", crate::endpoint::new_token()?));
                fs::rename(&prev, &backup)?;
                rollback.saved_previous = Some((backup, prev.clone()));
            }
            fs::rename(&dest, &prev)?;
            rollback.replaced = Some(prev.clone());
            previous = Some(Previous { version: old.as_ref().unwrap().version.clone(), path: prev });
        }
        fs::rename(&staged, &dest)?;
        rollback.new_binary = true;
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
    for (path, bytes) in prepared_icons {
        write(&path, &bytes)?;
    }
    write(integration.desktop_entry.as_ref().unwrap(), desktop(opts.app, &dest).as_bytes())?;
    symlink(integration.cli.as_ref().unwrap(), &dest)?;
    if repair_autostart(&autostart, opts.app, &dest, opts.start_at_login)? {
        integration.autostart = Some(autostart);
    }
    cleanup_duplicates(environment, opts.app, integration.desktop_entry.as_ref().unwrap(), &dest)?;
    let now = manifest::now_rfc3339();
    let receipt = Receipt {
        schema: 1,
        id: opts.app.id.clone(),
        version: opts.version.into(),
        channel: opts.channel.into(),
        method: InstallMethod::Appimage,
        managed_by: opts.managed_by,
        path: dest,
        integration,
        previous,
        installed_at: old.as_ref().map_or_else(|| now.clone(), |r| r.installed_at.clone()),
        updated_at: now,
    };
    environment.receipts().write(&receipt)?;
    rollback.committed = true;
    if opts.remove_download && !same {
        remove_file(&source)?;
    }
    if opts.refresh_caches {
        refresh_caches(environment);
    }
    Ok(receipt)
}

/// Rebuild integration using the receipt's paths. The app supplies its stable
/// desktop identity and display metadata; no path is guessed. `moved_to` is
/// explicit consent to repoint a moved installation.
#[cfg(target_os = "linux")]
pub fn repair(environment: &Environment, app: &AppInfo, moved_to: Option<&Path>) -> io::Result<Receipt> {
    app.validate()?;
    let mut receipt = environment.receipts().read(&app.id)?.ok_or_else(|| input("no receipt"))?;
    if let Some(path) = moved_to {
        if !path.is_absolute() || !path.is_file() {
            return Err(input("moved installation must be an absolute file"));
        }
        receipt.path = path.into();
    }
    if !receipt.path.is_file() {
        return Err(input("installed executable is missing"));
    }
    if let Some(path) = &receipt.integration.desktop_entry {
        write(path, desktop(app, &receipt.path).as_bytes())?;
        cleanup_duplicates(environment, app, path, &receipt.path)?;
    }
    if let Some(path) = &receipt.integration.cli {
        symlink(path, &receipt.path)?;
    }
    if let Some(path) = &receipt.integration.autostart {
        repair_autostart(path, app, &receipt.path, true)?;
    }
    receipt.updated_at = manifest::now_rfc3339();
    environment.receipts().write(&receipt)?;
    Ok(receipt)
}

#[cfg(target_os = "linux")]
fn refresh_caches(environment: &Environment) {
    let _ = Command::new("update-desktop-database").arg(environment.data.join("applications")).output();
    let _ = Command::new("gtk-update-icon-cache").arg("-t").arg(environment.data.join("icons/hicolor")).output();
}

pub struct UninstallOptions<'a> {
    pub remove_data: bool,
    pub data_folders: &'a [PathBuf],
}
/// Removes only receipt-listed integration and explicitly supplied data.
/// Does not invoke a Windows uninstaller: apps/Tools run its recorded command.
pub fn uninstall(environment: &Environment, id: &str, opts: UninstallOptions<'_>) -> io::Result<bool> {
    let Some(receipt) = environment.receipts().read(id)? else { return Ok(false) };
    for path in std::iter::once(&receipt.path).chain(receipt.previous.iter().map(|p| &p.path)) {
        if path.is_dir() && (receipt.method != crate::receipt::InstallMethod::MacosBundle || path.extension().is_none_or(|e| e != "app")) {
            return Err(input("refusing a directory as an installed executable"));
        }
    }
    if opts.remove_data {
        for p in opts.data_folders {
            if !p.is_absolute()
                || p.parent().is_none()
                || [
                    environment.home.as_path(),
                    environment.data.as_path(),
                    environment.config.as_path(),
                    environment.applications.as_path(),
                    environment.bin.as_path(),
                ]
                .iter()
                .any(|root| canonical(root).starts_with(canonical(p)))
            {
                return Err(input("refusing a root directory as app data"));
            }
        }
    }
    for p in receipt.integration.icons.iter().chain(receipt.integration.desktop_entry.iter()).chain(receipt.integration.autostart.iter()) {
        remove_file(p)?;
    }
    if let Some(cli) = &receipt.integration.cli {
        if fs::symlink_metadata(cli).is_ok_and(|m| m.file_type().is_symlink()) {
            if fs::read_link(cli)? == receipt.path {
                remove_file(cli)?;
            }
        } else if cfg!(windows) {
            remove_file(cli)?;
        }
    }
    remove_installation(&receipt.path)?;
    if let Some(previous) = &receipt.previous {
        remove_installation(&previous.path)?;
    }
    if opts.remove_data {
        for p in opts.data_folders {
            remove_installation(p)?;
        }
    }
    manifest::remove_manifest(&environment.locations, id)?;
    environment.receipts().remove(id)?;
    #[cfg(windows)]
    platform::windows::remove_path_if_last(environment, &environment.bin)?;
    Ok(true)
}
fn remove_installation(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => fs::remove_dir_all(path),
        Ok(_) => remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryStatus {
    pub path: PathBuf,
    pub exists: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub receipt: Option<Receipt>,
    pub entries: Vec<EntryStatus>,
    pub bin_on_path: bool,
    pub path_hint: Option<String>,
}
pub fn path_hint(bin: &Path, shell: &str) -> String {
    let quoted = format!("'{}'", bin.to_string_lossy().replace('\'', "'\\''"));
    if shell.rsplit('/').next() == Some("fish") {
        format!("fish_add_path {quoted}")
    } else {
        format!("export PATH={quoted}:\"$PATH\"")
    }
}
/// Read-only. Reports missing entries (including previous), never repairs them.
pub fn integration_status(environment: &Environment, id: &str, shell: &str) -> io::Result<IntegrationStatus> {
    let receipt = environment.receipts().read(id)?;
    let mut entries = Vec::new();
    if let Some(r) = &receipt {
        for p in std::iter::once(&r.path)
            .chain(r.integration.desktop_entry.iter())
            .chain(r.integration.icons.iter())
            .chain(r.integration.cli.iter())
            .chain(r.integration.autostart.iter())
            .chain(r.integration.uninstaller.iter())
            .chain(r.previous.iter().map(|p| &p.path))
        {
            entries.push(EntryStatus { path: p.clone(), exists: p.exists() });
        }
    }
    let bin_on_path = environment.path.as_ref().is_some_and(|p| env::split_paths(p).any(|p| canonical(&p) == canonical(&environment.bin)));
    Ok(IntegrationStatus { receipt, entries, bin_on_path, path_hint: (!bin_on_path).then(|| path_hint(&environment.bin, shell)) })
}

/// Replaces this process on Unix, or starts a successor on Windows. The
/// caller passes original args and owns any old-process wait handshake.
pub fn reexec(path: &Path, original_args: &[std::ffi::OsString]) -> io::Result<()> {
    let mut cmd = Command::new(path);
    cmd.args(original_args).env_remove("APPIMAGE").env_remove("APPDIR");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(cmd.exec())
    }
    #[cfg(not(unix))]
    {
        cmd.spawn().map(|_| ())
    }
}
