//! A log file for an app that runs without a terminal.
//!
//! Started from the launcher, at login or by Tools, an app's stderr goes
//! nowhere, so its warnings and errors were lost although its manifest
//! advertised a log folder. [`capture_stderr`] sends stderr to
//! `<dir>/<name>.log` instead, keeping one older file (`<name>.log.1`) once
//! the log passes [`MAX_BYTES`]. In a terminal nothing changes.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The size at which the log is rotated when the app starts.
pub const MAX_BYTES: u64 = 1024 * 1024;

#[cfg(unix)]
extern "C" {
    fn isatty(fd: i32) -> i32;
    fn dup2(old: i32, new: i32) -> i32;
}

/// Whether stderr is a terminal (where the user reads it directly).
pub fn stderr_is_terminal() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: isatty only inspects the descriptor.
        unsafe { isatty(2) == 1 }
    }
    #[cfg(windows)]
    {
        use std::io::IsTerminal;
        io::stderr().is_terminal()
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// Redirects stderr to `<dir>/<name>.log` unless it is a terminal. Returns
/// the log file's path when it did. Call it once, early, from the resident
/// app (not from command-line subcommands meant for a user's terminal).
pub fn capture_stderr(dir: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    if stderr_is_terminal() {
        return Ok(None);
    }
    capture_stderr_to(dir, name).map(Some)
}

/// [`capture_stderr`] without the terminal check (for tests and callers
/// that already decided).
pub fn capture_stderr_to(dir: &Path, name: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.log"));
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join(format!("{name}.log.1")));
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "--- {} started (pid {}) ---", name, std::process::id())?;
    redirect(&file)?;
    Ok(path)
}

#[cfg(unix)]
fn redirect(file: &fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: both descriptors are valid; dup2 replaces fd 2 atomically.
    if unsafe { dup2(file.as_raw_fd(), 2) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn redirect(file: &fs::File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    // The handle must outlive the process's use of stderr: leak a clone.
    let handle = file.try_clone()?;
    let raw = handle.as_raw_handle();
    std::mem::forget(handle);
    // SAFETY: a valid, leaked file handle.
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, raw as _) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn redirect(_: &fs::File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs in a child process (this same test binary), since it replaces
    /// the process's stderr.
    #[test]
    fn stderr_goes_to_a_rotated_log() {
        if let Some(dir) = std::env::var_os("ARCADE_LOGS_CHILD") {
            capture_stderr_to(Path::new(&dir), "app").unwrap();
            eprintln!("written to the log");
            return;
        }
        let dir = std::env::temp_dir().join(format!("arcade-logs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("app.log"), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logs::tests::stderr_goes_to_a_rotated_log", "--nocapture"])
            .env("ARCADE_LOGS_CHILD", &dir)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let log = fs::read_to_string(dir.join("app.log")).unwrap();
        assert!(log.contains("app started") && log.contains("written to the log"), "{log}");
        assert_eq!(fs::metadata(dir.join("app.log.1")).unwrap().len(), MAX_BYTES + 1);
        let _ = fs::remove_dir_all(dir);
    }
}
