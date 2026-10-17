//! The environment for programs an app starts on the user's behalf.
//!
//! An AppImage's runtime, its `AppRun` and linuxdeploy's GTK hook point
//! `LD_LIBRARY_PATH`, `PATH`, `XDG_DATA_DIRS`, `GTK_*`, `GIO_*`, `PYTHON*` and
//! more into the image's temporary mount, force `GTK_THEME`, and change into
//! `$APPDIR/usr`. Another app started with that environment loads the wrong
//! libraries, themes and desktop files, and breaks when the mount goes away.
//! [`sanitize`] removes exactly that: mount entries from path lists, the
//! variables only the AppImage tooling sets, and a working directory inside
//! the mount. The user's own values are kept.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Set only by the AppImage runtime, `AppRun` or the linuxdeploy GTK hook.
const APPIMAGE_ONLY: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "GTK_DATA_PREFIX",
    "GTK_EXE_PREFIX",
    "GTK_PATH",
    "GTK_IM_MODULE_FILE",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_MODULE_DIR",
    "GI_TYPELIB_PATH",
    "PYTHONHOME",
    "PYTHONDONTWRITEBYTECODE",
    "QT_QPA_PLATFORM_PLUGIN_PATH",
];

/// A change to apply to a child's environment: `None` removes the variable.
pub type Change = (String, Option<String>);

fn in_mount(entry: &str, appdir: Option<&str>) -> bool {
    let entry = entry.trim_end_matches('/');
    appdir.is_some_and(|dir| {
        let dir = dir.trim_end_matches('/');
        !dir.is_empty() && (entry == dir || entry.starts_with(&format!("{dir}/")))
    }) || entry.split('/').any(|part| part.starts_with(".mount_"))
}

/// The changes [`sanitize`] makes, computed from `vars` (normally
/// `std::env::vars`). Pure, so it can be tested without touching the process.
pub fn changes<I, K, V>(vars: I) -> Vec<Change>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let vars: Vec<(String, String)> = vars.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
    let get = |key: &str| vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let appdir = get("APPDIR").filter(|dir| !dir.is_empty());
    let from_appimage = appdir.is_some() || get("APPIMAGE").is_some();
    // linuxdeploy's GTK hook forces Adwaita whenever it runs; its signature is
    // GTK_DATA_PREFIX pointing at the mount.
    let gtk_hook = get("GTK_DATA_PREFIX").is_some_and(|prefix| in_mount(prefix, appdir));
    let mut out = Vec::new();
    for (key, value) in &vars {
        if from_appimage && (APPIMAGE_ONLY.contains(&key.as_str()) || (key == "GTK_THEME" && gtk_hook)) {
            out.push((key.clone(), None));
            continue;
        }
        if !value.split(':').any(|entry| in_mount(entry, appdir)) {
            continue;
        }
        let mut kept: Vec<&str> = Vec::new();
        for entry in value.split(':') {
            if !entry.is_empty() && !in_mount(entry, appdir) && !kept.contains(&entry) {
                kept.push(entry);
            }
        }
        out.push((key.clone(), (!kept.is_empty()).then(|| kept.join(":"))));
    }
    out
}

/// A working directory for the child when ours is inside an AppImage mount.
pub fn working_directory(current: &Path, home: Option<&Path>, appdir: Option<&str>) -> Option<PathBuf> {
    let text = current.to_string_lossy();
    in_mount(&text, appdir).then(|| home.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/")))
}

/// Applies [`changes`] for this process's environment to `cmd`. Call it before
/// setting the child's own variables: later `cmd.env` calls still win.
pub fn sanitize(cmd: &mut Command) -> &mut Command {
    let vars = std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    for (key, value) in changes(vars) {
        match value {
            Some(value) => cmd.env(key, value),
            None => cmd.env_remove(key),
        };
    }
    let appdir = std::env::var("APPDIR").ok();
    if let Ok(current) = std::env::current_dir() {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        if let Some(dir) = working_directory(&current, home.as_deref(), appdir.as_deref()) {
            cmd.current_dir(dir);
        }
    }
    cmd
}

/// The sanitized environment as a whole, for spawn APIs that take a full
/// environment instead of a [`Command`].
pub fn sanitized_vars() -> Vec<(OsString, OsString)> {
    let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let utf8 = vars.iter().filter_map(|(k, v)| Some((k.to_str()?.to_owned(), v.to_str()?.to_owned()))).collect::<Vec<_>>();
    for (key, value) in changes(utf8) {
        vars.retain(|(k, _)| k.to_str() != Some(key.as_str()));
        if let Some(value) = value {
            vars.push((key.into(), value.into()));
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(vars: &[(&str, &str)]) -> Vec<(String, String)> {
        let changes = changes(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let mut out: Vec<(String, String)> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        for (key, value) in changes {
            out.retain(|(k, _)| *k != key);
            if let Some(value) = value {
                out.push((key, value));
            }
        }
        out.sort();
        out
    }

    #[test]
    fn strips_mount_entries_and_appimage_variables() {
        let env = apply(&[
            ("APPDIR", "/tmp/.mount_ArcadeAB"),
            ("APPIMAGE", "/home/u/Applications/Arcade/Arcade-Tools.AppImage"),
            ("GTK_DATA_PREFIX", "/tmp/.mount_ArcadeAB"),
            ("GTK_THEME", "Adwaita:light"),
            ("PATH", "/tmp/.mount_ArcadeAB/usr/bin/:/tmp/.mount_ArcadeCD/usr/bin:/usr/bin:/bin:/usr/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_ArcadeAB/usr/lib/:/tmp/.mount_ArcadeAB/usr/lib32/"),
            ("XDG_DATA_DIRS", "/tmp/.mount_ArcadeAB/usr/share/:/usr/share:/home/u/.local/share/flatpak/exports/share:"),
            ("PYTHONHOME", "/tmp/.mount_ArcadeAB/usr/"),
            ("HOME", "/home/u"),
            ("LANG", "C.UTF-8"),
        ]);
        assert_eq!(
            env,
            vec![
                ("HOME".into(), "/home/u".into()),
                ("LANG".into(), "C.UTF-8".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
                ("XDG_DATA_DIRS".into(), "/usr/share:/home/u/.local/share/flatpak/exports/share".into()),
            ]
        );
    }

    #[test]
    fn leaves_a_normal_environment_alone() {
        let vars = [("PATH", "/usr/bin:/bin"), ("GTK_THEME", "Adwaita:dark"), ("PYTHONHOME", "/opt/py")];
        assert!(changes(vars.iter().map(|(k, v)| (k.to_string(), v.to_string()))).is_empty());
    }

    #[test]
    fn keeps_a_user_theme_when_the_gtk_hook_did_not_run() {
        let env = apply(&[("APPIMAGE", "/a/X.AppImage"), ("APPDIR", "/tmp/.mount_X"), ("GTK_THEME", "Yaru-dark")]);
        assert_eq!(env, vec![("GTK_THEME".into(), "Yaru-dark".into())]);
    }

    #[test]
    fn moves_out_of_a_mounted_working_directory() {
        let home = Path::new("/home/u");
        assert_eq!(working_directory(Path::new("/tmp/.mount_ArcadeAB/usr"), Some(home), Some("/tmp/.mount_ArcadeAB")), Some(home.into()));
        assert_eq!(working_directory(Path::new("/home/u/Downloads"), Some(home), Some("/tmp/.mount_ArcadeAB")), None);
    }
}
