//! Helper programs the apps use but never ship. An app looks for the user's
//! own install first; when there is none, it can offer a per-user download
//! into a folder every Arcade app searches, so one download serves them all.
//!
//! Only Tesseract (OCR) today. Downloads go through the system `curl` (part
//! of Windows 10+, macOS and every desktop Linux) and are checked against a
//! pinned SHA-256 before anything runs.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// `<data>/arcade/engines`, beside the registry (`$ARCADE_HOME/engines` when set).
pub fn engines_dir() -> PathBuf {
    let registry = crate::paths::registry_dir();
    registry.parent().map(Path::to_path_buf).unwrap_or(registry).join("engines")
}

/// Launchers for downloaded engines. Apps search it after `PATH`.
pub fn bin_dir() -> PathBuf {
    engines_dir().join("bin")
}

/// The result of [`download_tesseract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Download {
    /// Installed and checked; the path runs Tesseract.
    Ready(PathBuf),
    /// The platform installer was opened; the user finishes it.
    InstallerOpened,
}

/// The user's Tesseract: `PATH`, the platform's usual install folders, then
/// a copy downloaded by any Arcade app.
pub fn find_tesseract() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("PATH").map(|p| env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(install_dirs());
    dirs.push(bin_dir());
    first_executable(&dirs, if cfg!(windows) { "tesseract.exe" } else { "tesseract" })
}

fn install_dirs() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = env::var_os(var) {
                dirs.push(PathBuf::from(root).join("Tesseract-OCR"));
            }
        }
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Programs").join("Tesseract-OCR"));
        }
        dirs
    }
    #[cfg(target_os = "macos")]
    {
        vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into(), "/opt/local/bin".into()]
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        vec!["/usr/bin".into(), "/usr/local/bin".into()]
    }
}

fn first_executable(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// What a download is: where it comes from, its size for the prompt, and its hash.
pub struct Source {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_mb: u32,
}

/// The download offered on this platform, if any. Linux x86_64 gets a
/// self-contained AppImage (English and seven other languages); Windows gets
/// the UB Mannheim installer, which the user runs.
pub fn tesseract_source() -> Option<Source> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(Source {
            url: "https://github.com/AlexanderP/tesseract-appimage/releases/download/v5.5.3/tesseract-5.5.3_lept-1.87-x86_64.AppImage",
            sha256: "ef025f1e29321f5871c3864d9c2a9cf9ab7ca688a5e084ca18a73068c2717a5d",
            size_mb: 24,
        })
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some(Source {
            url: "https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe",
            sha256: "c885fff6998e0608ba4bb8ab51436e1c6775c2bafc2559a19b423e18678b60c9",
            size_mb: 48,
        })
    } else {
        None
    }
}

/// Gets Tesseract for this user. Blocks for the whole download; call it off
/// the UI thread. On macOS it runs `brew install tesseract` when Homebrew is
/// present; elsewhere without a download it explains how to install.
pub fn download_tesseract() -> Result<Download, String> {
    if cfg!(target_os = "macos") {
        let brew = first_executable(&["/opt/homebrew/bin".into(), "/usr/local/bin".into()], "brew")
            .ok_or("Install Tesseract with Homebrew (`brew install tesseract`), then try again.")?;
        let status = Command::new(brew).args(["install", "tesseract"]).status().map_err(|e| format!("Homebrew: {e}"))?;
        if !status.success() {
            return Err("Homebrew could not install Tesseract".into());
        }
        return find_tesseract().map(Download::Ready).ok_or_else(|| "Tesseract was installed but not found".into());
    }
    let source = tesseract_source().ok_or("Install Tesseract (and its English language data) from your package manager, then try again.")?;
    let engines = engines_dir();
    let work = engines.join(".tesseract-download");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| format!("Cannot create {}: {e}", work.display()))?;
    let result = install_from(&source, &engines, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

fn install_from(source: &Source, engines: &Path, work: &Path) -> Result<Download, String> {
    let file = work.join(if cfg!(windows) { "tesseract-setup.exe" } else { "tesseract.AppImage" });
    fetch(source.url, &file)?;
    verify_sha256(&file, source.sha256)?;
    if cfg!(windows) {
        // The installer stays put until the user has finished with it.
        let setup = engines.join("tesseract-setup.exe");
        fs::rename(&file, &setup).map_err(|e| e.to_string())?;
        Command::new(&setup).spawn().map_err(|e| format!("Cannot open the Tesseract installer: {e}"))?;
        return Ok(Download::InstallerOpened);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        // Extracted once, so no FUSE is needed and nothing unpacks per run.
        let out = Command::new(&file).arg("--appimage-extract").current_dir(work).output().map_err(|e| format!("Cannot unpack Tesseract: {e}"))?;
        let unpacked = work.join("squashfs-root");
        if !out.status.success() || !unpacked.join("AppRun").is_file() {
            return Err("Cannot unpack Tesseract".into());
        }
        let target = engines.join("tesseract");
        let _ = fs::remove_dir_all(&target);
        fs::rename(&unpacked, &target).map_err(|e| e.to_string())?;
        let bin = engines.join("bin");
        fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        let launcher = bin.join("tesseract");
        let _ = fs::remove_file(&launcher);
        std::os::unix::fs::symlink("../tesseract/AppRun", &launcher).map_err(|e| e.to_string())?;
        let ok = Command::new(&launcher).arg("--version").output().is_ok_and(|o| o.status.success());
        if !ok {
            return Err("The downloaded Tesseract does not run on this system".into());
        }
        Ok(Download::Ready(launcher))
    }
    #[cfg(not(unix))]
    unreachable!()
}

fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--proto", "=https", "--tlsv1.2", "--retry", "2", "--max-time", "900", "-o"]).arg(to).arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("Cannot run curl to download: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Download failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn verify_sha256(file: &Path, expected: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    let mut f = fs::File::open(file).map_err(|e| e.to_string())?;
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual == expected {
        Ok(())
    } else {
        Err("The download did not match its expected checksum, so it was discarded".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("arcade-link-engines-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let dir = scratch("sha");
        let file = dir.join("f");
        fs::write(&file, b"abc").unwrap();
        verify_sha256(&file, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap();
        assert!(verify_sha256(&file, &"0".repeat(64)).is_err());
    }

    #[test]
    fn finds_the_first_executable_in_order() {
        let dir = scratch("find");
        let (a, b) = (dir.join("a"), dir.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("tool"), b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(a.join("tool"), b"").unwrap(); // not executable: skipped
            fs::set_permissions(b.join("tool"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(first_executable(&[a, b.clone()], "tool"), Some(b.join("tool")));
    }

    /// Network: `ARCADE_HOME=/tmp/x cargo test -p arcade-link --features engines -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_tesseract() {
        assert!(env::var_os("ARCADE_HOME").is_some(), "set ARCADE_HOME to a scratch directory");
        match download_tesseract().unwrap() {
            Download::Ready(path) => assert!(Command::new(path).arg("--list-langs").output().unwrap().status.success()),
            Download::InstallerOpened => {}
        }
    }
}
