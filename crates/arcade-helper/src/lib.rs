//! The Arcade helper: every Arcade app's actions in the file manager menus and the browser extension (Arcade Link
//! SPEC §5.5; Arcade-catalog `dev/everywhere/PLAN.md` §4.4–4.5). Arcade Tools installs it and starts it at login.

pub mod client;
pub mod ids;
pub mod integrations;
pub mod jobs;
pub mod mime;
pub mod native;
pub mod pins;
pub mod resident;
