//! The header every app's "Copy diagnostics" starts with (STANDARD.md §10):
//! app, version, channel, OS and desktop, Link protocol and peers, engines.
//! App-specific lines follow it. Nothing here reads secrets: no tokens,
//! paths inside the user's documents or clipboard contents.
//!
//! ```no_run
//! use arcade_link::diagnostics::Header;
//! let mut header = Header::new("arcade.box", "0.4.0", "stable");
//! header.link_enabled = true;
//! header.peers_from(&arcade_link::Locations::discover());
//! header.engine("FFmpeg", "7.1");
//! let text = header.render() + "Jobs: 0 running\n";
//! ```

use crate::client::{app_state, AppState};
use crate::manifest::{app_name, ids};
use crate::paths::Locations;
use crate::registry::Registry;
use crate::wire::{PeerInfo, PROTOCOL_VERSION};

/// The shared first lines of an app's diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub app_id: String,
    pub version: String,
    pub channel: String,
    pub os: String,
    pub desktop: String,
    /// The app's "Connect with other Arcade apps" switch.
    pub link_enabled: bool,
    /// (Link id, "running 0.4.0" / "installed 0.3.2" / "not installed").
    pub peers: Vec<(String, String)>,
    /// (engine, status), e.g. ("Tesseract", "5.3.4") or ("FFmpeg", "not found").
    pub engines: Vec<(String, String)>,
}

impl Header {
    /// OS and desktop come from the running system; peers and engines start empty.
    pub fn new(app_id: &str, version: &str, channel: &str) -> Header {
        Header {
            app_id: app_id.into(),
            version: version.into(),
            channel: channel.into(),
            os: os_description(),
            desktop: desktop_description(),
            link_enabled: true,
            peers: Vec::new(),
            engines: Vec::new(),
        }
    }

    /// Every other Arcade app's state, from the registry and endpoints.
    pub fn peers_from(&mut self, locations: &Locations) {
        let registry = Registry::load(locations);
        let me = PeerInfo { id: self.app_id.clone(), version: self.version.clone() };
        self.peers = ids::APPS
            .iter()
            .chain([&ids::TOOLS])
            .filter(|id| **id != self.app_id)
            .map(|id| {
                let state = match app_state(locations, &registry, id, &me) {
                    AppState::Running { version } => format!("running {version}"),
                    AppState::Installed { version } => format!("installed {version}"),
                    AppState::NotInstalled => "not installed".into(),
                };
                (id.to_string(), state)
            })
            .collect();
    }

    pub fn engine(&mut self, name: &str, status: &str) {
        self.engines.push((name.into(), status.into()));
    }

    /// The header as plain text, one fact per line, ending in a newline.
    pub fn render(&self) -> String {
        let mut out = format!("{} {} ({})\n", app_name(&self.app_id), self.version, self.channel);
        out += &format!("OS: {}\n", self.os);
        out += &format!("Desktop: {}\n", self.desktop);
        out += &format!("Link: protocol {}, connections {}\n", PROTOCOL_VERSION, if self.link_enabled { "on" } else { "off" });
        if !self.peers.is_empty() {
            let peers: Vec<String> = self.peers.iter().map(|(id, state)| format!("{} {state}", app_name(id).trim_start_matches("Arcade "))).collect();
            out += &format!("Peers: {}\n", peers.join(", "));
        }
        if !self.engines.is_empty() {
            let engines: Vec<String> = self.engines.iter().map(|(name, status)| format!("{name} {status}")).collect();
            out += &format!("Engines: {}\n", engines.join(", "));
        }
        out
    }
}

fn os_description() -> String {
    let arch = std::env::consts::ARCH;
    #[cfg(target_os = "linux")]
    {
        let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let name = release.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_string()).unwrap_or_else(|| "Linux".into());
        format!("{name} ({arch})")
    }
    #[cfg(not(target_os = "linux"))]
    {
        let name = if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            std::env::consts::OS
        };
        format!("{name} ({arch})")
    }
}

fn desktop_description() -> String {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    if cfg!(windows) || cfg!(target_os = "macos") {
        return "native".into();
    }
    let desktop = if var("HYPRLAND_INSTANCE_SIGNATURE").is_some() { Some("Hyprland".to_string()) } else { var("XDG_CURRENT_DESKTOP") };
    let session = var("XDG_SESSION_TYPE").unwrap_or_else(|| if var("WAYLAND_DISPLAY").is_some() { "wayland".into() } else { "x11".into() });
    match desktop {
        Some(d) => format!("{d} ({session})"),
        None => format!("unknown ({session})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_shared_lines_in_order() {
        let mut h = Header::new("arcade.box", "0.4.0", "stable");
        h.os = "Ubuntu 24.04 (x86_64)".into();
        h.desktop = "GNOME (wayland)".into();
        h.peers = vec![("arcade.look".into(), "running 0.4.1".into()), ("arcade.find".into(), "not installed".into())];
        h.engine("FFmpeg", "7.1");
        assert_eq!(
            h.render(),
            "Arcade Box 0.4.0 (stable)\nOS: Ubuntu 24.04 (x86_64)\nDesktop: GNOME (wayland)\nLink: protocol 1, connections on\n\
             Peers: Look running 0.4.1, Find not installed\nEngines: FFmpeg 7.1\n"
        );
    }

    #[test]
    fn peers_cover_every_other_app() {
        let root = std::env::temp_dir().join(format!("arcade-diag-{}", std::process::id()));
        let mut h = Header::new("arcade.shelf", "0.4.0", "beta");
        h.peers_from(&Locations::under(&root));
        assert_eq!(h.peers.len(), 7);
        assert!(h.peers.iter().all(|(id, state)| id != "arcade.shelf" && state == "not installed"));
        let _ = std::fs::remove_dir_all(root);
    }
}
