//! Error codes and the standard messages every app shows for them (SPEC §6).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Every failure that crosses the Link carries one of these codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotInstalled,
    NotRunning,
    LaunchFailed,
    Timeout,
    UnsupportedInput,
    Unavailable,
    TooLarge,
    Denied,
    Busy,
    Cancelled,
    VersionMismatch,
    /// Also used for codes from a newer peer that this version doesn't know.
    #[serde(other)]
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::NotInstalled => "not_installed",
            ErrorCode::NotRunning => "not_running",
            ErrorCode::LaunchFailed => "launch_failed",
            ErrorCode::Timeout => "timeout",
            ErrorCode::UnsupportedInput => "unsupported_input",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::Denied => "denied",
            ErrorCode::Busy => "busy",
            ErrorCode::Cancelled => "cancelled",
            ErrorCode::VersionMismatch => "version_mismatch",
            ErrorCode::Internal => "internal",
        }
    }
}

/// Reasons for `denied` that have their own standard message.
pub mod reason {
    pub const PRIVATE_MODE: &str = "private_mode";
    pub const SECRET: &str = "secret";
    pub const USER_CANCELLED: &str = "user_cancelled";
    pub const DISABLED: &str = "disabled";
    pub const TOKEN: &str = "token";
}

/// A Link error as it travels on the wire: `{"code", "message", "reason"?, "limit"?}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkError {
    pub code: ErrorCode,
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

impl LinkError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        LinkError { code, message: message.into(), reason: None, limit: None }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn internal(message: impl Into<String>) -> Self {
        LinkError::new(ErrorCode::Internal, message)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        LinkError::new(ErrorCode::Unavailable, reason.clone()).with_reason(reason)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        LinkError::new(ErrorCode::UnsupportedInput, message)
    }

    pub fn denied(reason: &str) -> Self {
        LinkError::new(ErrorCode::Denied, reason).with_reason(reason)
    }

    pub fn too_large(limit: u64) -> Self {
        LinkError::new(ErrorCode::TooLarge, format!("limit {limit} bytes")).with_limit(limit)
    }

    pub fn cancelled() -> Self {
        LinkError::new(ErrorCode::Cancelled, "cancelled")
    }

    pub fn busy() -> Self {
        LinkError::new(ErrorCode::Busy, "jobs are running")
    }

    /// The message to show the user, naming the app that failed.
    pub fn user_message(&self, app_name: &str) -> String {
        standard_message(self.code, app_name, self.reason.as_deref(), self.limit)
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            f.write_str(self.code.as_str())
        } else {
            write!(f, "{}: {}", self.code.as_str(), self.message)
        }
    }
}

impl std::error::Error for LinkError {}

impl From<std::io::Error> for LinkError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => LinkError::new(ErrorCode::Timeout, e.to_string()),
            // The peer went away (crashed or was killed mid-connection). The
            // client maps a closed connection during `hello` to not_running.
            std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::UnexpectedEof => LinkError::internal(format!("the connection closed ({e})")),
            _ => LinkError::internal(e.to_string()),
        }
    }
}

/// Formats a byte limit the way people read it: "16 MB", "32 KB".
pub fn format_limit(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB && bytes.is_multiple_of(MIB) {
        format!("{} MB", bytes / MIB)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} bytes")
    }
}

fn sentence(reason: &str) -> String {
    let r = reason.trim().trim_end_matches('.');
    format!("{r}.")
}

/// The standard, user-facing message for an error code. Every Arcade app
/// uses these verbatim (the Qt module implements the same table, and the
/// conformance vectors in `spec/vectors/errors.json` pin them).
pub fn standard_message(code: ErrorCode, app: &str, reason: Option<&str>, limit: Option<u64>) -> String {
    let reason = reason.filter(|r| !r.trim().is_empty());
    match code {
        ErrorCode::NotInstalled => format!("{app} isn't installed."),
        ErrorCode::NotRunning => format!("{app} isn't running."),
        ErrorCode::LaunchFailed => format!("{app} didn't start."),
        ErrorCode::Timeout => format!("{app} didn't respond in time."),
        ErrorCode::UnsupportedInput => format!("{app} can't open this kind of content."),
        ErrorCode::Unavailable => match reason {
            Some(r) => format!("{app} can't do this yet: {}", sentence(r)),
            None => format!("{app} can't do this right now."),
        },
        ErrorCode::TooLarge => {
            let limit = limit.map(|l| format!(" (limit {})", format_limit(l))).unwrap_or_default();
            if app == "Arcade Clipboard" {
                format!("Too large to send to your devices{limit}.")
            } else {
                format!("Too large for {app}{limit}.")
            }
        }
        ErrorCode::Denied => match reason {
            Some(reason::PRIVATE_MODE) => format!("{app} is in Private mode."),
            Some(reason::SECRET) => "Not sent: this looks like a password or key.".to_string(),
            Some(reason::USER_CANCELLED) => "Cancelled.".to_string(),
            Some(reason::DISABLED) => {
                format!("{app} has connections to other Arcade apps turned off.")
            }
            _ => format!("{app} declined this request."),
        },
        ErrorCode::Busy => format!("{app} is busy. Try again when its current job finishes."),
        ErrorCode::Cancelled => "Cancelled.".to_string(),
        ErrorCode::VersionMismatch => format!("{app} needs an update to work with this app."),
        ErrorCode::Internal => match reason {
            Some(r) => format!("{app} ran into a problem: {}", sentence(r)),
            None => format!("{app} ran into a problem."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_that_went_away_reads_as_a_closed_connection() {
        for kind in [std::io::ErrorKind::ConnectionReset, std::io::ErrorKind::BrokenPipe, std::io::ErrorKind::UnexpectedEof] {
            let e = LinkError::from(std::io::Error::from(kind));
            assert_eq!(e.code, ErrorCode::Internal);
            assert!(e.message.contains("closed"), "{e}");
        }
    }

    #[test]
    fn unknown_codes_become_internal() {
        let e: LinkError = serde_json::from_str(r#"{"code":"from_the_future","message":"x"}"#).unwrap();
        assert_eq!(e.code, ErrorCode::Internal);
    }

    #[test]
    fn plan_examples() {
        assert_eq!(
            standard_message(ErrorCode::Unavailable, "Arcade Box", Some("FFmpeg isn't installed"), None),
            "Arcade Box can't do this yet: FFmpeg isn't installed."
        );
        assert_eq!(standard_message(ErrorCode::TooLarge, "Arcade Clipboard", None, Some(16 * 1024 * 1024)), "Too large to send to your devices (limit 16 MB).");
        assert_eq!(standard_message(ErrorCode::Denied, "Arcade Clipboard", Some(reason::PRIVATE_MODE), None), "Arcade Clipboard is in Private mode.");
    }
}
