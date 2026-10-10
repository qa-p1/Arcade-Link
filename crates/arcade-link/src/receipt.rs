//! Install receipts, independent of the installer feature (SPEC §8.6).
//! Use `Store::new(&locations)` to avoid process-wide environment changes.

use crate::{
    paths::{self, Locations},
    shortcuts::identifier,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallMethod {
    Appimage,
    WindowsInstaller,
    MacosBundle,
    Tarball,
    Manual,
    Dev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManagedBy {
    #[serde(rename = "self")]
    SelfManaged,
    #[serde(rename = "tools")]
    Tools,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    #[serde(default)]
    pub desktop_entry: Option<PathBuf>,
    #[serde(default)]
    pub icons: Vec<PathBuf>,
    #[serde(default)]
    pub cli: Option<PathBuf>,
    #[serde(default)]
    pub autostart: Option<PathBuf>,
    #[serde(default)]
    pub uninstaller: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Previous {
    pub version: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub channel: String,
    pub method: InstallMethod,
    pub managed_by: ManagedBy,
    pub path: PathBuf,
    pub integration: Integration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Previous>,
    pub installed_at: String,
    pub updated_at: String,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn valid_id(id: &str) -> bool {
    id.strip_prefix("arcade.").is_some_and(identifier)
}
fn absolute(path: &Path) -> bool {
    let s = path.to_string_lossy();
    path.is_absolute()
        || s.starts_with('/') // Receipt vectors may describe another OS.
        || (s.len() > 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':' && matches!(s.as_bytes()[2], b'\\' | b'/'))
        || s.starts_with("\\\\")
}
fn timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return false;
    }
    if b.iter().enumerate().any(|(i, c)| ![4, 7, 10, 13, 16, 19].contains(&i) && !c.is_ascii_digit()) {
        return false;
    }
    let n = |a, b| s[a..b].parse::<u32>().unwrap_or(0);
    let (y, m, d) = (n(0, 4), n(5, 7), n(8, 10));
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    };
    d > 0 && d <= days && n(11, 13) < 24 && n(14, 16) < 60 && n(17, 19) < 60
}

impl Receipt {
    pub fn validate(&self) -> io::Result<()> {
        if self.schema != 1 || !valid_id(&self.id) || self.version.trim().is_empty() || self.channel.trim().is_empty() {
            return Err(invalid("receipt needs schema 1, Arcade id, version and channel"));
        }
        if !absolute(&self.path) {
            return Err(invalid("receipt path must be absolute"));
        }
        for p in self
            .integration
            .icons
            .iter()
            .chain(self.integration.desktop_entry.iter())
            .chain(self.integration.cli.iter())
            .chain(self.integration.autostart.iter())
            .chain(self.integration.uninstaller.iter())
        {
            if !absolute(p) {
                return Err(invalid("integration paths must be absolute"));
            }
        }
        if self.previous.as_ref().is_some_and(|p| p.version.trim().is_empty() || !absolute(&p.path) || p.path == self.path) {
            return Err(invalid("invalid previous installation"));
        }
        if !timestamp(&self.installed_at) || !timestamp(&self.updated_at) || self.updated_at < self.installed_at {
            return Err(invalid("receipt timestamps must be ordered UTC RFC3339 seconds"));
        }
        Ok(())
    }
    pub fn from_json(text: &str) -> io::Result<Self> {
        let r: Self = serde_json::from_str(text).map_err(|e| invalid(e.to_string()))?;
        r.validate()?;
        Ok(r)
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}
impl Store {
    /// A sibling of the registry: `$ARCADE_HOME/installs`, or
    /// `%LOCALAPPDATA%\Arcade\installs` on Windows.
    pub fn new(locations: &Locations) -> Self {
        Self { dir: locations.registry.parent().unwrap_or(&locations.registry).join("installs") }
    }
    pub fn discover() -> Self {
        Self::new(&Locations::discover())
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn path(&self, id: &str) -> io::Result<PathBuf> {
        if !valid_id(id) {
            return Err(invalid("invalid receipt id"));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }
    pub fn read(&self, id: &str) -> io::Result<Option<Receipt>> {
        match fs::read_to_string(self.path(id)?) {
            Ok(text) => {
                let receipt = Receipt::from_json(&text)?;
                if receipt.id != id {
                    return Err(invalid("receipt id differs from filename"));
                }
                Ok(Some(receipt))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    /// Atomic replacement, 0600 on Unix. Dev builds never create receipts.
    pub fn write(&self, receipt: &Receipt) -> io::Result<()> {
        receipt.validate()?;
        if receipt.method == InstallMethod::Dev {
            return Err(invalid("dev builds never write receipts"));
        }
        paths::ensure_private_dir(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(receipt).map_err(|e| invalid(e.to_string()))?;
        paths::write_atomic(&self.path(&receipt.id)?, &bytes, true)
    }
    pub fn remove(&self, id: &str) -> io::Result<()> {
        match fs::remove_file(self.path(id)?) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}
