//! The resident side: listen on the endpoint and serve requests (SPEC §4).
//!
//! One accept thread blocks in `accept` (no timers, no polling); each
//! connection gets its own thread. Long work runs as a [`Job`] whose
//! progress and result are sent as notifications on the caller's connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::Duration;

use interprocess::local_socket::{prelude::*, Stream};
use serde_json::{json, Value};

use crate::endpoint::{self, EndpointInfo};
use crate::error::{reason, ErrorCode, LinkError};
use crate::manifest::{self, Action};
use crate::paths::{self, Locations};
use crate::transport;
use crate::wire::{self, method, InvokeRequest, InvokeResult, JobDone, JobProgress, LineReader, Message, PeerInfo};

/// How long a new connection may take to say `hello`.
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);

/// What an app implements to serve the Link.
pub trait Handler: Send + Sync + 'static {
    /// The live action list (same shape as the manifest's `actions`).
    fn describe(&self) -> Vec<Action>;

    /// Runs an action: return a result directly, or start a job with
    /// [`InvokeContext::start_job`], move the [`Job`] to the worker that
    /// finishes it, and return its [`Job::ticket`].
    fn invoke(&self, request: InvokeRequest, ctx: &InvokeContext) -> Result<Reply, LinkError>;

    /// Extra fields for `app.status`.
    fn status(&self) -> Value {
        Value::Null
    }

    /// Brings the app's main window forward.
    fn activate(&self) -> Result<(), LinkError> {
        Err(LinkError::unavailable("this app has no window to show"))
    }

    /// Quits the app. Called after the `app.quit` response is sent.
    fn quit(&self) -> Result<(), LinkError> {
        Err(LinkError::unavailable("this app can't be quit over the Link"))
    }
}

/// An action's immediate answer.
pub enum Reply {
    Done(InvokeResult),
    Job(JobTicket),
}

/// Identifies a started job in [`Reply::Job`].
pub struct JobTicket {
    id: String,
    sink: Arc<dyn Sink>,
    gate: Arc<Gate>,
}

impl JobTicket {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn open(&self) {
        self.gate.open(&*self.sink);
    }
}

/// Where a job's messages go: a connection, or stdout in one-shot mode.
pub(crate) trait Sink: Send + Sync {
    /// Returns false once the receiver is gone.
    fn send(&self, m: &Message) -> bool;
}

/// Job messages wait here until the `{"job": id}` response is on the wire,
/// so a fast job can never report before the caller knows its id.
struct Gate {
    queue: Mutex<Option<Vec<Message>>>,
}

impl Gate {
    fn closed() -> Arc<Gate> {
        Arc::new(Gate { queue: Mutex::new(Some(Vec::new())) })
    }

    fn send(&self, sink: &dyn Sink, m: Message) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        match q.as_mut() {
            Some(pending) => pending.push(m),
            None => {
                sink.send(&m);
            }
        }
    }

    fn open(&self, sink: &dyn Sink) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pending) = q.take() {
            for m in pending {
                sink.send(&m);
            }
        }
    }
}

type CancelHandler = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub(crate) struct Jobs {
    next: AtomicU64,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    on_cancel: Mutex<HashMap<String, CancelHandler>>,
    idle: Condvar,
}

impl Jobs {
    fn count(&self) -> usize {
        self.running.lock().map(|m| m.len()).unwrap_or(0)
    }

    fn cancel(&self, id: &str) -> bool {
        match self.running.lock().ok().and_then(|m| m.get(id).cloned()) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                let handler = self.on_cancel.lock().ok().and_then(|mut h| h.remove(id));
                if let Some(h) = handler {
                    h();
                }
                true
            }
            None => false,
        }
    }

    fn cancel_all(&self) {
        let ids: Vec<String> = self.running.lock().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        for id in ids {
            self.cancel(&id);
        }
    }

    fn finish(&self, id: &str) {
        if let Ok(mut h) = self.on_cancel.lock() {
            h.remove(id);
        }
        if let Ok(mut m) = self.running.lock() {
            m.remove(id);
            if m.is_empty() {
                self.idle.notify_all();
            }
        }
    }
}

/// Passed to [`Handler::invoke`].
pub struct InvokeContext {
    pub(crate) sink: Arc<dyn Sink>,
    pub(crate) jobs: Arc<Jobs>,
    pub(crate) peer: PeerInfo,
    pub(crate) started: Mutex<Vec<String>>,
}

impl InvokeContext {
    /// The calling app (`arcade.lens`, …); empty in one-shot mode.
    pub fn peer(&self) -> &PeerInfo {
        &self.peer
    }

    /// Starts a job. Move it to wherever the work happens and call
    /// [`Job::finish`] when done.
    pub fn start_job(&self) -> Job {
        let n = self.jobs.next.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("j-{n}");
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut m) = self.jobs.running.lock() {
            m.insert(id.clone(), cancel.clone());
        }
        if let Ok(mut s) = self.started.lock() {
            s.push(id.clone());
        }
        Job { id, sink: self.sink.clone(), cancel, gate: Gate::closed(), jobs: Arc::downgrade(&self.jobs), finished: false }
    }
}

/// A long-running action. Dropping it without [`Job::finish`] reports an
/// internal error, so a caller never waits forever.
pub struct Job {
    id: String,
    sink: Arc<dyn Sink>,
    cancel: Arc<AtomicBool>,
    gate: Arc<Gate>,
    jobs: Weak<Jobs>,
    finished: bool,
}

impl Job {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Set by `job.cancel` or when the caller disconnects.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The cancellation flag, for code that already takes an `&AtomicBool`.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Runs `f` once if the job is cancelled (by `job.cancel`, the caller
    /// disconnecting, or the server stopping), for work that must be told
    /// rather than polled. Runs it now if the job is already cancelled.
    pub fn on_cancel(&self, f: impl FnOnce() + Send + 'static) {
        if self.is_cancelled() {
            return f();
        }
        if let Some(jobs) = self.jobs.upgrade() {
            if let Ok(mut h) = jobs.on_cancel.lock() {
                h.insert(self.id.clone(), Box::new(f));
            }
            // Cancelled between the check and the insert: run it now.
            if self.is_cancelled() {
                if let Some(h) = jobs.on_cancel.lock().ok().and_then(|mut h| h.remove(&self.id)) {
                    h();
                }
            }
        }
    }

    pub fn progress(&self, fraction: Option<f32>, message: &str) {
        let p = JobProgress { job: self.id.clone(), fraction: fraction.map(|f| f.clamp(0.0, 1.0)), message: message.into() };
        self.gate.send(&*self.sink, Message::notification(method::JOB_PROGRESS, serde_json::to_value(p).unwrap_or_default()));
    }

    /// Reports the outcome. A cancelled job reports `cancelled` whatever `result` says.
    pub fn finish(mut self, result: Result<InvokeResult, LinkError>) {
        self.send_done(result);
    }

    fn send_done(&mut self, result: Result<InvokeResult, LinkError>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let done = match result {
            _ if self.is_cancelled() => {
                JobDone { job: self.id.clone(), status: "cancelled".into(), result: InvokeResult::default(), error: Some(LinkError::cancelled()) }
            }
            Ok(r) => JobDone { job: self.id.clone(), status: "success".into(), result: r, error: None },
            Err(e) if e.code == ErrorCode::Cancelled => {
                JobDone { job: self.id.clone(), status: "cancelled".into(), result: InvokeResult::default(), error: Some(e) }
            }
            Err(e) => JobDone { job: self.id.clone(), status: "error".into(), result: InvokeResult::message(e.message.clone()), error: Some(e) },
        };
        self.gate.send(&*self.sink, Message::notification(method::JOB_DONE, serde_json::to_value(done).unwrap_or_default()));
        if let Some(jobs) = self.jobs.upgrade() {
            jobs.finish(&self.id);
        }
    }

    /// What [`Handler::invoke`] returns for this job.
    pub fn ticket(&self) -> JobTicket {
        JobTicket { id: self.id.clone(), sink: self.sink.clone(), gate: self.gate.clone() }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if !self.finished {
            self.send_done(Err(LinkError::internal("the job ended without a result")));
        }
    }
}

/// Serializes writes to one connection.
struct Conn {
    stream: Arc<Stream>,
    write: Mutex<()>,
    alive: AtomicBool,
    topics: Mutex<Vec<String>>,
}

impl Sink for Conn {
    fn send(&self, m: &Message) -> bool {
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        let _g = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let ok = wire::write_message(&mut &*self.stream, m).is_ok();
        if !ok {
            self.alive.store(false, Ordering::SeqCst);
        }
        ok
    }
}

struct ArcRead(Arc<Stream>);

impl std::io::Read for ArcRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

/// Identity of the serving app.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub app: PeerInfo,
    pub locations: Locations,
}

struct Inner {
    config: ServerConfig,
    handler: Arc<dyn Handler>,
    token: String,
    address: String,
    jobs: Arc<Jobs>,
    conns: Mutex<Vec<Weak<Conn>>>,
    stopping: AtomicBool,
}

/// A running Link server. Dropping it stops listening and removes the endpoint file.
pub struct Server {
    inner: Arc<Inner>,
    accept: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Server {
    /// Binds the endpoint and starts the accept thread.
    ///
    /// Fails with `AddrInUse` if a live instance of the same app already
    /// answers on its endpoint. A stale endpoint (dead process) is replaced.
    pub fn start(config: ServerConfig, handler: Arc<dyn Handler>) -> std::io::Result<Server> {
        let loc = &config.locations;
        paths::ensure_private_dir(&loc.runtime)?;
        if crate::client::probe(loc, &config.app.id, &config.app).is_some() {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, format!("{} is already serving the Link", config.app.id)));
        }
        let addr = transport::address_for(loc, &config.app.id)?;
        #[cfg(unix)]
        let _ = std::fs::remove_file(&addr.address);
        let listener = transport::listen(&addr.address)?;
        let token = endpoint::new_token()?;
        endpoint::write(
            loc,
            &config.app.id,
            &EndpointInfo {
                protocol: wire::SUPPORTED_PROTOCOLS.to_vec(),
                transport: addr.transport.into(),
                address: addr.address.clone(),
                pid: std::process::id(),
                started_at: manifest::now_rfc3339(),
                token: token.clone(),
            },
        )?;
        let inner = Arc::new(Inner {
            config,
            handler,
            token,
            address: addr.address,
            jobs: Arc::new(Jobs::default()),
            conns: Mutex::new(Vec::new()),
            stopping: AtomicBool::new(false),
        });
        let accept = inner.clone();
        let handle = std::thread::Builder::new().name("arcade-link-accept".into()).spawn(move || {
            for conn in listener.incoming() {
                if accept.stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = conn else { continue };
                let inner = accept.clone();
                let _ = std::thread::Builder::new().name("arcade-link-conn".into()).spawn(move || serve(inner, stream));
            }
        })?;
        Ok(Server { inner, accept: Mutex::new(Some(handle)) })
    }

    /// Tells subscribers that this app's actions or availability changed.
    pub fn notify_changed(&self) {
        let m = Message::notification(method::APP_CHANGED, json!({ "app": self.inner.config.app.id }));
        for c in self.subscribers(method::APP_CHANGED) {
            c.send(&m);
        }
    }

    fn subscribers(&self, topic: &str) -> Vec<Arc<Conn>> {
        let mut conns = self.inner.conns.lock().unwrap_or_else(|e| e.into_inner());
        conns.retain(|w| w.strong_count() > 0);
        conns.iter().filter_map(Weak::upgrade).filter(|c| c.topics.lock().map(|t| t.iter().any(|x| topic_matches(x, topic))).unwrap_or(false)).collect()
    }

    /// Jobs are running (an update should wait).
    pub fn busy(&self) -> bool {
        self.inner.jobs.count() > 0
    }

    pub fn address(&self) -> &str {
        &self.inner.address
    }

    /// Stops accepting, removes the endpoint file and cancels running jobs.
    pub fn stop(&self) {
        if self.inner.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        endpoint::remove_if_ours(&self.inner.config.locations, &self.inner.config.app.id, &self.inner.token);
        self.inner.jobs.cancel_all();
        // Close live connections too: switched off means no longer served,
        // and on Windows their pipe instances would block the next listener.
        // Aborting a connection's pending read ends its thread, which closes
        // the pipe; repeat until all are gone, as one may be between reads.
        #[cfg(windows)]
        {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            loop {
                let live: Vec<Arc<Conn>> = self.inner.conns.lock().unwrap_or_else(|e| e.into_inner()).iter().filter_map(Weak::upgrade).collect();
                if live.is_empty() || std::time::Instant::now() >= deadline {
                    break;
                }
                for conn in &live {
                    transport::cancel_io(&conn.stream);
                }
                drop(live);
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        // Wake the accept thread so it sees `stopping` and drops the listener,
        // and wait (briefly) until it has: a Windows pipe name can't be served
        // again while the old listener still holds it.
        let handle = self.accept.lock().unwrap_or_else(|e| e.into_inner()).take();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while let Some(h) = &handle {
            if h.is_finished() || std::time::Instant::now() >= deadline {
                break;
            }
            let _ = transport::connect(&self.inner.address, Duration::from_millis(100));
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(h) = handle.filter(|h| h.is_finished()) {
            let _ = h.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn topic_matches(subscribed: &str, topic: &str) -> bool {
    subscribed == topic || subscribed.strip_suffix('*').is_some_and(|p| topic.starts_with(p))
}

fn serve(inner: Arc<Inner>, stream: Stream) {
    let stream = Arc::new(stream);
    let conn = Arc::new(Conn { stream: stream.clone(), write: Mutex::new(()), alive: AtomicBool::new(true), topics: Mutex::new(Vec::new()) });
    let mut reader = LineReader::new(ArcRead(stream.clone()));
    let _ = stream.set_recv_timeout(Some(HELLO_TIMEOUT));
    let peer = match handshake(&inner, &conn, &mut reader) {
        Some(p) => p,
        None => return,
    };
    let _ = stream.set_recv_timeout(None);
    if let Ok(mut conns) = inner.conns.lock() {
        conns.push(Arc::downgrade(&conn));
    }
    let mut mine: Vec<String> = Vec::new();
    while let Ok(Some(m)) = reader.read_message() {
        if inner.stopping.load(Ordering::SeqCst) {
            break;
        }
        let Some(id) = m.id else { continue };
        let method = m.method.clone().unwrap_or_default();
        let reply = match method.as_str() {
            method::DESCRIBE => Ok(json!({ "actions": inner.handler.describe() })),
            method::INVOKE => {
                mine.extend(invoke(&inner, &conn, &peer, &m, id));
                continue;
            }
            method::JOB_CANCEL => {
                let job = m.params().get("job").and_then(Value::as_str).unwrap_or_default();
                Ok(json!({ "cancelled": inner.jobs.cancel(job) }))
            }
            method::SUBSCRIBE => {
                let topics: Vec<String> = m
                    .params()
                    .get("topics")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if let Ok(mut t) = conn.topics.lock() {
                    t.clone_from(&topics);
                }
                Ok(json!({ "topics": topics }))
            }
            method::APP_STATUS => Ok(json!({
                "id": inner.config.app.id,
                "version": inner.config.app.version,
                "pid": std::process::id(),
                "protocol": wire::PROTOCOL_VERSION,
                "busy": inner.jobs.count() > 0,
                "jobs": inner.jobs.count(),
                "status": inner.handler.status(),
            })),
            method::APP_ACTIVATE => inner.handler.activate().map(|_| json!({ "activated": true })),
            method::APP_QUIT => {
                let force = m.params().get("force").and_then(Value::as_bool).unwrap_or(false);
                if inner.jobs.count() > 0 && !force {
                    Err(LinkError::busy())
                } else {
                    conn.send(&Message::response(id, Ok(json!({ "quitting": true }))));
                    let _ = inner.handler.quit();
                    continue;
                }
            }
            method::HELLO => Err(LinkError::internal("hello was already sent")),
            other => Err(LinkError::internal(format!("unknown method {other:?}"))),
        };
        conn.send(&Message::response(id, reply));
    }
    conn.alive.store(false, Ordering::SeqCst);
    // Nobody is left to receive these jobs' results.
    for job in mine {
        inner.jobs.cancel(&job);
    }
}

fn handshake(inner: &Inner, conn: &Conn, reader: &mut LineReader<ArcRead>) -> Option<PeerInfo> {
    let m = reader.read_message().ok()??;
    let id = m.id?;
    if m.method.as_deref() != Some(method::HELLO) {
        conn.send(&Message::response(id, Err(LinkError::denied(reason::TOKEN).with_reason("hello must come first"))));
        return None;
    }
    let p = m.params();
    let token = p.get("token").and_then(Value::as_str).unwrap_or_default();
    if !endpoint::token_eq(token, &inner.token) {
        conn.send(&Message::response(id, Err(LinkError::denied(reason::TOKEN))));
        return None;
    }
    let theirs: Vec<u32> =
        p.get("protocol").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_u64().map(|x| x as u32)).collect()).unwrap_or_default();
    let Some(version) = wire::negotiate(&theirs, wire::SUPPORTED_PROTOCOLS) else {
        conn.send(&Message::response(id, Err(wire::version_mismatch(&theirs))));
        return None;
    };
    let peer: PeerInfo = p.get("client").cloned().and_then(|c| serde_json::from_value(c).ok()).unwrap_or_default();
    conn.send(&Message::response(id, Ok(json!({ "server": inner.config.app, "protocol": version }))));
    Some(peer)
}

/// Runs `invoke`; returns the ids of jobs it started.
fn invoke(inner: &Inner, conn: &Arc<Conn>, peer: &PeerInfo, m: &Message, id: u64) -> Vec<String> {
    let request: InvokeRequest = match serde_json::from_value(m.params().clone()) {
        Ok(r) => r,
        Err(e) => {
            conn.send(&Message::response(id, Err(LinkError::unsupported(format!("invalid invoke: {e}")))));
            return Vec::new();
        }
    };
    let ctx = InvokeContext { sink: conn.clone(), jobs: inner.jobs.clone(), peer: peer.clone(), started: Mutex::new(Vec::new()) };
    let outcome = inner.handler.invoke(request, &ctx);
    let started = ctx.started.into_inner().unwrap_or_default();
    match outcome {
        Ok(Reply::Done(result)) => {
            conn.send(&Message::response(id, Ok(serde_json::to_value(result).unwrap_or_default())));
        }
        Ok(Reply::Job(ticket)) => {
            conn.send(&Message::response(id, Ok(json!({ "job": ticket.id() }))));
            ticket.open();
        }
        Err(e) => {
            conn.send(&Message::response(id, Err(e)));
        }
    }
    started
}
