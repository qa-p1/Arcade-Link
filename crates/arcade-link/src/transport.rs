//! Local sockets: Unix domain sockets, Windows named pipes (SPEC §4.1).

use std::io;
#[cfg(not(windows))]
use std::path::PathBuf;
use std::time::Duration;

use interprocess::local_socket::{prelude::*, ConnectOptions, GenericFilePath, Listener, ListenerOptions, Stream};

use crate::paths::Locations;

/// Unix socket paths must fit `sun_path` (104 bytes on macOS, 108 on Linux).
pub const MAX_SOCKET_PATH: usize = 100;

/// A transport name and the address to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// "unix" or "pipe".
    pub transport: &'static str,
    pub address: String,
}

/// FNV-1a: a short, stable name from a long string (not a security measure).
pub(crate) fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

/// Where `app_id` listens.
///
/// Unix: `<runtime>/<id>.sock`. If that path is too long for a socket (a
/// deep `ARCADE_HOME`, or a long macOS `$TMPDIR`), a hashed name in a private
/// `/tmp/arcade-<uid>/` directory is used instead; the endpoint file records
/// the real address, so clients never compute it themselves.
///
/// Windows: `\\.\pipe\arcade-<user-hash>-<id>`, where the hash covers the user
/// and the runtime directory, so separate `ARCADE_HOME`s never collide.
pub fn address_for(locations: &Locations, app_id: &str) -> io::Result<Address> {
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").unwrap_or_default();
        let hash = short_hash(&format!("{user}|{}", locations.runtime.display()));
        Ok(Address { transport: "pipe", address: format!(r"\\.\pipe\arcade-{hash}-{app_id}") })
    }
    #[cfg(not(windows))]
    {
        let path = locations.runtime.join(format!("{app_id}.sock"));
        let s = path.to_string_lossy().into_owned();
        if s.len() <= MAX_SOCKET_PATH {
            return Ok(Address { transport: "unix", address: s });
        }
        Ok(Address { transport: "unix", address: short_socket_path(locations, app_id)?.to_string_lossy().into_owned() })
    }
}

#[cfg(not(windows))]
fn short_socket_path(locations: &Locations, app_id: &str) -> io::Result<PathBuf> {
    let dir = PathBuf::from(format!("/tmp/arcade-{}", crate::paths::user_id()));
    crate::paths::ensure_private_dir(&dir)?;
    Ok(dir.join(format!("{}.sock", short_hash(&format!("{}|{app_id}", locations.runtime.display())))))
}

pub(crate) fn listen(address: &str) -> io::Result<Listener> {
    let name = address.to_fs_name::<GenericFilePath>()?;
    let opts = ListenerOptions::new().name(name).try_overwrite(true).max_spin_time(Duration::from_millis(200));
    #[cfg(windows)]
    let opts = {
        use interprocess::os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor};
        // Owner (the current user) and SYSTEM only; nobody else may connect.
        let sd = SecurityDescriptor::deserialize(widestring::u16cstr!("D:P(A;;GA;;;SY)(A;;GA;;;OW)"))?;
        opts.security_descriptor(sd)
    };
    let listener = opts.create_sync()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(address, std::fs::Permissions::from_mode(0o600));
    }
    Ok(listener)
}

/// Ends a served connection from the server side: its pending read fails and
/// its thread exits, which frees the pipe instance. While any instance is
/// open, a new listener can't claim the pipe name.
#[cfg(windows)]
pub(crate) fn disconnect(stream: &Stream) {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    #[allow(irrefutable_let_patterns)]
    if let Stream::NamedPipe(s) = stream {
        // SAFETY: the handle is a live server end of this named pipe.
        unsafe { windows_sys::Win32::System::Pipes::DisconnectNamedPipe(s.inner().as_handle().as_raw_handle()) };
    }
}

pub fn connect(address: &str, timeout: Duration) -> io::Result<Stream> {
    let name = address.to_fs_name::<GenericFilePath>()?;
    let opts = ConnectOptions::new().name(name);
    #[cfg(unix)]
    let opts = opts.wait_mode(interprocess::ConnectWaitMode::Timeout(timeout));
    #[cfg(not(unix))]
    let _ = timeout;
    opts.connect_sync()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn long_runtime_dirs_get_a_short_socket() {
        let deep = std::env::temp_dir().join("a".repeat(120));
        let a = address_for(&Locations::under(&deep), "arcade.clipboard").unwrap();
        assert!(a.address.len() <= MAX_SOCKET_PATH, "{}", a.address);
        assert!(a.address.starts_with("/tmp/arcade-"));
        let b = address_for(&Locations::under(&deep.join("b")), "arcade.clipboard").unwrap();
        assert_ne!(a.address, b.address);
    }

    /// The macOS case: `$TMPDIR` looks like `/var/folders/xx/<28 chars>/T/`.
    #[cfg(unix)]
    #[test]
    fn macos_style_tmpdir_fits() {
        let tmp = PathBuf::from("/var/folders/zz/zyxvpxvq6csfxvn_n0000000000000/T/arcade");
        let loc = Locations { registry: tmp.clone(), runtime: tmp.clone(), handoff: tmp };
        let a = address_for(&loc, "arcade.clipboard").unwrap();
        assert!(a.address.len() <= MAX_SOCKET_PATH && a.address.len() < 104);
        assert!(a.address.ends_with("arcade.clipboard.sock"));
    }
}
