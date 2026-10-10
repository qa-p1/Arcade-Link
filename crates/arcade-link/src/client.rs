//! The calling side: connect, say hello, call methods (SPEC §4, §7).
//!
//! Everything here blocks. Call it from a worker thread, never a UI thread.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use interprocess::local_socket::Stream;
use serde_json::{json, Value};

use crate::endpoint;
use crate::error::{ErrorCode, LinkError};
use crate::manifest::{Action, Manifest};
use crate::paths::Locations;
use crate::transport;
use crate::wire::{self, method, InvokeRequest, InvokeResult, JobDone, JobProgress, LineReader, Message, PeerInfo};
use crate::Content;

/// An endpoint is dead if connecting or `hello` takes longer than this.
pub const HELLO_TIMEOUT: Duration = Duration::from_millis(150);
/// How long to wait for a launched app's endpoint.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(3);
/// Show a spinner if a launch takes longer than this.
pub const SPINNER_DELAY: Duration = Duration::from_millis(150);
/// Default answer time for ordinary (non-job) calls.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

struct ArcRead(Arc<Stream>, transport::IoControl);

impl std::io::Read for ArcRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.1.read(&self.0, buf)
    }
}
struct ArcWrite<'a>(&'a Stream, &'a transport::IoControl);
impl std::io::Write for ArcWrite<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.1.write(self.0, buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.1.flush(self.0)
    }
}

/// A connection to one running app.
pub struct Client {
    stream: Arc<Stream>,
    io: transport::IoControl,
    reader: LineReader<ArcRead>,
    next_id: u64,
    /// The app on the other end.
    pub server: PeerInfo,
    /// The negotiated protocol version.
    pub protocol: u32,
    notifications: std::collections::VecDeque<Message>,
}

/// A cancellation handle for a blocking subscription. Closing is idempotent
/// on Unix; Windows signals a cancellation event and cancels pending pipe I/O.
/// Closing before a read starts also cancels that read. The Client is dropped after
/// cancellation. This never kills a process.
#[derive(Clone)]
pub struct ConnectionControl {
    stream: Arc<Stream>,
    io: transport::IoControl,
}
impl ConnectionControl {
    pub fn close(&self) {
        self.io.close(&self.stream);
    }
}

fn not_running(app_id: &str) -> LinkError {
    LinkError::new(ErrorCode::NotRunning, format!("{app_id} is not running"))
}

impl Client {
    pub fn connection_control(&self) -> ConnectionControl {
        ConnectionControl { stream: self.stream.clone(), io: self.io.clone() }
    }
    pub fn set_timeout(&self, timeout: Duration) -> Result<(), LinkError> {
        self.io.recv_timeout(&self.stream, Some(timeout))?;
        self.io.send_timeout(&self.stream, Some(timeout))?;
        Ok(())
    }
    /// Connects to `app_id`'s endpoint and authenticates.
    pub fn connect(locations: &Locations, app_id: &str, me: &PeerInfo) -> Result<Client, LinkError> {
        Client::connect_with(locations, app_id, me, HELLO_TIMEOUT)
    }

    pub fn connect_with(locations: &Locations, app_id: &str, me: &PeerInfo, timeout: Duration) -> Result<Client, LinkError> {
        let ep = endpoint::read(locations, app_id).map_err(|_| not_running(app_id))?;
        let stream = transport::connect(&ep.address, timeout).map_err(|_| not_running(app_id))?;
        let stream = Arc::new(stream);
        let io = transport::IoControl::new()?;
        io.recv_timeout(&stream, Some(timeout))?;
        io.send_timeout(&stream, Some(timeout))?;
        let mut c = Client {
            reader: LineReader::new(ArcRead(stream.clone(), io.clone())),
            stream,
            io,
            next_id: 0,
            server: PeerInfo::default(),
            protocol: 0,
            notifications: Default::default(),
        };
        let hello = c.call_raw(method::HELLO, json!({ "token": ep.token, "client": me, "protocol": wire::SUPPORTED_PROTOCOLS }));
        let r = match hello {
            Ok(r) => r,
            Err(e) if e.code == ErrorCode::Internal && e.message.contains("closed") => return Err(not_running(app_id)),
            Err(e) => return Err(e),
        };
        c.server = r.get("server").cloned().and_then(|s| serde_json::from_value(s).ok()).unwrap_or_default();
        c.protocol = r.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32;
        if !wire::SUPPORTED_PROTOCOLS.contains(&c.protocol) {
            return Err(wire::version_mismatch(&[c.protocol]));
        }
        let _ = c.io.recv_timeout(&c.stream, Some(CALL_TIMEOUT));
        let _ = c.io.send_timeout(&c.stream, Some(CALL_TIMEOUT));
        Ok(c)
    }

    fn send(&mut self, method: &str, params: Value) -> Result<u64, LinkError> {
        self.next_id += 1;
        let id = self.next_id;
        wire::write_message(&mut ArcWrite(&self.stream, &self.io), &Message::request(id, method, params)).map_err(LinkError::from)?;
        Ok(id)
    }

    fn next_message(&mut self) -> Result<Message, LinkError> {
        match self.reader.read_message() {
            Ok(Some(m)) => Ok(m),
            Ok(None) => Err(LinkError::internal(format!("{} closed the connection", self.server.id))),
            Err(e) => Err(e),
        }
    }

    fn call_raw(&mut self, method: &str, params: Value) -> Result<Value, LinkError> {
        let id = self.send(method, params)?;
        loop {
            let m = self.next_message()?;
            match m.kind() {
                wire::Kind::Response if m.id == Some(id) => {
                    return match (m.result, m.error) {
                        (_, Some(e)) => Err(e),
                        (Some(r), None) => Ok(r),
                        _ => Err(LinkError::internal("empty response")),
                    };
                }
                wire::Kind::Notification => self.notifications.push_back(m),
                _ => {}
            }
        }
    }

    /// Calls `method` and waits for its response (notifications arriving
    /// meanwhile are kept for [`Client::next_notification`]).
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, LinkError> {
        self.call_raw(method, params)
    }

    /// The live action list.
    pub fn describe(&mut self) -> Result<Vec<Action>, LinkError> {
        let r = self.call(method::DESCRIBE, json!({}))?;
        serde_json::from_value(r.get("actions").cloned().unwrap_or(Value::Array(vec![]))).map_err(|e| LinkError::internal(e.to_string()))
    }

    pub fn describe_full(&mut self) -> Result<crate::app::Description, LinkError> {
        self.typed(method::DESCRIBE, json!({}))
    }
    fn typed<T: serde::de::DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<T, LinkError> {
        serde_json::from_value(self.call(method, params)?).map_err(|e| LinkError::internal(format!("invalid {method} result: {e}")))
    }
    pub fn settings(&mut self) -> Result<(), LinkError> {
        self.call(method::APP_SETTINGS, json!({})).map(|_| ())
    }
    pub fn restart(&mut self, mode: Option<&str>) -> Result<(), LinkError> {
        let params = mode.map_or_else(|| json!({}), |mode| json!({"mode": mode}));
        self.call(method::APP_RESTART, params).map(|_| ())
    }
    pub fn menu(&mut self) -> Result<Vec<crate::manifest::MenuItem>, LinkError> {
        let value = self.call(method::APP_MENU, json!({}))?;
        serde_json::from_value(value.get("items").cloned().ok_or_else(|| LinkError::internal("menu has no items"))?)
            .map_err(|e| LinkError::internal(e.to_string()))
    }
    pub fn menu_invoke(&mut self, id: &str) -> Result<(), LinkError> {
        self.call(method::APP_MENU_INVOKE, json!({"id": id})).map(|_| ())
    }
    pub fn shortcuts_set(&mut self, id: &str, accelerator: &str) -> Result<crate::app::ShortcutSetResult, LinkError> {
        self.typed(method::APP_SHORTCUTS_SET, json!({"id": id, "accelerator": accelerator}))
    }
    pub fn settings_export(&mut self) -> Result<Vec<Content>, LinkError> {
        let value = self.call(method::APP_SETTINGS_EXPORT, json!({}))?;
        serde_json::from_value(value.get("outputs").cloned().ok_or_else(|| LinkError::internal("export has no outputs"))?)
            .map_err(|e| LinkError::internal(e.to_string()))
    }
    pub fn settings_import(&mut self, inputs: &[Content]) -> Result<crate::app::ImportResult, LinkError> {
        self.typed(method::APP_SETTINGS_IMPORT, json!({"inputs": inputs}))
    }

    pub fn status(&mut self) -> Result<Value, LinkError> {
        self.call(method::APP_STATUS, json!({}))
    }

    pub fn subscribe(&mut self, topics: &[&str]) -> Result<(), LinkError> {
        self.call(method::SUBSCRIBE, json!({ "topics": topics })).map(|_| ())
    }

    /// Runs an action. If it starts a job, waits for `job.done`, passing
    /// progress to `on_progress`. Setting `cancel` sends `job.cancel`.
    pub fn invoke(
        &mut self,
        request: &InvokeRequest,
        on_progress: &mut dyn FnMut(&JobProgress),
        cancel: Option<&AtomicBool>,
    ) -> Result<InvokeResult, LinkError> {
        let r = self.call(method::INVOKE, serde_json::to_value(request).map_err(|e| LinkError::internal(e.to_string()))?)?;
        let Some(job) = r.get("job").and_then(Value::as_str).map(String::from) else {
            return serde_json::from_value(r).map_err(|e| LinkError::internal(e.to_string()));
        };
        self.wait_job(&job, on_progress, cancel)
    }

    fn wait_job(&mut self, job: &str, on_progress: &mut dyn FnMut(&JobProgress), cancel: Option<&AtomicBool>) -> Result<InvokeResult, LinkError> {
        // While a cancel flag is supplied, wake up every 100 ms to check it;
        // this only happens during an active job, never while idle.
        let _ = self.io.recv_timeout(&self.stream, cancel.map(|_| Duration::from_millis(100)));
        let mut cancel_sent = false;
        let result = loop {
            if let Some(flag) = cancel {
                if flag.load(Ordering::SeqCst) && !cancel_sent {
                    cancel_sent = true;
                    let _ = self.send(method::JOB_CANCEL, json!({ "job": job }));
                }
            }
            let m = match self.notifications.pop_front() {
                Some(m) => m,
                None => match self.next_message() {
                    Ok(m) => m,
                    Err(e) if e.code == ErrorCode::Timeout => continue,
                    Err(_) => break Err(LinkError::new(ErrorCode::NotRunning, format!("{} stopped while working", self.server.id))),
                },
            };
            let p = m.params();
            if p.get("job").and_then(Value::as_str) != Some(job) {
                if m.kind() == wire::Kind::Notification {
                    continue;
                }
                continue;
            }
            match m.method.as_deref() {
                Some(method::JOB_PROGRESS) => {
                    if let Ok(pr) = serde_json::from_value::<JobProgress>(p.clone()) {
                        on_progress(&pr);
                    }
                }
                Some(method::JOB_DONE) => {
                    break serde_json::from_value::<JobDone>(p.clone()).map_err(|e| LinkError::internal(e.to_string())).and_then(JobDone::into_result);
                }
                _ => {}
            }
        };
        let _ = self.io.recv_timeout(&self.stream, Some(CALL_TIMEOUT));
        result
    }

    /// Waits for the next notification (`None` waits forever).
    pub fn next_notification(&mut self, timeout: Option<Duration>) -> Result<Message, LinkError> {
        if let Some(m) = self.notifications.pop_front() {
            return Ok(m);
        }
        let _ = self.io.recv_timeout(&self.stream, timeout);
        let r = loop {
            match self.next_message() {
                Ok(m) if m.kind() == wire::Kind::Notification => break Ok(m),
                Ok(_) => continue,
                Err(e) => break Err(e),
            }
        };
        let _ = self.io.recv_timeout(&self.stream, Some(CALL_TIMEOUT));
        r
    }
}

/// Connects and says hello; `Some(server)` if the app is alive.
pub fn probe(locations: &Locations, app_id: &str, me: &PeerInfo) -> Option<PeerInfo> {
    Client::connect(locations, app_id, me).ok().map(|c| c.server)
}

/// What the Connected apps page shows for an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppState {
    Running { version: String },
    Installed { version: String },
    NotInstalled,
}

/// The state of `app_id`: running (endpoint answers), installed (manifest
/// with an existing executable), or not installed.
pub fn app_state(locations: &Locations, registry: &crate::registry::Registry, app_id: &str, me: &PeerInfo) -> AppState {
    let Some(m) = registry.get(app_id) else {
        return AppState::NotInstalled;
    };
    match probe(locations, app_id, me) {
        Some(p) => AppState::Running { version: if p.version.is_empty() { m.version.clone() } else { p.version } },
        None => AppState::Installed { version: m.version.clone() },
    }
}

/// Options for [`invoke_action`].
#[derive(Default)]
pub struct CallOptions<'a> {
    pub on_progress: Option<&'a mut dyn FnMut(&JobProgress)>,
    pub cancel: Option<&'a AtomicBool>,
    /// Called once when the app has to be started first (show a spinner
    /// after [`SPINNER_DELAY`]).
    pub on_launching: Option<&'a mut dyn FnMut()>,
}

/// The manifest action a request names (`action` + `#preset`, or `action`).
pub fn find_action<'m>(manifest: &'m Manifest, request: &InvokeRequest) -> Option<&'m Action> {
    if let Some(p) = &request.preset {
        let id = format!("{}#{p}", request.action);
        if let Some(a) = manifest.action(&id) {
            return Some(a);
        }
    }
    manifest.action(&request.action)
}

/// Runs `request` against `manifest`'s app, following the lifecycle in
/// SPEC §7: use the running instance; else a one-shot process for headless
/// actions; else start the app in the background and wait for it.
pub fn invoke_action(
    locations: &Locations,
    me: &PeerInfo,
    manifest: &Manifest,
    request: &InvokeRequest,
    mut opts: CallOptions<'_>,
) -> Result<InvokeResult, LinkError> {
    if !manifest.settings.link_enabled {
        return Err(LinkError::denied(crate::error::reason::DISABLED));
    }
    let action = find_action(manifest, request).ok_or_else(|| LinkError::unavailable(format!("{} has no action {}", manifest.name, request.action)))?;
    if !action.available {
        return Err(LinkError::unavailable(action.reason.clone().unwrap_or_default()));
    }
    let mut noop = |_: &JobProgress| {};
    let on_progress: &mut dyn FnMut(&JobProgress) = match opts.on_progress.take() {
        Some(f) => f,
        None => &mut noop,
    };
    if let Ok(mut c) = Client::connect(locations, &manifest.id, me) {
        return c.invoke(request, on_progress, opts.cancel);
    }
    if !action.interactive {
        if let Some(args) = &manifest.launch.invoke {
            return crate::oneshot::run(&manifest.executable, args, request, on_progress, opts.cancel);
        }
    }
    if let Some(f) = opts.on_launching.take() {
        f();
    }
    let mut c = launch_and_connect(locations, manifest, me)?;
    c.invoke(request, on_progress, opts.cancel)
}

/// Starts `manifest`'s app in the background and connects once its
/// endpoint answers (within [`LAUNCH_TIMEOUT`]).
pub fn launch_and_connect(locations: &Locations, manifest: &Manifest, me: &PeerInfo) -> Result<Client, LinkError> {
    if !manifest.executable_exists() {
        return Err(LinkError::new(ErrorCode::NotInstalled, format!("{} is not installed", manifest.name)));
    }
    spawn_detached(&manifest.executable, &manifest.launch.background)
        .map_err(|e| LinkError::new(ErrorCode::LaunchFailed, format!("could not start {}: {e}", manifest.name)))?;
    let deadline = Instant::now() + LAUNCH_TIMEOUT;
    // A bounded wait during a user-initiated launch (not idle polling).
    while Instant::now() < deadline {
        if let Ok(c) = Client::connect(locations, &manifest.id, me) {
            return Ok(c);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(LinkError::new(ErrorCode::LaunchFailed, format!("{} did not start within {} s", manifest.name, LAUNCH_TIMEOUT.as_secs())))
}

/// Starts a process that outlives the caller, with no inherited stdio.
pub fn spawn_detached(executable: &str, args: &[String]) -> std::io::Result<()> {
    let mut cmd = Command::new(executable);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        // Windows passes every inheritable handle to the child, our own stdio
        // included: a launched app would hold the caller's output pipe open
        // (e.g. `arcade-link invoke … | x` would wait until the app quits).
        // The child's stdio is set explicitly above, so ours needn't be.
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
        for handle in [std::io::stdin().as_raw_handle(), std::io::stdout().as_raw_handle(), std::io::stderr().as_raw_handle()] {
            if !handle.is_null() {
                // SAFETY: a std handle of this process (or an invalid one, which fails harmlessly).
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
            }
        }
    }
    let mut child = cmd.spawn()?;
    // Reap it in the background so it never lingers as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
