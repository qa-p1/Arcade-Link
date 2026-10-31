//! Explicit-path integration, called on a worker thread after user consent.
//! `Environment::under` isolates *every* filesystem write. No profile is edited.
//! App/toolkit code owns dialogs, consent, restart waiting and launch policy.

use crate::{
    manifest,
    paths::Locations,
    receipt::{ManagedBy, Receipt, SkippedEntry, Store},
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
            windows_environment_key: isolated_environment_key(root),
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
    pub fn validate(&self) -> io::Result<()> {
        for path in
            [&self.home, &self.data, &self.config, &self.applications, &self.bin, &self.local_app_data, &self.locations.registry, &self.locations.runtime]
        {
            crate::receipt::native_absolute(path)?;
        }
        Ok(())
    }
}

/// Shared with Qt and the installer helper. This is a Windows test-key
/// identity, not a filesystem path normalization or an ownership check.
pub fn isolated_environment_key(root: &Path) -> String {
    let spelling = root.to_string_lossy().replace('\\', "/");
    let spelling = spelling.trim_end_matches('/').to_ascii_lowercase();
    format!("Software\\ArcadeLink\\Isolated\\{}", crate::transport::short_hash(&spelling))
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
    let installed_path = receipt.as_ref().filter(|r| r.path.is_absolute() && r.path.exists()).map(|r| r.path.clone());
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
    (detection.is_app_image || (cfg!(target_os = "macos") && detection.running_from.extension().is_some_and(|extension| extension == "app")))
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
    // Exec quoting, then Desktop Entry string escaping. '%' is a literal
    // here, never a field code. Control characters use the string escapes
    // (a raw newline would end the key). Never invoke a shell.
    let exec = s.replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$").replace('%', "%%");
    format!("\"{}\"", exec.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r"))
}
#[cfg(target_os = "linux")]
fn exec(path: &Path, args: &[String]) -> String {
    let program = path.to_string_lossy();
    // GLib looks the program up before expanding "%%", so a '%' in the path
    // (e.g. in HOME) makes every launcher "unable to load". `env` takes the
    // program as an ordinary argument, where "%%" is expanded.
    let env = program.contains('%').then(|| "env".to_string());
    env.into_iter().chain(std::iter::once(quote(&program))).chain(args.iter().map(|s| quote(s))).collect::<Vec<_>>().join(" ")
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
    replace_symlink(path, target)
}

// Rollback owns the replacement at this path, including a regular launcher.
// A staged symlink plus rename restores the saved link atomically.
#[cfg(unix)]
fn replace_symlink(path: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| input("no symlink parent"))?)?;
    let tmp = path.with_extension(format!("{}.link", crate::endpoint::new_token()?));
    std::os::unix::fs::symlink(target, &tmp)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(target_os = "linux")]
fn repair_autostart(path: &Path, app: &AppInfo, executable: &Path, create: bool, enable: bool) -> io::Result<bool> {
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
    // A login entry taken over from an older copy becomes recognizably ours.
    let marked = entry_value(&old, "X-Arcade-Id").is_some();
    let mut lines = Vec::new();
    for line in old.lines() {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        }
        // An explicit "Start at login" re-enables an entry the desktop's
        // startup settings switched off; a repair leaves that choice alone.
        if enable && in_entry && (line.trim() == "Hidden=true" || line.trim() == "X-GNOME-Autostart-enabled=false") {
            if line.trim() != "Hidden=true" {
                lines.push("X-GNOME-Autostart-enabled=true".into());
            }
            continue;
        }
        if in_entry && line.starts_with("Exec=") {
            lines.push(command.clone());
            if !marked {
                lines.push(format!("X-Arcade-Id={}", app.id));
            }
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

/// Whether a login entry will actually start the app: present and not
/// switched off with `Hidden=true` or `X-GNOME-Autostart-enabled=false`.
#[cfg(target_os = "linux")]
pub fn autostart_enabled(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else { return false };
    entry_value(&text, "Hidden").is_none_or(|v| v.trim() != "true") && entry_value(&text, "X-GNOME-Autostart-enabled").is_none_or(|v| v.trim() != "false")
}

#[cfg(target_os = "linux")]
fn entry_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut in_entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        }
        if in_entry {
            if let Some((name, value)) = line.split_once('=') {
                if name.trim() == key {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// Undo Desktop Entry string escaping before Exec argument quoting. This
/// reads only the executable token: arguments/field codes are never executed.
#[cfg(target_os = "linux")]
fn desktop_executable(text: &str) -> Option<PathBuf> {
    let value = entry_value(text, "Exec")?;
    let mut decoded = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        decoded.push(if c == '\\' {
            match chars.next()? {
                's' => ' ',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                _ => return None,
            }
        } else {
            c
        });
    }
    let mut rest = decoded.as_str();
    let mut token = exec_token(&mut rest)?;
    // Launchers for paths containing '%' run the program through `env`.
    if token == "env" {
        token = exec_token(&mut rest)?;
    }
    (!token.is_empty() && !token.contains(['=', '\0', '\n', '\r'])).then(|| PathBuf::from(token))
}

/// Reads one Exec argument from the front of `rest` (already string-unescaped).
#[cfg(target_os = "linux")]
fn exec_token(rest: &mut &str) -> Option<String> {
    let text = rest.trim_start();
    let mut chars = text.char_indices().peekable();
    let quoted = chars.peek().map(|(_, c)| *c) == Some('"');
    if quoted {
        chars.next();
    }
    let mut token = String::new();
    let mut closed = !quoted;
    let mut end = text.len();
    while let Some((i, c)) = chars.next() {
        if quoted && c == '"' {
            closed = true;
            if chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
                return None;
            }
            end = i + 1;
            break;
        }
        if !quoted && c.is_whitespace() {
            end = i;
            break;
        }
        token.push(match c {
            '\\' if quoted => match chars.next()?.1 {
                c @ ('"' | '`' | '$' | '\\') => c,
                _ => return None,
            },
            '%' => {
                if chars.next()?.1 != '%' {
                    return None;
                }
                '%'
            }
            '"' | '\\' if !quoted => return None,
            c => c,
        });
    }
    if !closed {
        return None;
    }
    *rest = &text[end..];
    Some(token)
}

#[cfg(target_os = "linux")]
fn desktop_owned(path: &Path, app: &AppInfo, recorded: bool) -> bool {
    let Ok(text) = fs::read_to_string(path) else { return false };
    let marker = entry_value(&text, "X-Arcade-Id");
    if marker.is_some_and(|id| id != app.id) {
        return false;
    }
    if recorded {
        return true;
    }
    let Some(executable) = desktop_executable(&text) else { return false };
    if entry_value(&text, "Type") != Some("Application") || !executable.is_absolute() {
        return false;
    }
    if marker == Some(&app.id) {
        return true;
    }
    let known_id = path.file_stem().is_some_and(|id| {
        id == app.desktop_id.as_str() || id == app.autostart_id.as_str() || app.legacy_desktop_ids.iter().any(|legacy| id == legacy.as_str())
    });
    known_id && executable.file_name().is_some_and(|name| name == app.filename.as_str() || name == app.cli_name.as_str() || legacy_appimage_name(app, name))
}

/// Releases before the self-installer ran from wherever they were saved and
/// wrote their own launcher, e.g. `ArcadeLens-0.1.1-linux-x86_64.AppImage`.
/// Together with the app's own desktop id that identifies the app's older copy.
#[cfg(target_os = "linux")]
fn legacy_appimage_name(app: &AppInfo, name: &std::ffi::OsStr) -> bool {
    let normalize = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect::<String>();
    let name = name.to_string_lossy();
    if name == app.filename.as_str() || name == app.cli_name.as_str() {
        return false;
    }
    let app_name = normalize(&app.name);
    !app_name.is_empty() && name.to_ascii_lowercase().ends_with(".appimage") && normalize(&name).starts_with(&app_name)
}

/// Older copies of this app that its launcher or login entry still run,
/// i.e. what an install replaces. Apps can mention them in the prompt.
#[cfg(target_os = "linux")]
pub fn legacy_copies(environment: &Environment, app: &AppInfo) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::iter::once(app.desktop_id.as_str())
        .chain(app.legacy_desktop_ids.iter().map(String::as_str))
        .map(|id| environment.data.join("applications").join(format!("{id}.desktop")))
        .collect();
    entries.push(environment.config.join("autostart").join(format!("{}.desktop", app.autostart_id)));
    let mut copies = Vec::new();
    for entry in entries {
        let Ok(text) = fs::read_to_string(&entry) else { continue };
        let Some(executable) = desktop_executable(&text) else { continue };
        let legacy = executable.file_name().is_some_and(|name| legacy_appimage_name(app, name));
        if legacy && entry_value(&text, "X-Arcade-Id").is_none() && executable.is_file() && !copies.contains(&executable) {
            copies.push(executable);
        }
    }
    copies
}

/// Asks processes of this user started from the AppImage `path` to quit, so
/// an older copy doesn't keep running beside its replacement. Matching the
/// app's `APPIMAGE`, not the runtime's executable, lets the runtime unmount
/// after the app exits instead of being killed under it.
#[cfg(target_os = "linux")]
fn stop_processes_running(path: &Path) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    let target = canonical(path);
    let me = std::process::id();
    let Ok(dir) = fs::read_dir("/proc") else { return };
    for entry in dir.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        if pid == me {
            continue;
        }
        let appimage = fs::read(entry.path().join("environ")).ok().and_then(|bytes| {
            bytes.split(|b| *b == 0).find_map(|var| var.strip_prefix(b"APPIMAGE=").map(|v| PathBuf::from(String::from_utf8_lossy(v).into_owned())))
        });
        if appimage.is_some_and(|p| canonical(&p) == target) {
            // SAFETY: kill has no memory preconditions; only our own
            // processes' environ is readable.
            unsafe {
                kill(pid as i32, SIGTERM);
            }
        }
    }
}

/// `$XDG_DOWNLOAD_DIR` from user-dirs.dirs, else `~/Downloads`.
#[cfg(target_os = "linux")]
fn download_dir(environment: &Environment) -> PathBuf {
    let configured = fs::read_to_string(environment.config.join("user-dirs.dirs")).ok().and_then(|text| {
        text.lines().find_map(|line| {
            let value = line.trim().strip_prefix("XDG_DOWNLOAD_DIR=")?.trim().trim_matches('"');
            let path = match value.strip_prefix("$HOME") {
                Some(rest) => environment.home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(value),
            };
            path.is_absolute().then_some(path)
        })
    });
    configured.unwrap_or_else(|| environment.home.join("Downloads"))
}

/// "Remove the downloaded file" only ever removes a file from the Downloads
/// folder: an AppImage the user keeps elsewhere is theirs.
#[cfg(target_os = "linux")]
pub fn is_download(environment: &Environment, source: &Path) -> bool {
    canonical(source).starts_with(canonical(&download_dir(environment)))
}

/// Error text that names the path, e.g. a read-only apps folder.
fn at<'a>(path: &'a Path, action: &'static str) -> impl FnOnce(io::Error) -> io::Error + 'a {
    move |e| io::Error::new(e.kind(), format!("{action} {}: {e}", path.display()))
}

/// Held for the whole of an install, repair or uninstall of one app, across
/// processes: Rust and Qt lock the same `<installs>/<id>.lock`. A second
/// installer waits for the first instead of interleaving with it.
pub struct AppLock {
    _file: fs::File,
}

#[cfg(unix)]
fn try_lock(file: &fs::File) -> io::Result<bool> {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    // SAFETY: the descriptor is owned by `file` and open for the call.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(e)
    }
}
#[cfg(windows)]
fn try_lock(file: &fs::File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY},
        System::IO::OVERLAPPED,
    };
    // SAFETY: zeroed OVERLAPPED is the documented start-of-file offset.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is owned by `file`; `overlapped` outlives the call.
    let ok = unsafe { LockFileEx(file.as_raw_handle() as _, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY, 0, u32::MAX, u32::MAX, &mut overlapped) };
    if ok != 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(33) {
        Ok(false) // ERROR_LOCK_VIOLATION
    } else {
        Err(e)
    }
}

/// Waits up to two minutes for another install, update or uninstall of `id`.
pub fn lock(environment: &Environment, id: &str) -> io::Result<AppLock> {
    let store = environment.receipts();
    let path = store.path(id)?.with_extension("lock");
    crate::paths::ensure_private_dir(store.dir())?;
    let file = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&path).map_err(at(&path, "Couldn't open"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !try_lock(&file)? {
        if std::time::Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "another install, update or uninstall of this app is still running"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(AppLock { _file: file })
}

/// `<stem>.<token>.stage` copies left behind by an installer that was killed.
/// Called with the app's lock held, so no other installer owns them.
#[cfg(target_os = "linux")]
fn remove_stale_stages(applications: &Path, filename: &str) {
    let stem = Path::new(filename).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Ok(entries) = fs::read_dir(applications) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let token = name.strip_prefix(&format!("{stem}.")).and_then(|rest| rest.strip_suffix(".stage"));
        if token.is_some_and(|t| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit())) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(target_os = "linux")]
fn available_entry(path: &Path, owned: bool) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(owned && (meta.is_file() || meta.file_type().is_symlink())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "linux")]
fn skipped(integration: &mut Integration, path: PathBuf) {
    if !integration.skipped.iter().any(|entry| entry.path == path) {
        integration.skipped.push(SkippedEntry { path, reason: "existing entry belongs to another installation".into() });
    }
}

#[cfg(target_os = "linux")]
fn recorded_desktop_owned(path: &Path, id: &str, executable: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else { return false };
    match entry_value(&text, "X-Arcade-Id") {
        Some(marker) => marker == id,
        None => {
            entry_value(&text, "Type") == Some("Application")
                && desktop_executable(&text).is_some_and(|p| p.is_absolute() && canonical(&p) == canonical(executable))
        }
    }
}

#[cfg(target_os = "linux")]
fn integration_conflicts(receipt: &Receipt) -> io::Result<Vec<PathBuf>> {
    let mut conflicts = Vec::new();
    for path in receipt.integration.desktop_entry.iter().chain(receipt.integration.autostart.iter()) {
        if !available_entry(path, recorded_desktop_owned(path, &receipt.id, &receipt.path))? {
            conflicts.push(path.clone());
        }
    }
    if let Some(path) = &receipt.integration.cli {
        let owned = fs::read_link(path).is_ok_and(|target| target == receipt.path);
        if !available_entry(path, owned)? {
            conflicts.push(path.clone());
        }
    }
    Ok(conflicts)
}

#[cfg(target_os = "linux")]
fn within_root(path: &Path, root: &Path) -> io::Result<()> {
    crate::receipt::native_absolute(path)?;
    // Resolve the nearest existing parent, including symlinked directories.
    let parent = path.parent().ok_or_else(|| input("entry has no parent"))?;
    let existing = parent.ancestors().find(|p| p.exists()).ok_or_else(|| input("entry has no existing ancestor"))?;
    let expected = root.ancestors().find(|p| p.exists()).ok_or_else(|| input("root has no existing ancestor"))?;
    let resolved_parent = canonical(existing).join(parent.strip_prefix(existing).map_err(|_| input("invalid entry root"))?);
    let resolved_root = canonical(expected).join(root.strip_prefix(expected).map_err(|_| input("invalid configured root"))?);
    if !path.starts_with(root) || !resolved_parent.starts_with(resolved_root) {
        return Err(input("integration path escapes its configured root"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_integration_roots(environment: &Environment, receipt: &Receipt) -> io::Result<()> {
    for p in &receipt.integration.icons {
        within_root(p, &environment.data.join("icons"))?;
    }
    if let Some(p) = &receipt.integration.desktop_entry {
        within_root(p, &environment.data.join("applications"))?;
    }
    if let Some(p) = &receipt.integration.autostart {
        within_root(p, &environment.config.join("autostart"))?;
    }
    if let Some(p) = &receipt.integration.cli {
        within_root(p, &environment.bin)?;
    }
    if let Some(p) = &receipt.previous {
        within_root(&p.path, &environment.applications.join(".previous"))?;
    }
    Ok(())
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
        let owned = desktop_owned(&path, app, false);
        let Some(entry_executable) = desktop_executable(&text) else { continue };
        let same = canonical(&entry_executable) == canonical(executable);
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
                    let _ = replace_symlink(path, target);
                }
            }
        }
    }
}

/// Linux self-install. Explicit invocation may install a plain fixture file;
/// first-run UI must additionally use `Detection::is_app_image`.
#[cfg(target_os = "linux")]
pub fn install(environment: &Environment, opts: InstallOptions<'_>) -> io::Result<Receipt> {
    environment.validate()?;
    opts.app.validate()?;
    if detect(environment, &opts.app.id, opts.runtime)?.state == State::DevBuild {
        return Err(input("dev builds cannot be installed"));
    }
    if opts.version.trim().is_empty() || opts.channel.trim().is_empty() {
        return Err(input("version and channel are required"));
    }
    // Refusals above leave no trace; everything below runs under the lock,
    // starting from a fresh read of the receipt.
    let _lock = lock(environment, &opts.app.id)?;
    let detected = detect(environment, &opts.app.id, opts.runtime)?;
    let source = detected.running_from;
    if !source.is_absolute() || !source.is_file() {
        return Err(input("source must be an absolute file"));
    }
    let dest = environment.applications.join(&opts.app.filename);
    fs::create_dir_all(&environment.applications).map_err(at(&environment.applications, "Couldn't create"))?;
    remove_stale_stages(&environment.applications, &opts.app.filename);
    let legacy = legacy_copies(environment, opts.app);
    let old = detected.receipt;
    if let Some(receipt) = &old {
        receipt.validate_native_paths()?;
        validate_integration_roots(environment, receipt)?;
    }
    let same = canonical(&source) == canonical(&dest);
    let mut previous = old.as_ref().and_then(|r| r.previous.clone());
    let mut integration = Integration::default();
    let desktop_entry = environment.data.join("applications").join(format!("{}.desktop", opts.app.desktop_id));
    within_root(&desktop_entry, &environment.data.join("applications"))?;
    let recorded = old.as_ref().is_some_and(|r| r.integration.desktop_entry.as_ref() == Some(&desktop_entry));
    let owned = if recorded {
        old.as_ref().is_some_and(|r| recorded_desktop_owned(&desktop_entry, &r.id, &r.path))
    } else {
        desktop_owned(&desktop_entry, opts.app, false)
    };
    if available_entry(&desktop_entry, owned)? {
        integration.desktop_entry = Some(desktop_entry.clone());
    } else {
        skipped(&mut integration, desktop_entry.clone());
    }
    let cli = environment.bin.join(&opts.app.cli_name);
    within_root(&cli, &environment.bin)?;
    let cli_owned = fs::read_link(&cli).is_ok_and(|p| p == dest || old.as_ref().is_some_and(|r| p == r.path));
    if available_entry(&cli, cli_owned)? {
        integration.cli = Some(cli);
    } else {
        skipped(&mut integration, cli);
    }
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
        within_root(&target, &environment.data.join("icons"))?;
        if !available_entry(&target, old.as_ref().is_some_and(|r| r.integration.icons.contains(&target)))? {
            skipped(&mut integration, target);
            continue;
        }
        let bytes = fs::read(&icon.source)?;
        prepared_icons.push((target.clone(), bytes));
        integration.icons.push(target);
    }
    if let Some(old) = &old {
        for icon in &old.integration.icons {
            if !integration.icons.contains(icon) && !integration.skipped.iter().any(|s| &s.path == icon) {
                integration.icons.push(icon.clone());
            }
        }
    }
    let autostart = environment.config.join("autostart").join(format!("{}.desktop", opts.app.autostart_id));
    within_root(&autostart, &environment.config.join("autostart"))?;
    // A launcher or login entry the old receipt recorded under another desktop
    // id (the id changed, e.g. `arcade-find` → `dev.arcade.Find`) is replaced by
    // the new one rather than left behind as a second menu entry or login item.
    let renamed: Vec<PathBuf> = old
        .iter()
        .flat_map(|r| {
            [r.integration.desktop_entry.as_ref(), r.integration.autostart.as_ref()]
                .into_iter()
                .flatten()
                .filter(|p| **p != desktop_entry && **p != autostart && recorded_desktop_owned(p, &r.id, &r.path))
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    // Start at login carries over to the new id, with the entry's own choices
    // (such as the desktop's switched-off flag) intact.
    let carried_login = old.as_ref().and_then(|r| r.integration.autostart.clone()).filter(|p| renamed.contains(p));
    if fs::symlink_metadata(&autostart).is_ok() || opts.start_at_login || carried_login.is_some() {
        let recorded = old.as_ref().is_some_and(|r| r.integration.autostart.as_ref() == Some(&autostart));
        let owned = if recorded {
            old.as_ref().is_some_and(|r| recorded_desktop_owned(&autostart, &r.id, &r.path))
        } else {
            desktop_owned(&autostart, opts.app, false)
        };
        if available_entry(&autostart, owned)? {
            integration.autostart = Some(autostart.clone());
        } else {
            skipped(&mut integration, autostart.clone());
        }
    }
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
    if integration.autostart.is_some() {
        rollback.save(&autostart)?;
    }
    for path in &renamed {
        rollback.save(path)?;
    }
    // Duplicate cleanup is reversible too, including legacy launchers.
    if let Ok(entries) = fs::read_dir(environment.data.join("applications")) {
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "desktop") {
                let owned = desktop_owned(&path, opts.app, false);
                if owned {
                    rollback.save(&path)?;
                }
            }
        }
    }
    // symlink_metadata, not exists(): a dangling symlink is still somebody's entry.
    let dest_present = fs::symlink_metadata(&dest).is_ok();
    if !same {
        if dest_present && old.as_ref().is_none_or(|r| r.path != dest) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination exists without this app's receipt"));
        }
        use std::io::Write;
        let mut stage = fs::OpenOptions::new().write(true).create_new(true).open(&staged).map_err(at(&environment.applications, "Couldn't write to"))?;
        io::copy(&mut fs::File::open(&source).map_err(at(&source, "Couldn't read"))?, &mut stage).map_err(at(&staged, "Couldn't copy to"))?;
        stage.flush()?;
        stage.sync_all().map_err(at(&staged, "Couldn't save"))?;
        drop(stage);
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        if dest_present {
            let preferred = environment.applications.join(".previous").join(&opts.app.filename);
            let owned_previous = old.as_ref().and_then(|r| r.previous.as_ref()).map(|p| &p.path);
            let prev = if let Some(owned) = owned_previous {
                owned.clone()
            } else if fs::symlink_metadata(&preferred).is_ok() {
                preferred.with_file_name(format!("{}-{}", crate::endpoint::new_token()?, opts.app.filename))
            } else {
                preferred
            };
            fs::create_dir_all(prev.parent().unwrap())?;
            if fs::symlink_metadata(&prev).is_ok() {
                if owned_previous != Some(&prev) {
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, "previous destination is not receipt-owned"));
                }
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
    if let Some(path) = &integration.desktop_entry {
        write(path, desktop(opts.app, &dest).as_bytes())?;
    }
    if let Some(path) = &integration.cli {
        symlink(path, &dest)?;
    }
    if let Some(path) = &integration.autostart {
        if let Some(previous) = carried_login.as_ref().filter(|_| fs::symlink_metadata(path).is_err()) {
            write(path, &fs::read(previous)?)?;
        }
        repair_autostart(path, opts.app, &dest, opts.start_at_login || carried_login.is_some(), opts.start_at_login)?;
    }
    if integration.desktop_entry.is_some() {
        cleanup_duplicates(environment, opts.app, &desktop_entry, &dest)?;
    }
    let replaced_by = |path: &PathBuf| {
        let autostart_entry = old.as_ref().is_some_and(|r| r.integration.autostart.as_ref() == Some(path));
        if autostart_entry {
            integration.autostart.is_some()
        } else {
            integration.desktop_entry.is_some()
        }
    };
    for path in renamed.iter().filter(|p| replaced_by(p)) {
        remove_file(path)?;
    }
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
    drop(rollback);
    // The install has happened: from here nothing may turn it into a failure.
    if opts.remove_download && !same && is_download(environment, &source) {
        let _ = remove_file(&source);
    }
    for copy in legacy.iter().filter(|copy| canonical(copy) != canonical(&receipt.path) && canonical(copy) != canonical(&source)) {
        stop_processes_running(copy);
        if opts.remove_download && is_download(environment, copy) {
            let _ = remove_file(copy);
        }
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
    environment.validate()?;
    app.validate()?;
    environment.receipts().read(&app.id)?.ok_or_else(|| input("no receipt"))?;
    let _lock = lock(environment, &app.id)?;
    let mut receipt = environment.receipts().read(&app.id)?.ok_or_else(|| input("no receipt"))?;
    receipt.validate_native_paths()?;
    validate_integration_roots(environment, &receipt)?;
    let conflicts = integration_conflicts(&receipt)?;
    if let Some(path) = moved_to {
        crate::receipt::native_absolute(path)?;
        if !path.is_file() {
            return Err(input("moved installation must be an absolute file"));
        }
        receipt.path = fs::canonicalize(path)?;
    }
    receipt.validate_native_paths()?;
    if !receipt.path.is_file() {
        return Err(input("installed executable is missing"));
    }
    for path in conflicts {
        if receipt.integration.desktop_entry.as_ref() == Some(&path) {
            receipt.integration.desktop_entry = None;
        }
        if receipt.integration.autostart.as_ref() == Some(&path) {
            receipt.integration.autostart = None;
        }
        if receipt.integration.cli.as_ref() == Some(&path) {
            receipt.integration.cli = None;
        }
        skipped(&mut receipt.integration, path);
    }
    if let Some(path) = &receipt.integration.desktop_entry {
        write(path, desktop(app, &receipt.path).as_bytes())?;
        cleanup_duplicates(environment, app, path, &receipt.path)?;
    }
    if let Some(path) = &receipt.integration.cli {
        symlink(path, &receipt.path)?;
    }
    if let Some(path) = &receipt.integration.autostart {
        repair_autostart(path, app, &receipt.path, true, false)?;
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
    environment.validate()?;
    if environment.receipts().read(id)?.is_none() {
        return Ok(false);
    }
    let _lock = lock(environment, id)?;
    let Some(receipt) = environment.receipts().read(id)? else { return Ok(false) };
    receipt.validate_native_paths()?;
    #[cfg(target_os = "linux")]
    validate_integration_roots(environment, &receipt)?;
    #[cfg(target_os = "linux")]
    if let Some(path) = integration_conflicts(&receipt)?.first() {
        return Err(input(&format!("integration ownership conflict at {}; repair to detach the foreign entry before uninstalling", path.display())));
    }
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
    for command in &receipt.integration.commands {
        // Only a link to this installation: never another program's command.
        if command.parent() == Some(environment.bin.as_path()) && fs::read_link(command).is_ok_and(|target| target == receipt.path) {
            remove_file(command)?;
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
    pub skipped: Vec<SkippedEntry>,
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
            entries.push(EntryStatus { path: p.clone(), exists: p.is_absolute() && p.exists() });
        }
    }
    let bin_on_path = environment.path.as_ref().is_some_and(|p| env::split_paths(p).any(|p| canonical(&p) == canonical(&environment.bin)));
    let skipped = receipt.as_ref().map_or_else(Vec::new, |r| r.integration.skipped.clone());
    #[cfg(target_os = "linux")]
    let skipped = {
        let mut skipped = skipped;
        if let Some(r) = &receipt {
            if r.validate_native_paths().is_ok() {
                for path in integration_conflicts(r)? {
                    if !skipped.iter().any(|entry| entry.path == path) {
                        skipped.push(SkippedEntry { path, reason: "existing entry belongs to another installation".into() });
                    }
                }
            }
        }
        skipped
    };
    Ok(IntegrationStatus { receipt, entries, bin_on_path, path_hint: (!bin_on_path).then(|| path_hint(&environment.bin, shell)), skipped })
}

/// Asks a running copy of `id` (an older one, before its replacement starts)
/// to quit over the Link and waits up to `timeout` for its endpoint to go
/// away. `Ok(false)`: nothing was running. A busy app refuses with an error.
pub fn quit_running(locations: &crate::Locations, id: &str, timeout: std::time::Duration) -> io::Result<bool> {
    let me = crate::PeerInfo { id: format!("{id}.installer"), version: env!("CARGO_PKG_VERSION").into() };
    let Ok(endpoint) = crate::endpoint::read(locations, id) else { return Ok(false) };
    let mut client = match crate::Client::connect_with(locations, id, &me, std::time::Duration::from_secs(2)) {
        Ok(client) => client,
        Err(_) => return Ok(false), // a stale endpoint file
    };
    client.call(crate::wire::method::APP_QUIT, serde_json::json!({})).map_err(|e| io::Error::other(e.message))?;
    drop(client);
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let replaced = crate::endpoint::read(locations, id).map_or(true, |current| current.token != endpoint.token);
        // An app that exits without removing its endpoint is gone too.
        let exited = cfg!(target_os = "linux") && !Path::new(&format!("/proc/{}", endpoint.pid)).exists();
        let gone = replaced || exited;
        if gone {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, format!("{id} did not quit")))
}

/// Starts the installed copy with the original args. From an AppImage (and on
/// Windows) it starts a detached successor and returns `Ok`, and the caller
/// exits: exec would keep the downloaded image mounted until the app quits.
/// Elsewhere on Unix it replaces this process. The successor gets the user's
/// environment, not this image's runtime variables. The caller owns any
/// old-process wait handshake.
pub fn reexec(path: &Path, original_args: &[std::ffi::OsString]) -> io::Result<()> {
    crate::receipt::native_absolute(path)?;
    let mut cmd = Command::new(path);
    crate::childenv::sanitize(&mut cmd);
    cmd.args(original_args).env_remove("APPIMAGE").env_remove("APPDIR");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if env::var_os("APPIMAGE").is_none() {
            return Err(cmd.exec());
        }
        // Its own process group; output still reaches this process's stdout
        // and stderr, as with exec (e.g. `--install --silent --status`).
        cmd.stdin(std::process::Stdio::null()).process_group(0);
        cmd.spawn().map(|_| ())
    }
    #[cfg(not(unix))]
    {
        cmd.spawn().map(|_| ())
    }
}
