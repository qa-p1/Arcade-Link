//! Where the registry, endpoints and handoff files live (SPEC §2).
//!
//! `ARCADE_HOME`, if set, replaces every root: `$ARCADE_HOME/apps`,
//! `$ARCADE_HOME/run` and `$ARCADE_HOME/handoff`.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn arcade_home() -> Option<PathBuf> {
    env::var_os("ARCADE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
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
        env_dir("XDG_DATA_HOME")
            .unwrap_or_else(|| home().join(".local/share"))
            .join("arcade/apps")
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
            None => env_dir("XDG_CACHE_HOME")
                .unwrap_or_else(|| home().join(".cache"))
                .join("arcade/run"),
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
        env_dir("XDG_CACHE_HOME")
            .unwrap_or_else(|| home().join(".cache"))
            .join("arcade/handoff")
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
        Locations {
            registry: registry_dir(),
            runtime: runtime_dir(),
            handoff: handoff_dir(),
        }
    }

    /// Every root under `root`, the same layout as `ARCADE_HOME`.
    pub fn under(root: &Path) -> Locations {
        Locations {
            registry: root.join("apps"),
            runtime: root.join("run"),
            handoff: root.join("handoff"),
        }
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
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a real directory", dir.display()),
            ));
        }
        // SAFETY: geteuid has no preconditions.
        let uid = unsafe { geteuid() };
        if meta.uid() != uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} belongs to another user", dir.display()),
            ));
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

/// Writes `bytes` to `path` through a temporary file and an atomic rename.
/// The file is readable only by the current user on Unix.
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(if private { 0o600 } else { 0o644 });
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_matches_the_arcade_home_layout() {
        let dir = env::temp_dir().join("arcade-link-paths-test");
        let l = Locations::under(&dir);
        assert_eq!(
            l.manifest("arcade.box"),
            dir.join("apps").join("arcade.box.json")
        );
        assert_eq!(
            l.endpoint("arcade.box"),
            dir.join("run").join("arcade.box.endpoint")
        );
        assert_eq!(l.handoff, dir.join("handoff"));
    }

    #[test]
    fn private_dir_is_0700() {
        let dir = env::temp_dir().join(format!("arcade-link-private-{}", std::process::id()));
        ensure_private_dir(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(dir).ok();
    }
}
