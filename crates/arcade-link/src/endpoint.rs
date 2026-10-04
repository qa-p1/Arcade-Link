//! Endpoint files: `<runtime>/<arcade-id>.endpoint`, mode 0600 (SPEC §4.1).

use serde::{Deserialize, Serialize};
use std::io;

use crate::paths::{self, Locations};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInfo {
    pub protocol: Vec<u32>,
    /// "unix" or "pipe".
    pub transport: String,
    pub address: String,
    pub pid: u32,
    #[serde(default)]
    pub started_at: String,
    pub token: String,
}

/// 32 bytes from the OS random generator, hex-encoded.
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(hex(&bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares tokens in constant time.
pub(crate) fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn read(locations: &Locations, app_id: &str) -> io::Result<EndpointInfo> {
    let text = std::fs::read_to_string(locations.endpoint(app_id))?;
    serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write(locations: &Locations, app_id: &str, info: &EndpointInfo) -> io::Result<()> {
    paths::ensure_private_dir(&locations.runtime)?;
    let json = serde_json::to_string_pretty(info).map_err(io::Error::other)?;
    paths::write_atomic(&locations.endpoint(app_id), json.as_bytes(), true)
}

/// Removes the endpoint file if it is still ours (same token).
pub fn remove_if_ours(locations: &Locations, app_id: &str, token: &str) {
    if read(locations, app_id).is_ok_and(|e| token_eq(&e.token, token)) {
        let _ = std::fs::remove_file(locations.endpoint(app_id));
    }
}

/// Endpoint files present in the runtime directory (some may be stale).
pub fn list(locations: &Locations) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(&locations.runtime)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".endpoint")).map(String::from))
        .collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_compare() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(token_eq(&a, &a.clone()));
        assert!(!token_eq(&a, &b));
    }

    #[test]
    fn endpoint_file_is_private() {
        let dir = std::env::temp_dir().join(format!("arcade-link-endpoint-{}", std::process::id()));
        let loc = Locations::under(&dir);
        let info = EndpointInfo { protocol: vec![1], transport: "unix".into(), address: "/x".into(), pid: 1, started_at: String::new(), token: "t".into() };
        write(&loc, "arcade.test", &info).unwrap();
        assert_eq!(read(&loc, "arcade.test").unwrap(), info);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(loc.endpoint("arcade.test")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&loc.runtime).unwrap().permissions().mode() & 0o777, 0o700);
        }
        remove_if_ours(&loc, "arcade.test", "other");
        assert!(read(&loc, "arcade.test").is_ok());
        remove_if_ours(&loc, "arcade.test", "t");
        assert!(read(&loc, "arcade.test").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
