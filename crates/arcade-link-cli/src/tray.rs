use arcade_link::{
    manifest::{self, ManifestDocument, TrayHostSettings},
    server::{Handler, InvokeContext, Reply, Server, ServerConfig},
    trayhost::{TrayHostServer, TrayHostWatcher, WatcherConfig},
    Action, InvokeRequest, LinkError, Locations, Manifest, PeerInfo,
};
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};

struct Host;
impl Handler for Host {
    fn describe(&self) -> Vec<Action> {
        Vec::new()
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        Err(LinkError::unsupported("no actions"))
    }
    fn status(&self) -> serde_json::Value {
        serde_json::json!({"tray":"own"})
    }
}
pub fn run(loc: &Locations, args: &[String]) -> Result<(), String> {
    if args.get(1).is_some_and(|s| s == "host") {
        let host = TrayHostServer::new(true);
        let server = Server::start(
            ServerConfig { locations: loc.clone(), app: PeerInfo { id: "arcade.tools".into(), version: env!("CARGO_PKG_VERSION").into() } },
            Arc::new(Host),
        )
        .map_err(|e| e.to_string())?;
        server.attach_tray_host(host.clone());
        let mut document: ManifestDocument =
            Manifest::new("arcade.tools", env!("CARGO_PKG_VERSION"), &std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy()).into();
        document.manifest.launch.background = vec!["tray".into(), "host".into()];
        document.additions.tray_host = Some(TrayHostSettings { enabled: true, excluded: Vec::new() });
        manifest::write_document(loc, &document).map_err(|e| e.to_string())?;
        eprintln!("tray host ready: hosted true|false, restarting, exclude <ids>, include <id>, quit, crash");
        commands(loc, &host, &mut document)?;
        host.shutdown();
        server.stop();
        return Ok(());
    }
    let id = super::app_id(args.get(1).ok_or("tray needs an app or host")?);
    let watcher = TrayHostWatcher::start(
        WatcherConfig {
            locations: loc.clone(),
            app: PeerInfo { id, version: env!("CARGO_PKG_VERSION").into() },
            link_enabled: !super::flag(args, "--link-off"),
            has_tray: true,
        },
        |state| {
            println!("{}", state.as_str());
            let _ = io::stdout().flush();
        },
    )
    .map_err(|e| e.to_string())?;
    // Commands let tests change Link without a separate control socket.
    for line in io::stdin().lock().lines() {
        match line.map_err(|e| e.to_string())?.trim() {
            "link off" => watcher.set_link_enabled(false),
            "link on" => watcher.set_link_enabled(true),
            "quit" => break,
            _ => {}
        }
    }
    watcher.stop();
    Ok(())
}
pub(crate) fn commands(loc: &Locations, host: &TrayHostServer, doc: &mut ManifestDocument) -> Result<(), String> {
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["hosted", value @ ("true" | "false")] => {
                host.set_hosted(*value == "true");
                doc.additions.tray_host.as_mut().unwrap().enabled = *value == "true";
            }
            ["restarting"] | ["restarting", "true"] => {
                host.announce_restarting();
                continue;
            }
            ["exclude", ids @ ..] => {
                let ids: Vec<String> = ids.iter().map(|s| super::app_id(s)).collect();
                host.set_excluded(&ids);
                doc.additions.tray_host.as_mut().unwrap().excluded = ids;
            }
            ["include", id] => {
                let settings = doc.additions.tray_host.as_mut().unwrap();
                settings.excluded.retain(|s| s != &super::app_id(id));
                host.set_excluded(&settings.excluded);
            }
            ["quit"] => break,
            ["crash"] => std::process::exit(3),
            _ => {
                eprintln!("unrecognized tray command: {line}");
                continue;
            }
        }
        manifest::write_document(loc, doc).map_err(|e| e.to_string())?;
    }
    Ok(())
}
