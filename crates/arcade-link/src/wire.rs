//! The wire protocol (SPEC §4): newline-delimited JSON with JSON-RPC-style
//! envelopes. Requests carry `id`; notifications don't; responses carry `id`
//! and exactly one of `result` or `error`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};

use crate::content::Content;
use crate::error::{ErrorCode, LinkError};

/// Protocol versions this implementation speaks, oldest first.
pub const SUPPORTED_PROTOCOLS: &[u32] = &[1];
/// The newest protocol version this implementation speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The largest accepted line, newline included.
pub const MAX_LINE: usize = 1024 * 1024;

pub mod method {
    pub const HELLO: &str = "hello";
    pub const DESCRIBE: &str = "describe";
    pub const INVOKE: &str = "invoke";
    pub const JOB_CANCEL: &str = "job.cancel";
    pub const SUBSCRIBE: &str = "subscribe";
    pub const APP_STATUS: &str = "app.status";
    pub const APP_ACTIVATE: &str = "app.activate";
    pub const APP_QUIT: &str = "app.quit";
    pub const JOB_PROGRESS: &str = "job.progress";
    pub const JOB_DONE: &str = "job.done";
    pub const APP_CHANGED: &str = "app.changed";
}

/// Any message on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<LinkError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Request,
    Notification,
    Response,
}

impl Message {
    pub fn request(id: u64, method: &str, params: Value) -> Message {
        Message {
            v: PROTOCOL_VERSION,
            id: Some(id),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn notification(method: &str, params: Value) -> Message {
        Message {
            v: PROTOCOL_VERSION,
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn response(id: u64, result: Result<Value, LinkError>) -> Message {
        let (result, error) = match result {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        Message {
            v: PROTOCOL_VERSION,
            id: Some(id),
            method: None,
            params: None,
            result,
            error,
        }
    }

    pub fn kind(&self) -> Kind {
        match (&self.id, &self.method) {
            (Some(_), Some(_)) => Kind::Request,
            (None, Some(_)) => Kind::Notification,
            _ => Kind::Response,
        }
    }

    pub fn params(&self) -> &Value {
        static NULL: Value = Value::Null;
        self.params.as_ref().unwrap_or(&NULL)
    }

    /// One line, newline included.
    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).expect("message serializes");
        s.push('\n');
        s
    }

    /// Parses and validates one line.
    pub fn parse(line: &str) -> Result<Message, LinkError> {
        if line.len() > MAX_LINE {
            return Err(LinkError::too_large(MAX_LINE as u64));
        }
        let m: Message = serde_json::from_str(line.trim_end_matches(['\r', '\n']))
            .map_err(|e| LinkError::internal(format!("invalid message: {e}")))?;
        if m.v == 0 {
            return Err(LinkError::internal("invalid message: v must be 1 or later"));
        }
        let ok = match m.kind() {
            Kind::Request | Kind::Notification => {
                m.result.is_none()
                    && m.error.is_none()
                    && !m.method.as_deref().unwrap_or("").is_empty()
            }
            Kind::Response => m.id.is_some() && (m.result.is_some() != m.error.is_some()),
        };
        if !ok {
            return Err(LinkError::internal(
                "invalid message: not a request, notification or response",
            ));
        }
        Ok(m)
    }
}

/// Reads messages line by line. A partial line survives a receive timeout,
/// so callers can wait with a timeout without corrupting the stream.
pub struct LineReader<R> {
    inner: R,
    pending: Vec<u8>,
}

impl<R: Read> LineReader<R> {
    pub fn new(inner: R) -> Self {
        LineReader {
            inner,
            pending: Vec::new(),
        }
    }

    /// The next message; `Ok(None)` at end of stream. A timeout surfaces as
    /// a `timeout` error and can be retried.
    pub fn read_message(&mut self) -> Result<Option<Message>, LinkError> {
        loop {
            if let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=i).collect();
                let text = std::str::from_utf8(&line)
                    .map_err(|_| LinkError::internal("message is not UTF-8"))?;
                if text.trim().is_empty() {
                    continue;
                }
                return Message::parse(text).map(Some);
            }
            if self.pending.len() > MAX_LINE {
                self.pending.clear();
                return Err(LinkError::too_large(MAX_LINE as u64));
            }
            let mut chunk = [0u8; 16 * 1024];
            match self.inner.read(&mut chunk) {
                Ok(0) => {
                    return if self.pending.iter().all(u8::is_ascii_whitespace) {
                        Ok(None)
                    } else {
                        Err(LinkError::internal("connection closed mid-message"))
                    };
                }
                Ok(n) => self.pending.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}

pub fn write_message(writer: &mut impl Write, m: &Message) -> std::io::Result<()> {
    writer.write_all(m.to_line().as_bytes())?;
    writer.flush()
}

/// The newest version both sides speak.
pub fn negotiate(client: &[u32], server: &[u32]) -> Option<u32> {
    client.iter().filter(|v| server.contains(v)).max().copied()
}

/// Who is on the other end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PeerInfo {
    pub id: String,
    #[serde(default)]
    pub version: String,
}

/// `invoke` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvokeRequest {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default)]
    pub inputs: Vec<Content>,
    #[serde(default)]
    pub options: Value,
    #[serde(default)]
    pub context: InvokeContextInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvokeContextInfo {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub reason: String,
}

impl InvokeRequest {
    pub fn new(action: &str, source: &str) -> InvokeRequest {
        InvokeRequest {
            action: action.into(),
            options: json!({}),
            context: InvokeContextInfo {
                source: source.into(),
                interactive: true,
                reason: "user-click".into(),
            },
            ..Default::default()
        }
    }

    pub fn input(mut self, c: Content) -> InvokeRequest {
        self.inputs.push(c);
        self
    }

    pub fn preset(mut self, preset: Option<&str>) -> InvokeRequest {
        self.preset = preset.map(Into::into);
        self
    }

    pub fn options(mut self, options: Value) -> InvokeRequest {
        self.options = options;
        self
    }
}

/// What a finished action returns, directly or in `job.done`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InvokeResult {
    #[serde(default)]
    pub outputs: Vec<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl InvokeResult {
    pub fn message(msg: impl Into<String>) -> InvokeResult {
        InvokeResult {
            message: Some(msg.into()),
            ..Default::default()
        }
    }

    pub fn outputs(outputs: Vec<Content>, msg: impl Into<String>) -> InvokeResult {
        InvokeResult {
            outputs,
            message: Some(msg.into()),
            data: None,
        }
    }
}

/// `job.done` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobDone {
    pub job: String,
    /// "success", "error" or "cancelled".
    pub status: String,
    #[serde(flatten)]
    pub result: InvokeResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<LinkError>,
}

impl JobDone {
    pub fn into_result(self) -> Result<InvokeResult, LinkError> {
        match self.status.as_str() {
            "success" => Ok(self.result),
            "cancelled" => Err(self.error.unwrap_or_else(LinkError::cancelled)),
            _ => Err(self
                .error
                .unwrap_or_else(|| LinkError::internal(self.result.message.unwrap_or_default()))),
        }
    }
}

/// `job.progress` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobProgress {
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f32>,
    #[serde(default)]
    pub message: String,
}

pub(crate) fn version_mismatch(theirs: &[u32]) -> LinkError {
    LinkError::new(
        ErrorCode::VersionMismatch,
        format!(
            "no common protocol version (peer speaks {theirs:?}, we speak {SUPPORTED_PROTOCOLS:?})"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert_eq!(
            Message::parse(r#"{"v":1,"id":1,"method":"hello","params":{}}"#)
                .unwrap()
                .kind(),
            Kind::Request
        );
        assert_eq!(
            Message::parse(r#"{"v":1,"method":"job.progress","params":{}}"#)
                .unwrap()
                .kind(),
            Kind::Notification
        );
        assert_eq!(
            Message::parse(r#"{"v":1,"id":3,"result":{}}"#)
                .unwrap()
                .kind(),
            Kind::Response
        );
        assert!(
            Message::parse(r#"{"v":1,"id":3,"result":{},"error":{"code":"internal"}}"#).is_err()
        );
        assert!(Message::parse(r#"{"v":1,"id":3}"#).is_err());
        assert!(Message::parse(r#"{"id":3,"result":1}"#).is_err());
    }

    #[test]
    fn oversized_lines_are_refused() {
        let big = format!(
            "{{\"v\":1,\"method\":\"x\",\"params\":\"{}\"}}\n",
            "a".repeat(MAX_LINE)
        );
        let mut r = LineReader::new(big.as_bytes());
        assert_eq!(r.read_message().unwrap_err().code, ErrorCode::TooLarge);
    }
}
