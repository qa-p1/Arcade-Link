//! Local sockets: Unix domain sockets, Windows named pipes (SPEC §4.1).

use std::io;
#[cfg(not(windows))]
use std::path::PathBuf;
use std::time::Duration;

use interprocess::local_socket::{prelude::*, ConnectOptions, GenericFilePath, Listener, ListenerOptions, Stream};

use crate::paths::Locations;

#[cfg(windows)]
mod windows;

/// The named-pipe backend has no timeout support. Use native overlapped I/O
/// with event waits there; Unix keeps its existing socket implementation.
/// A manual cancellation event also covers closing just before a read starts.
#[derive(Clone)]
pub(crate) struct IoControl {
    #[cfg(windows)]
    native: std::sync::Arc<windows::State>,
}
impl IoControl {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            #[cfg(windows)]
            native: std::sync::Arc::new(windows::State::new()?),
        })
    }
    pub(crate) fn recv_timeout(&self, stream: &Stream, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(windows)]
        {
            let _ = stream;
            self.native.recv_timeout(timeout);
            Ok(())
        }
        #[cfg(unix)]
        {
            stream.set_recv_timeout(timeout)
        }
    }
    pub(crate) fn send_timeout(&self, stream: &Stream, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(windows)]
        {
            let _ = stream;
            self.native.send_timeout(timeout);
            Ok(())
        }
        #[cfg(unix)]
        {
            stream.set_send_timeout(timeout)
        }
    }
    pub(crate) fn read(&self, stream: &Stream, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            self.native.read(stream, buf)
        }
        #[cfg(unix)]
        {
            std::io::Read::read(&mut &*stream, buf)
        }
    }
    pub(crate) fn write(&self, stream: &Stream, buf: &[u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            self.native.write(stream, buf)
        }
        #[cfg(unix)]
        {
            std::io::Write::write(&mut &*stream, buf)
        }
    }
    pub(crate) fn flush(&self, stream: &Stream) -> io::Result<()> {
        // FlushFileBuffers waits for the peer to consume bytes and has no
        // deadline. Writes already queue ordered bytes; interprocess's limbo
        // handles draining at drop, marked dirty by our native write helper.
        #[cfg(windows)]
        {
            let _ = stream;
            Ok(())
        }
        #[cfg(unix)]
        {
            std::io::Write::flush(&mut &*stream)
        }
    }
    pub(crate) fn close(&self, stream: &Stream) {
        #[cfg(windows)]
        self.native.cancel();
        close(stream);
    }
}

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
/// deep `ARCADE_HOME`, or a long macOS `$TMPDIR`), a hashed name goes in a
/// private `arcade-<uid>/` directory under the first of `$XDG_RUNTIME_DIR`,
/// `$TMPDIR` and `/tmp` that keeps the path short enough (SPEC §4.1), so an
/// isolated run stays inside its own runtime or temporary root. The endpoint
/// file records the real address; clients never compute it themselves.
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
    let name = format!("{}.sock", short_hash(&format!("{}|{app_id}", locations.runtime.display())));
    let folder = format!("arcade-{}", crate::paths::user_id());
    for base in short_socket_bases() {
        let dir = base.join(&folder);
        if dir.join(&name).as_os_str().len() <= MAX_SOCKET_PATH {
            crate::paths::ensure_private_dir(&dir)?;
            return Ok(dir.join(name));
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput, "no runtime or temporary folder is short enough for a socket"))
}

/// Where long socket paths go instead, in order: `$XDG_RUNTIME_DIR`, `$TMPDIR`, `/tmp`.
#[cfg(not(windows))]
fn short_socket_bases() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> =
        ["XDG_RUNTIME_DIR", "TMPDIR"].iter().filter_map(std::env::var_os).filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute()).collect();
    bases.push(PathBuf::from("/tmp"));
    bases
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
pub(crate) fn cancel_io(stream: &Stream) {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    #[allow(irrefutable_let_patterns)]
    if let Stream::NamedPipe(s) = stream {
        // SAFETY: the handle is a live server end of this named pipe; a null
        // OVERLAPPED cancels every pending operation on it, synchronous reads
        // included (DisconnectNamedPipe would queue behind such a read).
        unsafe { windows_sys::Win32::System::IO::CancelIoEx(s.inner().as_handle().as_raw_handle(), std::ptr::null()) };
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

pub(crate) fn close(stream: &Stream) {
    #[cfg(unix)]
    {
        #[allow(irrefutable_let_patterns)]
        if let Stream::UdSocket(s) = stream {
            let _ = s.inner().shutdown(std::net::Shutdown::Both);
        }
    }
    #[cfg(windows)]
    cancel_io(stream);
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
        let folder = format!("arcade-{}", crate::paths::user_id());
        let first = short_socket_bases().into_iter().map(|b| b.join(&folder)).find(|d| d.as_os_str().len() + 17 <= MAX_SOCKET_PATH).unwrap();
        assert!(std::path::Path::new(&a.address).starts_with(&first), "{} not under {}", a.address, first.display());
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
