//! Process plumbing every Arcade app needs once: single instance, restart,
//! reaped children, local crash reports and memory trimming
//! (`ARCADE_APP.md` RT-1…RT-5). Shelf and Wheel (Qt) and Clipboard's
//! Flutter shell have their own implementations held to the same contract
//! tests (`spec/contract/`); `qt/arcade_trim.h` is the C twin of [`trim`].

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Single instance (RT-1)

/// Held by the one running instance. Dropping it (or the process ending, even
/// by a crash) frees the slot.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Tries to become the only running instance of `name` (an advisory lock on
/// `<dir>/<name>.lock`). `Ok(None)` when another process holds it.
pub fn acquire_instance(dir: &Path, name: &str) -> io::Result<Option<InstanceLock>> {
    crate::paths::ensure_private_dir(dir)?;
    let path = dir.join(format!("{name}.lock"));
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&path)?;
    if !try_lock(&file)? {
        return Ok(None);
    }
    let _ = (&file).write_all(format!("{}\n", std::process::id()).as_bytes());
    Ok(Some(InstanceLock { _file: file, path }))
}

/// [`acquire_instance`], waiting up to `timeout` (a restarted successor
/// waiting for its predecessor to exit).
pub fn acquire_instance_waiting(dir: &Path, name: &str, timeout: Duration) -> io::Result<Option<InstanceLock>> {
    let end = Instant::now() + timeout;
    loop {
        if let Some(lock) = acquire_instance(dir, name)? {
            return Ok(Some(lock));
        }
        if Instant::now() >= end {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    // SAFETY: flock on a descriptor we own.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(e)
    }
}

#[cfg(windows)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
    use windows_sys::Win32::Storage::FileSystem::{LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY};
    // SAFETY: a zeroed OVERLAPPED (offset 0) and a handle we own.
    let ok = unsafe {
        let mut overlapped = std::mem::zeroed();
        LockFileEx(file.as_raw_handle() as _, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY, 0, 1, 0, &mut overlapped)
    };
    if ok != 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
        Ok(false)
    } else {
        Err(e)
    }
}

// ---------------------------------------------------------------------------
// Restart (RT-2)

/// Set on a restarted successor: the predecessor's PID.
pub const RESTART_AFTER_ENV: &str = "ARCADE_RESTART_AFTER";

/// Starts a new instance of this app from what the user installed
/// (`$APPIMAGE`, never a file inside the AppImage's mount) with a sanitized
/// environment. The caller quits only if this succeeds, so a failed restart
/// never leaves the user without the app.
pub fn spawn_successor(args: &[&str]) -> io::Result<()> {
    let program = crate::manifest::current_executable();
    let mut cmd = Command::new(&program);
    crate::childenv::sanitize_for(&mut cmd, Path::new(&program)).args(args).env(RESTART_AFTER_ENV, std::process::id().to_string());
    spawn_reaped(&mut cmd).map(|_| ())
}

/// In a successor started by [`spawn_successor`]: waits (bounded) for the
/// predecessor to exit. Returns at once in any other process.
pub fn wait_for_predecessor(timeout: Duration) {
    if let Some(pid) = std::env::var(RESTART_AFTER_ENV).ok().and_then(|p| p.parse::<u32>().ok()) {
        wait_for_exit(pid, timeout);
    }
}

/// Waits (bounded) until process `pid` no longer exists.
pub fn wait_for_exit(pid: u32, timeout: Duration) {
    let end = Instant::now() + timeout;
    while process_alive(pid) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists.
        let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
        r == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        // SAFETY: the handle is checked and closed.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE as u32
        }
    }
}

// ---------------------------------------------------------------------------
// Children (RT-3)

/// Spawns `cmd` and reaps it when it exits, so a child the app no longer
/// watches never lingers as a zombie. Returns its PID.
pub fn spawn_reaped(cmd: &mut Command) -> io::Result<u32> {
    let child = cmd.spawn()?;
    let pid = child.id();
    reap(child);
    Ok(pid)
}

/// Hands an already started child to a reaper thread.
pub fn reap(mut child: Child) {
    let spawned = std::thread::Builder::new().name("arcade-reaper".into()).stack_size(64 * 1024).spawn(move || {
        let _ = child.wait();
    });
    if let Err(e) = spawned {
        eprintln!("arcade-link: no reaper thread ({e}); the child may become a zombie");
    }
}

// ---------------------------------------------------------------------------
// Crash reports (RT-4)

const KEEP_REPORTS: usize = 20;

/// Where crash reports go: `<data>/crashes/` of the app (`arcade/<app>/crashes`).
pub fn crash_dir(app_id: &str) -> PathBuf {
    crate::paths::app_dirs(app_id).data.join("crashes")
}

/// Writes a local report for a panic or a fatal signal into `dir` and keeps
/// the newest [`KEEP_REPORTS`]. Nothing is sent anywhere: Tools shows the
/// report, and the user may copy it or open a GitHub issue after reviewing it.
pub fn install_crash_reports(app_name: &str, version: &str, dir: PathBuf) {
    let header = format!("app: {app_name}\nversion: {version}\nos: {} {}\npid: {}\n", std::env::consts::OS, std::env::consts::ARCH, std::process::id());
    let _ = crate::paths::ensure_private_dir(&dir);
    prune_reports(&dir, KEEP_REPORTS - 1);
    #[cfg(unix)]
    signals::install(&dir, &header);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(non-string panic payload)".into());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let report = format!(
            "{header}kind: panic\nthread: {thread}\nlocation: {location}\nmessage: {message}\n\nbacktrace:\n{}\n",
            std::backtrace::Backtrace::force_capture()
        );
        let _ = write_report(&dir, "panic", report.as_bytes());
        previous(info);
    }));
}

/// The reports in `dir`, newest first.
pub fn crash_reports(dir: &Path) -> Vec<PathBuf> {
    let mut reports: Vec<PathBuf> =
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "crash")).collect();
    reports.sort();
    reports.reverse();
    reports
}

fn prune_reports(dir: &Path, keep: usize) {
    for old in crash_reports(dir).into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
}

fn report_name(kind: &str) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{secs:011}-{}-{kind}.crash", std::process::id())
}

fn write_report(dir: &Path, kind: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let path = dir.join(report_name(kind));
    crate::paths::write_atomic(&path, bytes, true)?;
    prune_reports(dir, KEEP_REPORTS);
    Ok(path)
}

#[cfg(unix)]
mod signals {
    use std::ffi::CString;
    use std::path::Path;
    use std::sync::OnceLock;

    const FATAL: [libc::c_int; 5] = [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGFPE, libc::SIGABRT];

    struct Prepared {
        path: CString,
        text: Vec<u8>,
    }

    /// Everything the handler writes is prepared now; the handler itself only
    /// calls open, write, close, sigaction and raise (async-signal-safe).
    static PREPARED: OnceLock<Prepared> = OnceLock::new();
    static mut PREVIOUS: [libc::sigaction; 5] = unsafe { std::mem::zeroed() };

    pub(super) fn install(dir: &Path, header: &str) {
        let path = dir.join(super::report_name("signal"));
        let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) else { return };
        let text = format!("{header}kind: fatal signal\nsignal: ").into_bytes();
        if PREPARED.set(Prepared { path, text }).is_err() {
            return;
        }
        // SAFETY: an alternate stack lets the handler run after a stack
        // overflow; sigaction with a plain function pointer.
        unsafe {
            let size = 64 * 1024;
            let stack = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0);
            if stack != libc::MAP_FAILED {
                let ss = libc::stack_t { ss_sp: stack, ss_flags: 0, ss_size: size };
                libc::sigaltstack(&ss, std::ptr::null_mut());
            }
            for (i, sig) in FATAL.iter().enumerate() {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = handler as extern "C" fn(libc::c_int) as usize;
                action.sa_flags = libc::SA_ONSTACK;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(*sig, &action, std::ptr::addr_of_mut!(PREVIOUS[i]));
            }
        }
    }

    extern "C" fn handler(sig: libc::c_int) {
        // SAFETY: async-signal-safe calls only; PREVIOUS was written once,
        // before any handler could run.
        unsafe {
            if let Some(p) = PREPARED.get() {
                let fd = libc::open(p.path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC, 0o600);
                if fd >= 0 {
                    libc::write(fd, p.text.as_ptr().cast(), p.text.len());
                    let mut digits = [0u8; 12];
                    let mut n = sig as u32;
                    let mut at = digits.len() - 1;
                    digits[at] = b'\n';
                    loop {
                        at -= 1;
                        digits[at] = b'0' + (n % 10) as u8;
                        n /= 10;
                        if n == 0 {
                            break;
                        }
                    }
                    libc::write(fd, digits[at..].as_ptr().cast(), digits.len() - at);
                    libc::close(fd);
                }
            }
            if let Some(i) = FATAL.iter().position(|s| *s == sig) {
                libc::sigaction(sig, std::ptr::addr_of!(PREVIOUS[i]), std::ptr::null_mut());
            }
            libc::raise(sig);
            // The previous handler returned (Rust's own SIGSEGV handler does
            // for anything but a stack overflow): die with the signal anyway.
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }
}

// ---------------------------------------------------------------------------
// Memory (RT-5; AUDIT/memory.md W0, W1, W3b)

/// W0: gives freed heap back to the OS and drops this process's resident
/// executable code pages (`r-xp` file mappings). Code refaults from the page
/// cache in microseconds. Call it after startup and after each hide.
/// Read-only data and RELRO mappings are never touched: discarding them
/// would undo relocations. Linux only; elsewhere it does nothing.
pub fn trim() {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: malloc_trim only releases free memory; madvise(DONTNEED)
        // on private, never-written executable file mappings refaults the
        // same bytes from the file.
        unsafe { libc::malloc_trim(0) };
        for (start, len) in mappings(|perms, file| file && perms.starts_with("r-xp")) {
            unsafe { libc::madvise(start as *mut libc::c_void, len, libc::MADV_DONTNEED) };
        }
    }
}

/// W1: after the last WebKit window closed, ends this app's
/// `WebKitNetworkProcess` children (WebKit starts a new one with the next
/// window) and then runs [`trim`]. Returns how many it signalled.
pub fn release_webkit() -> usize {
    #[cfg(target_os = "linux")]
    let n = children_named("WebKitNetworkPr")
        .into_iter()
        // SAFETY: SIGTERM to our own child process.
        .filter(|pid| unsafe { libc::kill(*pid as libc::pid_t, libc::SIGTERM) } == 0)
        .count();
    #[cfg(not(target_os = "linux"))]
    let n = 0;
    trim();
    n
}

/// W3b: asks the kernel to page out this process's private anonymous memory
/// while it is hidden. Free unless the machine is under memory pressure.
pub fn page_out_hidden() {
    #[cfg(target_os = "linux")]
    for (start, len) in mappings(|perms, file| !file && perms.starts_with("rw-p")) {
        // SAFETY: MADV_PAGEOUT is advisory and keeps contents intact.
        unsafe { libc::madvise(start as *mut libc::c_void, len, libc::MADV_PAGEOUT) };
    }
}

#[cfg(target_os = "linux")]
fn mappings(keep: impl Fn(&str, bool) -> bool) -> Vec<(usize, usize)> {
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else { return Vec::new() };
    maps.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let range = f.next()?;
            let perms = f.next()?;
            let path = f.nth(3).unwrap_or("");
            let file = path.starts_with('/') && !line.ends_with("(deleted)");
            if path.starts_with('[') || !keep(perms, file) {
                return None;
            }
            let (a, b) = range.split_once('-')?;
            let (a, b) = (usize::from_str_radix(a, 16).ok()?, usize::from_str_radix(b, 16).ok()?);
            Some((a, b - a))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn children_named(comm: &str) -> Vec<u32> {
    let me = std::process::id().to_string();
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            // comm is in parentheses and may contain spaces; ppid follows it.
            let Some(close) = stat.rfind(')') else { return false };
            let name = stat.get(stat.find('(').map_or(0, |o| o + 1)..close).unwrap_or("");
            let ppid = stat[close + 1..].split_whitespace().nth(1).unwrap_or("");
            name == comm && ppid == me
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Unpacked runtime (RT-7; AUDIT/memory.md U1)

/// Set in a process started from the unpacked copy.
pub const UNPACKED_ENV: &str = "ARCADE_UNPACKED";

/// Where this app's unpacked copies live: `<data>/arcade/runtime/`.
pub fn unpacked_root() -> PathBuf {
    crate::paths::UserDirs::discover().data.join("arcade").join("runtime")
}

/// U1: an app running from its AppImage's FUSE mount copies the image's
/// contents once to `<data>/arcade/runtime/<app>-<version>-<key>/` and runs
/// the same command line from there, so no FUSE helper stays resident and
/// file pages come from the real disk. `$APPIMAGE` is kept, so everything
/// that advertises or restarts the app still uses the installed AppImage.
///
/// Call it first thing in `main`. It returns `None` when nothing is to be
/// done (not in an AppImage, already unpacked, or copying failed: the app
/// then simply keeps running from the mount). Otherwise it has started the
/// unpacked copy and returns the code this process must exit with: with
/// `wait` (one-shot CLI calls) the copy's own exit code, without it
/// (resident starts) 0 at once, so the mount goes away.
pub fn relaunch_unpacked(app_id: &str, version: &str, wait: bool) -> Option<i32> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os(UNPACKED_ENV).is_some() {
            return None;
        }
        let appimage = PathBuf::from(std::env::var_os("APPIMAGE").filter(|v| !v.is_empty())?);
        let appdir = PathBuf::from(std::env::var_os("APPDIR").filter(|v| !v.is_empty())?);
        if !std::env::current_exe().ok()?.starts_with(&appdir) {
            return None;
        }
        let dir = unpacked_root().join(format!("{}-{version}-{}", app_id.trim_start_matches("arcade."), image_key(&appimage)?));
        if let Err(e) = ensure_unpacked(&appdir, &dir) {
            eprintln!("arcade-link: running from the AppImage mount ({}: {e})", dir.display());
            return None;
        }
        let mut cmd = Command::new(dir.join("AppRun"));
        cmd.args(std::env::args_os().skip(1)).env("APPDIR", &dir).env(UNPACKED_ENV, "1");
        crate::childenv::detach_from_image(&mut cmd);
        let mut child = cmd.spawn().ok()?;
        if wait {
            return Some(child.wait().ok().and_then(|s| s.code()).unwrap_or(1));
        }
        Some(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_id, version, wait);
        None
    }
}

/// Identifies one AppImage file: its size and modification time. A new
/// version or an update in place gets a new folder.
#[cfg(target_os = "linux")]
fn image_key(appimage: &Path) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(appimage).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    meta.len().hash(&mut h);
    meta.modified().ok()?.hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

#[cfg(target_os = "linux")]
/// Copies `from` to `to` once: into a temporary sibling, then renamed, with a
/// `.complete` marker, so a crash mid-copy never leaves a half copy in use.
fn ensure_unpacked(from: &Path, to: &Path) -> io::Result<()> {
    if to.join(".complete").is_file() {
        return Ok(());
    }
    let parent = to.parent().ok_or_else(|| io::Error::other("no parent"))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{}.tmp-{}", to.file_name().and_then(|n| n.to_str()).unwrap_or("app"), std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_tree(from, &tmp)?;
    std::fs::write(tmp.join(".complete"), b"")?;
    match std::fs::rename(&tmp, to) {
        Ok(()) => Ok(()),
        // Another process won the race: use its copy.
        Err(_) if to.join(".complete").is_file() => {
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

#[cfg(target_os = "linux")]
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    std::fs::set_permissions(to, std::fs::metadata(from)?.permissions())?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
        } else if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Removes unpacked copies except those named in `keep` (Tools runs this after
/// an update or uninstall). Returns the folders removed.
pub fn remove_unpacked_except(keep: &[&str]) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(unpacked_root()).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(&name.as_str()) && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed.push(entry.path());
        }
    }
    removed
}
