//! A log file for an app that runs without a terminal.
//!
//! Started from the launcher, at login or by Tools, an app's stderr often
//! goes nowhere (`/dev/null`, or no handle on Windows), so its warnings and
//! errors were lost although its manifest advertised a log folder.
//! [`capture_stderr`] sends that stderr to `<dir>/<name>.log` instead,
//! keeping one older file (`<name>.log.1`) once the log passes
//! [`MAX_BYTES`]. A terminal, a file, a pipe or the session journal keeps
//! receiving it.

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

/// Whether stderr is discarded: `/dev/null` or closed on Unix, no handle on
/// Windows. Anything else (a terminal, a file, a pipe, the journal) is read
/// by someone.
pub fn stderr_goes_nowhere() -> bool {
    #[cfg(unix)]
    {
        use std::os::fd::BorrowedFd;
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        // SAFETY: fd 2 is only borrowed for the duplicate below.
        let duplicate = unsafe { BorrowedFd::borrow_raw(2) }.try_clone_to_owned();
        let Ok(duplicate) = duplicate else { return true };
        let Ok(meta) = fs::File::from(duplicate).metadata() else { return true };
        meta.file_type().is_char_device()
            && fs::metadata("/dev/null").is_ok_and(|null| null.rdev() == meta.rdev())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE};
        // SAFETY: GetStdHandle only reads the process's standard handle.
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Redirects stderr to `<dir>/<name>.log` when it would otherwise be lost
/// ([`stderr_goes_nowhere`]). Returns the log file's path when it did. Call
/// it once, early, from the resident app.
pub fn capture_stderr(dir: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    if !stderr_goes_nowhere() {
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
            // Captured only because stderr is /dev/null here.
            assert!(capture_stderr(Path::new(&dir), "app").unwrap().is_some());
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

    /// A pipe (a terminal, journal or test runner reading stderr) keeps it.
    #[test]
    fn stderr_that_someone_reads_is_left_alone() {
        if std::env::var_os("ARCADE_LOGS_READ_CHILD").is_some() {
            let dir = std::env::temp_dir().join("arcade-logs-unused");
            assert!(capture_stderr(&dir, "app").unwrap().is_none());
            eprintln!("still on stderr");
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "logs::tests::stderr_that_someone_reads_is_left_alone", "--nocapture"])
            .env("ARCADE_LOGS_READ_CHILD", "1")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("still on stderr"));
    }
}
