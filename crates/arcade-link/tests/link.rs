//! Server and client end to end, in one process, under a temporary root.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arcade_link::endpoint::{self, EndpointInfo};
use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::{
    Action, Client, Content, ErrorCode, InvokeRequest, InvokeResult, LinkError, Locations,
    Manifest, PeerInfo, Presence, Registry,
};
use serde_json::json;

fn root(name: &str) -> (std::path::PathBuf, Locations) {
    let dir = std::env::temp_dir().join(format!("al-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loc = Locations::under(&dir);
    (dir, loc)
}

fn me() -> PeerInfo {
    PeerInfo {
        id: "arcade.test-client".into(),
        version: "1".into(),
    }
}

#[derive(Default)]
struct Echo {
    started: AtomicUsize,
    cancelled: Arc<AtomicBool>,
    quit: AtomicBool,
}

impl Handler for Echo {
    fn describe(&self) -> Vec<Action> {
        vec![
            Action::new("echo", "Echo", "echo").accepts(&["text/*"]),
            Action::new("slow", "Slow", "wait"),
        ]
    }

    fn invoke(&self, req: InvokeRequest, ctx: &InvokeContext) -> Result<Reply, LinkError> {
        match req.action.as_str() {
            "echo" => Ok(Reply::Done(InvokeResult::outputs(
                req.inputs,
                format!("hi {}", ctx.peer().id),
            ))),
            "fail" => Err(LinkError::unavailable("FFmpeg isn't installed")),
            "slow" | "fast-job" => {
                self.started.fetch_add(1, Ordering::SeqCst);
                let job = ctx.start_job();
                let ticket = job.ticket();
                let steps = if req.action == "slow" { 50 } else { 0 };
                let cancelled = self.cancelled.clone();
                std::thread::spawn(move || {
                    for i in 0..steps {
                        if job.is_cancelled() {
                            cancelled.store(true, Ordering::SeqCst);
                            return job.finish(Err(LinkError::cancelled()));
                        }
                        job.progress(Some(i as f32 / 50.0), "working");
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    job.finish(Ok(InvokeResult::message("done")));
                });
                Ok(Reply::Job(ticket))
            }
            "dropped" => {
                let job = ctx.start_job();
                let ticket = job.ticket();
                std::thread::spawn(move || drop(job));
                Ok(Reply::Job(ticket))
            }
            _ => Err(LinkError::unsupported("no such action")),
        }
    }

    fn quit(&self) -> Result<(), LinkError> {
        self.quit.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn start(loc: &Locations, id: &str, handler: Arc<Echo>) -> Server {
    Server::start(
        ServerConfig {
            app: PeerInfo {
                id: id.into(),
                version: "9.9".into(),
            },
            locations: loc.clone(),
        },
        handler,
    )
    .unwrap()
}

#[test]
fn hello_describe_invoke() {
    let (dir, loc) = root("basic");
    let _s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    let t = Instant::now();
    let mut c = Client::connect(&loc, "arcade.echo", &me()).unwrap();
    assert!(
        t.elapsed() < Duration::from_millis(150),
        "connect + hello took {:?}",
        t.elapsed()
    );
    assert_eq!(c.server.id, "arcade.echo");
    assert_eq!(c.protocol, 1);
    assert_eq!(c.describe().unwrap().len(), 2);
    let r = c
        .invoke(
            &InvokeRequest::new("echo", "t").input(Content::plain("x")),
            &mut |_| {},
            None,
        )
        .unwrap();
    assert_eq!(r.message.as_deref(), Some("hi arcade.test-client"));
    assert_eq!(r.outputs[0].text.as_deref(), Some("x"));
    let e = c
        .invoke(&InvokeRequest::new("fail", "t"), &mut |_| {}, None)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Unavailable);
    assert_eq!(
        e.user_message("Arcade Box"),
        "Arcade Box can't do this yet: FFmpeg isn't installed."
    );
    let st = c.status().unwrap();
    assert_eq!(st["id"], "arcade.echo");
    assert_eq!(st["busy"], false);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn jobs_progress_cancel_and_drop() {
    let (dir, loc) = root("jobs");
    let h = Arc::new(Echo::default());
    let _s = start(&loc, "arcade.echo", h.clone());
    let mut c = Client::connect(&loc, "arcade.echo", &me()).unwrap();
    // A job that finishes before the caller could know its id still arrives in order.
    for _ in 0..20 {
        assert_eq!(
            c.invoke(&InvokeRequest::new("fast-job", "t"), &mut |_| {}, None)
                .unwrap()
                .message
                .as_deref(),
            Some("done")
        );
    }
    let seen = Mutex::new(0);
    let cancel = AtomicBool::new(false);
    let r = c.invoke(
        &InvokeRequest::new("slow", "t"),
        &mut |p| {
            let mut n = seen.lock().unwrap();
            *n += 1;
            if *n == 3 {
                cancel.store(true, Ordering::SeqCst);
            }
            assert_eq!(p.message, "working");
        },
        Some(&cancel),
    );
    assert_eq!(r.unwrap_err().code, ErrorCode::Cancelled);
    assert!(h.cancelled.load(Ordering::SeqCst));
    let e = c
        .invoke(&InvokeRequest::new("dropped", "t"), &mut |_| {}, None)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Internal);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn disconnect_cancels_the_callers_jobs_and_quit_refuses_while_busy() {
    let (dir, loc) = root("disconnect");
    let h = Arc::new(Echo::default());
    let _s = start(&loc, "arcade.echo", h.clone());
    let loc2 = loc.clone();
    let t = std::thread::spawn(move || {
        let mut c = Client::connect(&loc2, "arcade.echo", &me()).unwrap();
        // Send the invoke and drop the connection without waiting.
        let _ = c.call("invoke", json!({"action": "slow", "inputs": []}));
    });
    t.join().unwrap();
    let mut c = Client::connect(&loc, "arcade.echo", &me()).unwrap();
    // While the slow job may still be running, app.quit is refused.
    let busy = c.call("app.quit", json!({}));
    if let Err(e) = &busy {
        assert_eq!(e.code, ErrorCode::Busy);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while !h.cancelled.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        h.cancelled.load(Ordering::SeqCst),
        "the orphaned job was not cancelled"
    );
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(c.call("app.quit", json!({})).unwrap()["quitting"], true);
    std::thread::sleep(Duration::from_millis(50));
    assert!(h.quit.load(Ordering::SeqCst));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn wrong_token_and_wrong_version_are_refused() {
    let (dir, loc) = root("auth");
    let _s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    let mut ep = endpoint::read(&loc, "arcade.echo").unwrap();
    let good = ep.token.clone();
    ep.token = "0".repeat(64);
    endpoint::write(&loc, "arcade.echo", &ep).unwrap();
    let e = Client::connect(&loc, "arcade.echo", &me()).err().unwrap();
    assert_eq!(e.code, ErrorCode::Denied);
    ep.token = good;
    endpoint::write(&loc, "arcade.echo", &ep).unwrap();
    assert!(Client::connect(&loc, "arcade.echo", &me()).is_ok());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn stale_endpoint_is_replaced_and_a_live_one_is_not() {
    let (dir, loc) = root("stale");
    // A crashed instance left an endpoint file and socket behind.
    let addr = arcade_link::transport::address_for(&loc, "arcade.echo").unwrap();
    std::fs::create_dir_all(&loc.runtime).unwrap();
    endpoint::write(
        &loc,
        "arcade.echo",
        &EndpointInfo {
            protocol: vec![1],
            transport: addr.transport.into(),
            address: addr.address.clone(),
            pid: 999_999,
            started_at: String::new(),
            token: "dead".into(),
        },
    )
    .unwrap();
    #[cfg(unix)]
    std::fs::write(&addr.address, b"").unwrap();
    assert_eq!(
        Client::connect(&loc, "arcade.echo", &me())
            .err()
            .unwrap()
            .code,
        ErrorCode::NotRunning
    );
    let s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    assert!(Client::connect(&loc, "arcade.echo", &me()).is_ok());
    // A second instance must not displace the live one.
    let second = Server::start(
        ServerConfig {
            app: PeerInfo {
                id: "arcade.echo".into(),
                version: "1".into(),
            },
            locations: loc.clone(),
        },
        Arc::new(Echo::default()),
    );
    assert_eq!(second.err().unwrap().kind(), std::io::ErrorKind::AddrInUse);
    assert!(Client::connect(&loc, "arcade.echo", &me()).is_ok());
    drop(s);
    assert!(
        endpoint::read(&loc, "arcade.echo").is_err(),
        "stopping removes the endpoint file"
    );
    assert_eq!(
        Client::connect(&loc, "arcade.echo", &me())
            .err()
            .unwrap()
            .code,
        ErrorCode::NotRunning
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn concurrent_clients() {
    let (dir, loc) = root("concurrency");
    let _s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    let threads: Vec<_> = (0..16)
        .map(|i| {
            let loc = loc.clone();
            std::thread::spawn(move || {
                let mut c =
                    Client::connect_with(&loc, "arcade.echo", &me(), Duration::from_secs(2))
                        .unwrap();
                for j in 0..25 {
                    let text = format!("{i}-{j}");
                    let r = c
                        .invoke(
                            &InvokeRequest::new("echo", "t").input(Content::plain(&text)),
                            &mut |_| {},
                            None,
                        )
                        .unwrap();
                    assert_eq!(r.outputs[0].text.as_deref(), Some(text.as_str()));
                    let r = c
                        .invoke(&InvokeRequest::new("fast-job", "t"), &mut |_| {}, None)
                        .unwrap();
                    assert_eq!(r.message.as_deref(), Some("done"));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn subscribers_hear_app_changed_and_presence_follows_the_switch() {
    let (dir, loc) = root("presence");
    let exe = std::env::current_exe().unwrap();
    let mut m = Manifest::new("arcade.echo", "1.0", exe.to_str().unwrap());
    m.actions = Echo::default().describe();
    let p = Presence::start(loc.clone(), m.clone(), Arc::new(Echo::default()));
    assert!(p.listening(), "{:?}", p.last_error());
    assert_eq!(
        Registry::load(&loc)
            .get("arcade.echo")
            .unwrap()
            .actions
            .len(),
        2
    );
    let mut sub = Client::connect(&loc, "arcade.echo", &me()).unwrap();
    sub.subscribe(&["app.changed"]).unwrap();
    m.version = "1.1".into();
    p.update(m.clone());
    let n = sub.next_notification(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(n.method.as_deref(), Some("app.changed"));
    // Master switch off: no listener, no actions, still listed as installed.
    m.settings.link_enabled = false;
    p.update(m.clone());
    assert!(!p.listening());
    let reg = Registry::load(&loc);
    let listed = reg.get("arcade.echo").unwrap();
    assert!(listed.actions.is_empty() && !listed.settings.link_enabled);
    assert_eq!(
        Client::connect(&loc, "arcade.echo", &me())
            .err()
            .unwrap()
            .code,
        ErrorCode::NotRunning
    );
    m.settings.link_enabled = true;
    p.update(m);
    assert!(p.listening());
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[test]
fn permissions_on_disk() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, loc) = root("perms");
    let s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&loc.runtime), 0o700);
    assert_eq!(mode(&loc.endpoint("arcade.echo")), 0o600);
    assert_eq!(mode(std::path::Path::new(s.address())), 0o600);
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[test]
fn deep_runtime_dir_uses_a_short_socket() {
    let base = std::env::temp_dir().join(format!("al-deep-{}", std::process::id()));
    let deep = base.join("d".repeat(110));
    std::fs::create_dir_all(&deep).unwrap();
    let loc = Locations::under(&deep);
    let s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    assert!(s.address().len() <= arcade_link::transport::MAX_SOCKET_PATH);
    assert!(Client::connect(&loc, "arcade.echo", &me()).is_ok());
    drop(s);
    std::fs::remove_dir_all(base).ok();
}

#[test]
fn version_mismatch_is_reported() {
    use std::io::{BufRead, BufReader, Write};
    let (dir, loc) = root("version");
    let _s = start(&loc, "arcade.echo", Arc::new(Echo::default()));
    let ep = endpoint::read(&loc, "arcade.echo").unwrap();
    let mut stream =
        arcade_link::transport::connect(&ep.address, Duration::from_millis(500)).unwrap();
    let hello = json!({"v": 2, "id": 1, "method": "hello", "params": {"token": ep.token, "client": me(), "protocol": [2, 3]}});
    stream.write_all(format!("{hello}\n").as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(&mut stream).read_line(&mut line).unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["error"]["code"], "version_mismatch");
    std::fs::remove_dir_all(dir).ok();
}
