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
    // The folder is made with the first report, so a process that never crashes (a one-shot `--version`) creates
    // nothing (ARCADE_APP.md CLI-4).
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
    crate::paths::ensure_private_dir(dir)?;
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
        /// The report folder and the ancestors it may lack, outermost first; `mkdir` on one that exists fails
        /// harmlessly.
        dirs: Vec<CString>,
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
        let mut dirs: Vec<CString> = dir
            .ancestors()
            .take_while(|ancestor| !ancestor.exists())
            .filter_map(|ancestor| CString::new(ancestor.as_os_str().as_encoded_bytes()).ok())
            .collect();
        dirs.reverse();
        let text = format!("{header}kind: fatal signal\nsignal: ").into_bytes();
        if PREPARED.set(Prepared { dirs, path, text }).is_err() {
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
                // mkdir is async-signal-safe: make the folders that did not exist when the handler was installed.
                for dir in &p.dirs {
                    libc::mkdir(dir.as_ptr(), 0o700);
                }
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

const ARENA_MAX: &str = "MALLOC_ARENA_MAX";
/// Set when [`cap_malloc_arenas`] set `MALLOC_ARENA_MAX` itself.
static CAPPED_ARENAS: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Caps glibc's malloc arenas at two in this process and the WebKit processes it starts, unless the user set
/// `MALLOC_ARENA_MAX`. glibc gives a thread its own arena when the main one is contended, so how many WebKit made
/// while a window opened depended on timing: Tools' open-window PSS read about 257 or about 300 MB on the same build,
/// and capped, eight runners read 252–254 (Tools D18). Child processes inherit the variable; this process started
/// allocating before `main`, so it also gets `mallopt`. Call it first in `main`, while the process has one thread.
/// [`crate::childenv::sanitize`] removes the cap again for programs the app starts. Linux (glibc) only.
pub fn cap_malloc_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        if std::env::var_os(ARENA_MAX).is_some() {
            return;
        }
        std::env::set_var(ARENA_MAX, "2");
        let _ = CAPPED_ARENAS.set(());
        // SAFETY: mallopt only changes allocator tuning; M_ARENA_MAX limits arenas created from now on.
        unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) };
    }
}

/// The variable to remove from a child's environment: `MALLOC_ARENA_MAX` when [`cap_malloc_arenas`] set it.
pub(crate) fn capped_arena_variable() -> Option<&'static str> {
    CAPPED_ARENAS.get().map(|()| ARENA_MAX)
}

/// W0: gives freed heap back to the OS and drops this process's resident
/// pages of read-only file mappings (code `r-xp` and read-only data `r--p`)
/// that hold nothing but the file's own bytes. They refault from the page
/// cache in microseconds. Call it after startup and after each hide.
/// A mapping with any private copy (RELRO after relocation, text
/// relocations, a debugger's breakpoint) is never touched: discarding it
/// would undo the change. Linux only; elsewhere it does nothing.
pub fn trim() {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: malloc_trim only releases free memory; madvise(DONTNEED)
        // on a private file mapping none of whose pages was ever copied
        // (smaps: Anonymous and Swap 0) refaults the same bytes from the file.
        unsafe { libc::malloc_trim(0) };
        for (start, len) in clean_read_only_file_mappings() {
            unsafe { libc::madvise(start as *mut libc::c_void, len, libc::MADV_DONTNEED) };
        }
    }
}

/// `r-xp` and `r--p` file mappings of this process whose resident pages all
/// come from the file (`/proc/self/smaps`: no anonymous or swapped pages).
#[cfg(target_os = "linux")]
fn clean_read_only_file_mappings() -> Vec<(usize, usize)> {
    let Ok(smaps) = std::fs::read_to_string("/proc/self/smaps") else { return Vec::new() };
    let mut out = Vec::new();
    // (start, len, candidate, copied) of the mapping being read.
    let mut current: Option<(usize, usize, bool, bool)> = None;
    fn finish(current: Option<(usize, usize, bool, bool)>, out: &mut Vec<(usize, usize)>) {
        if let Some((start, len, true, false)) = current {
            out.push((start, len));
        }
    }
    for line in smaps.lines() {
        let mut fields = line.split_whitespace();
        let Some(first) = fields.next() else { continue };
        if let Some((a, b)) = first.split_once('-').filter(|_| !first.ends_with(':')) {
            finish(current.take(), &mut out);
            let perms = fields.next().unwrap_or("");
            let path = fields.nth(3).unwrap_or("");
            let file = path.starts_with('/') && !line.ends_with("(deleted)");
            let candidate = file && (perms == "r-xp" || perms == "r--p");
            current = match (usize::from_str_radix(a, 16), usize::from_str_radix(b, 16)) {
                (Ok(a), Ok(b)) if b > a => Some((a, b - a, candidate, false)),
                _ => None,
            };
        } else if first == "Anonymous:" || first == "Swap:" {
            if let Some(mapping) = current.as_mut() {
                mapping.3 |= fields.next().is_some_and(|kb| kb != "0");
            }
        }
    }
    finish(current, &mut out);
    out
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

/// W3b: asks the kernel to page out this process's private memory while it is
/// hidden: the heap and anonymous memory, and the pages a private file mapping
/// holds (written ones such as RELRO and `.data` go to swap, clean ones are
/// dropped and read back from the file). `MADV_PAGEOUT` never changes contents,
/// so nothing is lost; it costs nothing unless the memory is needed again.
/// Stacks, guard pages and the kernel's own mappings are left alone.
pub fn page_out_hidden() {
    #[cfg(target_os = "linux")]
    for (start, len) in private_mappings() {
        // SAFETY: MADV_PAGEOUT is advisory and keeps contents intact.
        unsafe { libc::madvise(start as *mut libc::c_void, len, libc::MADV_PAGEOUT) };
    }
}

#[cfg(target_os = "linux")]
fn private_mappings() -> Vec<(usize, usize)> {
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else { return Vec::new() };
    maps.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let range = f.next()?;
            let perms = f.next()?;
            let name = f.nth(3).unwrap_or("");
            let special = name.starts_with('[') && name != "[heap]" && !name.starts_with("[anon");
            if !perms.ends_with('p') || perms.starts_with('-') || special {
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

/// Set in a process started from the unpacked copy, to the copy's folder. `1` (a test harness, or by hand) means
/// "never relaunch". Any other value names another copy: it was inherited (a Restart's successor, or an app started by
/// one), so it does not stop this process from relaunching; [`crate::childenv::sanitize`] removes it for children.
pub const UNPACKED_ENV: &str = "ARCADE_UNPACKED";

/// Whether this process runs from its unpacked copy (or was told never to relaunch): [`UNPACKED_ENV`] is `1` or names
/// the folder this executable runs from. Apps use it for "I am the copy" decisions instead of testing the variable.
pub fn is_unpacked_copy() -> bool {
    std::env::var_os(UNPACKED_ENV).is_some_and(|value| marks(&value, std::env::current_exe().ok().as_deref()))
}

/// Whether the [`UNPACKED_ENV`] value `value` marks a process running `exe`: `1`, or the folder `exe` is inside (both
/// compared canonical, so a symlinked data folder still matches and the copy never relaunches itself).
fn marks(value: &std::ffi::OsStr, exe: Option<&Path>) -> bool {
    if value == "1" {
        return true;
    }
    let (Ok(folder), Some(exe)) = (std::fs::canonicalize(value), exe) else { return false };
    exe.starts_with(&folder) || std::fs::canonicalize(exe).is_ok_and(|exe| exe.starts_with(&folder))
}

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
            if is_unpacked_copy() {
                return None;
            }
            // Inherited from another app's or an older process's copy: not this process's. It runs first thing in
            // main, before any thread starts.
            std::env::remove_var(UNPACKED_ENV);
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
        cmd.args(std::env::args_os().skip(1));
        // This process's AppRun pointed GSETTINGS_SCHEMA_DIR, GIO_EXTRA_MODULES, XDG_DATA_DIRS and more into the mount,
        // which goes away as soon as this process exits. The copy's AppRun sets them again for the copy, but only
        // after its GTK hook has run `gsettings`, which then read the vanishing mount, crashed, and hung in its core
        // dump: the copy never started (Look's rc.2 gates, about one start in twelve on CI).
        let vars = std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
        for (key, value) in copy_changes(vars) {
            match value {
                Some(value) => cmd.env(key, value),
                None => cmd.env_remove(key),
            };
        }
        cmd.env("APPDIR", &dir).env(UNPACKED_ENV, &dir);
        crate::childenv::detach_from_image(&mut cmd);
        // The copy sets the arena cap itself and so owns it: inherited, it took the cap for the user's and passed it
        // on to every program it started.
        if let Some(variable) = capped_arena_variable() {
            cmd.env_remove(variable);
        }
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

/// The changes to this process's environment for its unpacked copy: [`crate::childenv::changes`] (every reference to
/// the mount goes), except the variables the copy is still meant to see: `APPIMAGE` (what restarts and advertises
/// the app), `OWD` (where the user started it) and `ARGV0`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn copy_changes<I: IntoIterator<Item = (String, String)>>(vars: I) -> Vec<crate::childenv::Change> {
    const KEEP: &[&str] = &["APPIMAGE", "OWD", "ARGV0"];
    crate::childenv::changes(vars).into_iter().filter(|(key, _)| !KEEP.contains(&key.as_str())).collect()
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

/// Removes one app's unpacked copies under `root` (`<app>-<version>-<key>` and an interrupted `.<app>-…tmp-<pid>`),
/// leaving other apps' copies alone. Uninstall runs this, so an app leaves no runtime copy behind. Best effort: a
/// copy that cannot be removed is reported and skipped (it is a cache Tools also collects). Returns the folders
/// removed.
pub fn remove_unpacked_of(root: &Path, app_id: &str) -> Vec<PathBuf> {
    let short = app_id.trim_start_matches("arcade.");
    let (own, interrupted) = (format!("{short}-"), format!(".{short}-"));
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with(&own) || name.starts_with(&interrupted)) || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => removed.push(entry.path()),
            Err(e) => eprintln!("arcade-link: could not remove the unpacked copy {}: {e}", entry.path().display()),
        }
    }
    removed
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_sees_nothing_of_the_mount_but_keeps_what_names_the_install() {
        let vars = [
            ("APPDIR", "/tmp/.mount_ArcadeAB"),
            ("APPIMAGE", "/home/u/Applications/Arcade/Arcade-Look.AppImage"),
            ("OWD", "/home/u/Pictures"),
            ("ARGV0", "arcade-look"),
            ("GSETTINGS_SCHEMA_DIR", "/tmp/.mount_ArcadeAB//usr/share/glib-2.0/schemas"),
            ("GIO_EXTRA_MODULES", "/tmp/.mount_ArcadeAB/usr/lib/x86_64-linux-gnu/gio/modules"),
            ("GTK_DATA_PREFIX", "/tmp/.mount_ArcadeAB"),
            ("GTK_THEME", "Adwaita:light"),
            ("XDG_DATA_DIRS", "/tmp/.mount_ArcadeAB/usr/share:/usr/share:/usr/local/share"),
            ("PATH", "/tmp/.mount_ArcadeAB/usr/bin:/usr/bin:/bin"),
            ("HOME", "/home/u"),
        ];
        let mut changes = copy_changes(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        changes.sort();
        let gone = |key: &str| (key.to_string(), None);
        assert_eq!(
            changes,
            vec![
                gone("APPDIR"),
                gone("GIO_EXTRA_MODULES"),
                gone("GSETTINGS_SCHEMA_DIR"),
                gone("GTK_DATA_PREFIX"),
                gone("GTK_THEME"),
                ("PATH".to_string(), Some("/usr/bin:/bin".to_string())),
                ("XDG_DATA_DIRS".to_string(), Some("/usr/share:/usr/local/share".to_string())),
            ]
        );
    }

    #[test]
    fn the_unpacked_marker_is_ours_only_for_our_own_copy() {
        let root = std::env::temp_dir().join(format!("arcade-marker-{}", std::process::id()));
        let ours = root.join("box-1.0.0-0123456789abcdef");
        let other = root.join("look-1.0.0-0123456789abcdef");
        std::fs::create_dir_all(ours.join("usr/bin")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let exe = ours.join("usr/bin/arcade-desktop");
        std::fs::write(&exe, b"").unwrap();
        // The copy itself, and "never relaunch".
        assert!(marks(ours.as_os_str(), Some(&exe)));
        assert!(marks(std::ffi::OsStr::new("1"), None));
        // Inherited from another app's copy, a folder that is gone, or nothing to compare with: not ours.
        assert!(!marks(other.as_os_str(), Some(&exe)));
        assert!(!marks(root.join("gone").as_os_str(), Some(&exe)));
        assert!(!marks(ours.as_os_str(), None));
        assert!(!marks(std::ffi::OsStr::new(""), Some(&exe)));
        // A data folder reached through a symlink still matches.
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&root, &link).unwrap();
            assert!(marks(link.join("box-1.0.0-0123456789abcdef").as_os_str(), Some(&exe)));
            assert!(marks(ours.as_os_str(), Some(&link.join("box-1.0.0-0123456789abcdef/usr/bin/arcade-desktop"))));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
