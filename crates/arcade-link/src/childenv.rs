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
//!
//! [`on_startup`] undoes what the launcher leaves on the app itself: open
//! handles on the mount that every child would inherit, and a working
//! directory inside the mount.

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

/// Undoes what an AppImage's launcher leaves on this process. Call it first
/// thing in `main`; outside an AppImage it changes nothing.
///
/// - The type-2 runtime gives the app an open handle on its mount (fd 1023)
///   that isn't close-on-exec, and libraries can leave files in the mount
///   open too. Every program the app started inherited them, so a child
///   that outlives the app (the installed copy, a lean restart, another
///   Arcade app) kept the mount busy, and the old runtime and its mount
///   never went away. [`release_mount_handles`] makes them close-on-exec.
/// - linuxdeploy's `AppRun` changes into `$APPDIR/usr`, so a relative path
///   on the command line resolved inside the mount. The working directory
///   goes back to `$OWD`, where the user started the app.
pub fn on_startup() {
    release_mount_handles();
    let (Some(owd), Ok(current)) = (std::env::var_os("OWD"), std::env::current_dir()) else {
        return;
    };
    let appdir = std::env::var("APPDIR").ok();
    if !owd.is_empty() && in_mount(&current.to_string_lossy(), appdir.as_deref()) {
        let _ = std::env::set_current_dir(owd);
    }
}

/// Marks this process's open handles on an AppImage mount (`$APPDIR` or any
/// runtime's `.mount_*` directory) close-on-exec, so programs it starts
/// don't keep the mount busy. [`sanitize`] calls it as well; it is cheap and
/// idempotent. Linux only: elsewhere there are no such mounts.
pub fn release_mount_handles() {
    #[cfg(target_os = "linux")]
    {
        let appdir = std::env::var("APPDIR").ok();
        let Ok(entries) = std::fs::read_dir("/proc/self/fd") else {
            return;
        };
        let fds: Vec<i32> = entries
            .flatten()
            .filter_map(|entry| {
                let fd: i32 = entry.file_name().to_str()?.parse().ok()?;
                let target = std::fs::read_link(entry.path()).ok()?;
                (fd > 2 && in_mount(&target.to_string_lossy(), appdir.as_deref())).then_some(fd)
            })
            .collect();
        for fd in fds {
            // SAFETY: F_GETFD/F_SETFD only read and set this descriptor's
            // flags; a descriptor closed meanwhile makes them fail harmlessly.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 && flags & libc::FD_CLOEXEC == 0 {
                    libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                }
            }
        }
    }
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
    release_mount_handles();
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

/// [`sanitize`], unless `program` lives inside this process's own AppImage
/// mount: helpers shipped in the same image need its library paths.
pub fn sanitize_for<'a>(cmd: &'a mut Command, program: &Path) -> &'a mut Command {
    release_mount_handles();
    let own = std::env::var_os("APPDIR").is_some_and(|dir| !dir.is_empty() && program.starts_with(Path::new(&dir)));
    if !own {
        sanitize(cmd);
    }
    cmd
}

/// The sanitized environment as a whole, for spawn APIs that take a full
/// environment instead of a [`Command`].
pub fn sanitized_vars() -> Vec<(OsString, OsString)> {
    release_mount_handles();
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

    #[cfg(target_os = "linux")]
    #[test]
    fn children_do_not_inherit_handles_on_a_mount() {
        use std::ffi::CString;
        let mount = std::env::temp_dir().join(format!(".mount_ArcadeTest{}", std::process::id()));
        std::fs::create_dir_all(&mount).unwrap();
        let other = std::env::temp_dir().join(format!("arcade-childenv-{}", std::process::id()));
        std::fs::create_dir_all(&other).unwrap();
        // What the AppImage runtime does: an inheritable handle on the mount.
        let open = |dir: &Path| {
            let path = CString::new(dir.to_str().unwrap()).unwrap();
            // SAFETY: a plain open(2) of a directory we just created.
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
            assert!(fd > 2);
            fd
        };
        let (on_mount, elsewhere) = (open(&mount), open(&other));
        release_mount_handles();
        // What the child holds: the targets of the shell's own descriptors.
        let listing = Command::new("sh").args(["-c", r#"for f in /proc/$$/fd/*; do readlink "$f"; done"#]).output().unwrap();
        let seen = String::from_utf8_lossy(&listing.stdout).into_owned();
        assert!(!seen.contains(mount.to_str().unwrap()), "the child inherited the mount handle:\n{seen}");
        assert!(seen.contains(other.to_str().unwrap()), "other handles are left alone:\n{seen}");
        // SAFETY: closing the descriptors opened above.
        unsafe {
            libc::close(on_mount);
            libc::close(elsewhere);
        }
        let _ = std::fs::remove_dir(&mount);
        let _ = std::fs::remove_dir(&other);
    }

    #[test]
    fn moves_out_of_a_mounted_working_directory() {
        let home = Path::new("/home/u");
        assert_eq!(working_directory(Path::new("/tmp/.mount_ArcadeAB/usr"), Some(home), Some("/tmp/.mount_ArcadeAB")), Some(home.into()));
        assert_eq!(working_directory(Path::new("/home/u/Downloads"), Some(home), Some("/tmp/.mount_ArcadeAB")), None);
    }
}
