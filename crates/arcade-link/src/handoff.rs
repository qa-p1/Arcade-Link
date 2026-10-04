//! Handoff files: in-memory content written once so another app can read
//! it by path (SPEC §5.3).
//!
//! The creator writes `<handoff>/<random>/<name>` (file 0600, directory
//! 0700) and puts itself in the value's `owner`. The receiver treats the file
//! as read-only. The creator deletes the directory when the job finishes
//! (dropping the [`Handoff`]); every app also removes directories older than
//! 24 hours at startup.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::content::{self, Content};
use crate::paths::{self, Locations};

/// Handoff directories older than this are removed at startup.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// One private handoff directory, deleted on drop unless [`Handoff::keep`] is called.
#[derive(Debug)]
pub struct Handoff {
    dir: PathBuf,
    owner: String,
    keep: bool,
}

fn random_name() -> io::Result<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(crate::endpoint::hex(&b))
}

impl Handoff {
    pub fn create(locations: &Locations, owner: &str) -> io::Result<Handoff> {
        paths::ensure_private_dir(&locations.handoff)?;
        let dir = locations.handoff.join(random_name()?);
        std::fs::create_dir(&dir)?;
        paths::ensure_private_dir(&dir)?;
        Ok(Handoff {
            dir,
            owner: owner.into(),
            keep: false,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes `bytes` as `name` (only the file name part is used).
    pub fn write(&self, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let name = Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.starts_with('.'))
            .unwrap_or("content");
        let path = self.dir.join(name);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&path)?;
        f.write_all(bytes)?;
        Ok(path)
    }

    /// Writes `bytes` and returns a `file/<kind>` value owned by this app.
    pub fn file(&self, name: &str, bytes: &[u8]) -> io::Result<Content> {
        let path = self.write(name, bytes)?;
        Ok(Content {
            kind: content::file_type_for_path(&path),
            path: Some(path.to_string_lossy().into_owned()),
            owner: Some(self.owner.clone()),
            size: Some(bytes.len() as u64),
            name: Some(name.into()),
            ..Default::default()
        })
    }

    /// Text as a value: inline up to 256 KiB, otherwise a handoff file.
    pub fn text(&self, kind: &str, text: &str) -> io::Result<Content> {
        if text.len() <= content::INLINE_TEXT_LIMIT {
            return Ok(Content::text(kind, text));
        }
        let mut c = self.file("text.txt", text.as_bytes())?;
        c.kind = kind.into();
        Ok(c)
    }

    /// Leaves the directory in place (the 24-hour cleanup removes it later).
    pub fn keep(mut self) -> PathBuf {
        self.keep = true;
        self.dir.clone()
    }
}

impl Drop for Handoff {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Reads text that may have been handed off as a file.
pub fn read_text(c: &Content) -> io::Result<String> {
    if let Some(t) = &c.text {
        return Ok(t.clone());
    }
    match &c.path {
        Some(p) => std::fs::read_to_string(p),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "value has no text",
        )),
    }
}

/// Removes handoff directories older than [`MAX_AGE`]; returns how many.
/// One directory listing: call it off the startup path.
pub fn cleanup_stale(locations: &Locations) -> usize {
    let Ok(rd) = std::fs::read_dir(&locations.handoff) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > MAX_AGE);
        if old
            && e.file_type().is_ok_and(|t| t.is_dir())
            && std::fs::remove_dir_all(e.path()).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_lifecycle() {
        let root = std::env::temp_dir().join(format!("arcade-link-handoff-{}", std::process::id()));
        let loc = Locations::under(&root);
        let h = Handoff::create(&loc, "arcade.test").unwrap();
        let c = h.file("../region.png", b"png").unwrap();
        assert_eq!(c.kind, "file/image");
        assert_eq!(c.owner.as_deref(), Some("arcade.test"));
        let p = PathBuf::from(c.path.unwrap());
        assert_eq!(p.parent(), Some(h.dir()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(h.dir()).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let dir = h.dir().to_path_buf();
        drop(h);
        assert!(!dir.exists());
        let big = "x".repeat(content::INLINE_TEXT_LIMIT + 1);
        let h = Handoff::create(&loc, "arcade.test").unwrap();
        let t = h.text("text/plain", &big).unwrap();
        assert!(t.text.is_none() && t.path.is_some());
        assert_eq!(read_text(&t).unwrap().len(), big.len());
        let kept = h.keep();
        assert!(kept.exists());
        assert_eq!(cleanup_stale(&loc), 0);
        std::fs::remove_dir_all(root).ok();
    }
}
