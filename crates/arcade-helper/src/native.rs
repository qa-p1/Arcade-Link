//! The browser bridge: Chrome's and Firefox's native messaging (a 32-bit length in native byte order, then UTF-8
//! JSON). The browser starts this process when the Arcade extension connects; it answers only that extension.
//!
//! Requests are `{id, method, params}`; answers `{id, result}` or `{id, error: {code, message}}`; job updates are
//! `{event: "job", job, status, fraction?, message?, outputs?}`. Methods: `hello`, `offers`, `form`, `run`,
//! `cancel`, `prefs.get`, `prefs.set`, `install` (see `Bridge`).

use serde_json::{json, Value};
use std::io::{self, Read, Write};

use crate::ids;

/// Largest message the extension may send. Browsers allow more, but nothing we take is bigger: files travel as
/// paths, or as base64 for a page's image.
pub const MAX_INCOMING: usize = 64 * 1024 * 1024;
/// Chrome's limit for host → extension messages.
pub const MAX_OUTGOING: usize = 1024 * 1024;

/// Reads one message; `None` at end of input (the browser closed the port).
pub fn read_message(r: &mut impl Read) -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_INCOMING {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("message of {len} bytes is too large")));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Writes one message. A message over Chrome's 1 MiB limit is refused rather than sent (the browser would drop the
/// connection).
pub fn write_message(w: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_OUTGOING {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message over 1 MiB"));
    }
    w.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

/// Which browser started us, from our arguments: Chrome passes the caller's origin
/// (`chrome-extension://<id>/`, plus `--parent-window=` on Windows); Firefox passes the host manifest's path and the
/// add-on ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Chromium { extension_id: String },
    Firefox { addon_id: String },
}

impl Caller {
    pub fn from_args(args: &[String]) -> Option<Caller> {
        if let Some(origin) = args.iter().find_map(|a| a.strip_prefix("chrome-extension://")) {
            return Some(Caller::Chromium { extension_id: origin.trim_end_matches('/').to_string() });
        }
        match args {
            [manifest, addon] if manifest.ends_with(".json") && !addon.starts_with('-') => Some(Caller::Firefox { addon_id: addon.clone() }),
            _ => None,
        }
    }

    /// Only the Arcade extension may use the helper. Browsers already enforce this through the host manifest's
    /// allow list; this is the second lock.
    pub fn allowed(&self) -> bool {
        match self {
            Caller::Chromium { extension_id } => extension_id == ids::CHROME_EXTENSION_ID,
            Caller::Firefox { addon_id } => addon_id == ids::FIREFOX_ADDON_ID,
        }
    }

    pub fn browser(&self) -> &'static str {
        match self {
            Caller::Chromium { .. } => "chromium",
            Caller::Firefox { .. } => "firefox",
        }
    }
}

/// An error answer.
pub fn error(id: &Value, code: &str, message: &str) -> Value {
    json!({"id": id, "error": {"code": code, "message": message}})
}

/// A result answer.
pub fn result(id: &Value, result: Value) -> Value {
    json!({"id": id, "result": result})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trip_and_end_of_input() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"id": 1, "method": "hello"})).unwrap();
        write_message(&mut buf, &json!({"id": 2})).unwrap();
        let mut r = io::Cursor::new(buf);
        assert_eq!(read_message(&mut r).unwrap().unwrap()["method"], "hello");
        assert_eq!(read_message(&mut r).unwrap().unwrap()["id"], 2);
        assert!(read_message(&mut r).unwrap().is_none());
    }

    #[test]
    fn oversized_and_broken_messages_are_refused() {
        let mut r = io::Cursor::new(((MAX_INCOMING + 1) as u32).to_ne_bytes().to_vec());
        assert!(read_message(&mut r).is_err());
        let mut bytes = 3u32.to_ne_bytes().to_vec();
        bytes.extend_from_slice(b"{x}");
        assert!(read_message(&mut io::Cursor::new(bytes)).is_err());
        let big = json!({"s": "x".repeat(MAX_OUTGOING)});
        assert!(write_message(&mut Vec::new(), &big).is_err());
    }

    #[test]
    fn callers_and_the_allow_list() {
        let chrome = Caller::from_args(&[format!("chrome-extension://{}/", ids::CHROME_EXTENSION_ID)]).unwrap();
        assert!(chrome.allowed());
        let windows = Caller::from_args(&[format!("chrome-extension://{}/", ids::CHROME_EXTENSION_ID), "--parent-window=0".into()]).unwrap();
        assert_eq!(windows, chrome);
        assert!(!Caller::from_args(&["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".into()]).unwrap().allowed());
        let firefox = Caller::from_args(&["/home/u/.mozilla/native-messaging-hosts/dev.arcade.helper.json".into(), ids::FIREFOX_ADDON_ID.into()]).unwrap();
        assert!(firefox.allowed() && firefox.browser() == "firefox");
        assert!(!Caller::from_args(&["/x/dev.arcade.helper.json".into(), "evil@example.com".into()]).unwrap().allowed());
        assert_eq!(Caller::from_args(&["menu".into(), "/home/u/a.png".into()]), None);
        assert_eq!(Caller::from_args(&["--background".into()]), None);
    }
}
