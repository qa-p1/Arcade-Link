//! Arcade Link: how the Arcade apps find each other and talk.
//!
//! A protocol and a small library, not a process:
//!
//! - [`registry`]: which apps are installed and what they can do (one
//!   manifest JSON file per app, cached by modification time);
//! - [`endpoint`] + [`transport`]: whether an app is running and where to
//!   connect (an endpoint file plus a Unix socket or Windows named pipe);
//! - [`wire`]: newline-delimited JSON messages;
//! - [`content`]: the shared content types;
//! - [`handoff`]: private files for in-memory content;
//! - [`server`] / [`client`] / [`oneshot`]: serving and calling actions;
//! - [`presence`]: an app's manifest and server, kept in step with its
//!   "Connect with other Arcade apps" switch.
//!
//! The core is synchronous: one accept thread blocked in `accept`, a thread
//! per connection, no timers, no polling. See `SPEC.md` for the protocol.

pub mod client;
pub mod content;
pub mod endpoint;
pub mod error;
pub mod handoff;
pub mod manifest;
pub mod oneshot;
pub mod paths;
pub mod presence;
pub mod registry;
pub mod server;
pub mod transport;
pub mod wire;

pub use client::{invoke_action, AppState, CallOptions, Client};
pub use content::Content;
pub use error::{ErrorCode, LinkError};
pub use handoff::Handoff;
pub use manifest::{ids, Action, Manifest};
pub use paths::Locations;
pub use presence::Presence;
pub use registry::{Registry, SharedRegistry};
pub use server::{Handler, InvokeContext, Job, JobTicket, Reply, Server, ServerConfig};
pub use wire::{InvokeRequest, InvokeResult, JobProgress, PeerInfo};
