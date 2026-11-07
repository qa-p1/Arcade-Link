//! Another user's process is dropped on accept, even if the socket's folder
//! permissions were loosened. Needs root to act as another user: CI runs it
//! with sudo and `ARCADE_REQUIRE_ROOT_TESTS=1`; elsewhere it skips.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::{endpoint, Action, InvokeRequest, LinkError, Locations, PeerInfo};

const CHILD: &str = "ARCADE_PEER_UID_SOCKET";

struct Nothing;

impl Handler for Nothing {
    fn describe(&self) -> Vec<Action> {
        Vec::new()
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        Err(LinkError::unsupported("none"))
    }
}

/// Connects, sends a hello, and prints how many reply bytes arrived.
fn hello_reply_len(socket: &str) -> usize {
    let mut s = UnixStream::connect(socket).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(b"{\"v\":1,\"id\":1,\"method\":\"hello\",\"params\":{\"token\":\"wrong\",\"protocol\":[1]}}\n").unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap_or(0)
}

#[test]
fn peer_child() {
    if let Ok(socket) = std::env::var(CHILD) {
        println!("REPLY:{}", hello_reply_len(&socket));
    }
}

fn open_up(p: &Path) {
    let mut p = Some(p);
    while let Some(dir) = p {
        if dir == Path::new("/") || dir == Path::new("/tmp") {
            break;
        }
        let mode = if dir.is_dir() { 0o777 } else { 0o666 };
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
        p = dir.parent();
    }
}

#[test]
fn another_users_connection_is_dropped_before_hello() {
    if unsafe { libc::geteuid() } != 0 {
        assert!(std::env::var_os("ARCADE_REQUIRE_ROOT_TESTS").is_none(), "this test must run as root in CI");
        eprintln!("not root; skipping");
        return;
    }
    let dir = Path::new("/tmp").join(format!("al-peer-uid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loc = Locations::under(&dir);
    let _server =
        Server::start(ServerConfig { app: PeerInfo { id: "arcade.peer".into(), version: "1".into() }, locations: loc.clone() }, Arc::new(Nothing)).unwrap();
    let address = endpoint::read(&loc, "arcade.peer").unwrap().address;
    open_up(Path::new(&address));

    // The same user gets an answer (a refusal, since the token is wrong)...
    assert!(hello_reply_len(&address) > 0);

    // ...another user gets nothing.
    // The build directory may be closed to other users (it is on CI runners).
    let exe = dir.join("peer-test");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out =
        Command::new(&exe).args(["--exact", "peer_child", "--nocapture", "--test-threads=1"]).env(CHILD, &address).uid(65534).gid(65534).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("REPLY:0"), "child output: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
