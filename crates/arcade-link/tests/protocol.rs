//! Protocol negotiation in `hello`, including peers that send no version.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::{endpoint, Action, InvokeRequest, LinkError, Locations, PeerInfo};
use serde_json::{json, Value};

struct Nothing;

impl Handler for Nothing {
    fn describe(&self) -> Vec<Action> {
        Vec::new()
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        Err(LinkError::unsupported("none"))
    }
}

fn hello(name: &str, params: impl Fn(&str) -> Value) -> Value {
    let dir = std::env::temp_dir().join(format!("al-proto-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loc = Locations::under(&dir);
    let _server =
        Server::start(ServerConfig { app: PeerInfo { id: "arcade.proto".into(), version: "1".into() }, locations: loc.clone() }, Arc::new(Nothing)).unwrap();
    let ep = endpoint::read(&loc, "arcade.proto").unwrap();
    let mut s = UnixStream::connect(&ep.address).unwrap();
    let msg = json!({"v": 1, "id": 1, "method": "hello", "params": params(&ep.token)});
    s.write_all(format!("{msg}\n").as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn a_hello_without_a_protocol_is_the_legacy_protocol_1() {
    let r = hello("legacy", |t| json!({"token": t, "client": {"id": "arcade.old", "version": "0.4.0"}}));
    assert_eq!(r["result"]["protocol"], 1, "{r}");
}

#[test]
fn an_unknown_major_is_refused_with_a_clear_error() {
    let r = hello("future", |t| json!({"token": t, "protocol": [2]}));
    assert_eq!(r["error"]["code"], "version_mismatch", "{r}");
    assert!(r["error"]["message"].as_str().unwrap().contains("no common protocol version"), "{r}");
}

#[test]
fn the_newest_common_version_is_chosen() {
    let r = hello("common", |t| json!({"token": t, "protocol": [1, 2]}));
    assert_eq!(r["result"]["protocol"], 1, "{r}");
}
