//! Signed release manifests and chunked (delta) downloads.
//!
//! A release publishes `manifest.json`: per file its size, SHA-256 and the
//! list of FastCDC chunks (8/16/32 KB, BLAKE3 per chunk). Stable and nightly
//! manifests are signed into `manifest.json.sig` with ed25519 in minisign's
//! format, so `minisign -Vm manifest.json -x manifest.json.sig -P <key>` also
//! verifies them.
//!
//! An updater verifies the manifest against its trusted keys, indexes the
//! chunks it already has ([`ChunkIndex`]), asks [`plan_delta`] which byte
//! ranges of the new file are missing, fetches them (HTTP Range), and calls
//! [`assemble`]. Every chunk and the whole file are checked; any error means
//! the caller downloads the full file instead.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use sha2::Digest;

pub const SCHEMA: u32 = 1;
pub const CHUNK_MIN: u32 = 8 * 1024;
pub const CHUNK_AVG: u32 = 16 * 1024;
pub const CHUNK_MAX: u32 = 32 * 1024;
/// The name of the signed manifest and its signature in a release.
pub const MANIFEST_NAME: &str = "manifest.json";
pub const SIGNATURE_NAME: &str = "manifest.json.sig";
/// The Actions secret (and environment variable) that holds the signing key.
pub const SIGNING_KEY_ENV: &str = "ARCADE_UPDATE_SIGNING_KEY";

/// Larger manifests are refused before parsing (a 1 GB file is ~6 MB of JSON).
const MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Debug)]
pub enum ReleaseError {
    Io(io::Error),
    Invalid(String),
    Signature(String),
    /// A chunk or file did not match the manifest. Fall back to a full download.
    Mismatch(String),
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid(m) => write!(f, "invalid manifest: {m}"),
            Self::Signature(m) => write!(f, "signature: {m}"),
            Self::Mismatch(m) => write!(f, "verification failed: {m}"),
        }
    }
}

impl std::error::Error for ReleaseError {}

impl From<io::Error> for ReleaseError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn invalid(m: impl Into<String>) -> ReleaseError {
    ReleaseError::Invalid(m.into())
}

// ---------------------------------------------------------------------------
// Manifest

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Nightly,
    Test,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Test => "test",
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "stable" => Ok(Self::Stable),
            "nightly" => Ok(Self::Nightly),
            "test" => Ok(Self::Test),
            _ => Err(format!("unknown channel {s:?} (stable, nightly or test)")),
        }
    }
}

/// One content-defined chunk: its length and BLAKE3 hash. Offsets follow from
/// the order. Serialized as `[length, "hex"]` to keep manifests small.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub len: u32,
    pub hash: [u8; 32],
}

impl Serialize for Chunk {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (self.len, hex(&self.hash)).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Chunk {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (len, h): (u32, String) = Deserialize::deserialize(d)?;
        let hash = unhex32(&h).ok_or_else(|| serde::de::Error::custom("chunk hash must be 64 hex digits"))?;
        Ok(Self { len, hash })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub chunks: Vec<Chunk>,
}

impl FileEntry {
    /// Chunks a file on disk; the entry is named after the file.
    pub fn from_path(path: &Path) -> Result<Self, ReleaseError> {
        let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| invalid(format!("{} has no UTF-8 file name", path.display())))?.to_string();
        let data = std::fs::read(path)?;
        Ok(Self::from_bytes(name, &data))
    }

    pub fn from_bytes(name: String, data: &[u8]) -> Self {
        Self { name, size: data.len() as u64, sha256: hex(&sha2::Sha256::digest(data)), chunks: chunks_of(data) }
    }

    /// Byte range of each chunk in the file, in order.
    pub fn ranges(&self) -> impl Iterator<Item = (Range<u64>, &Chunk)> {
        let mut at = 0u64;
        self.chunks.iter().map(move |c| {
            let r = at..at + u64::from(c.len);
            at = r.end;
            (r, c)
        })
    }

    /// Checks a complete file against the entry (size and SHA-256).
    pub fn verify_file(&self, path: &Path) -> Result<(), ReleaseError> {
        let mut f = File::open(path)?;
        let mut h = sha2::Sha256::new();
        let n = io::copy(&mut f, &mut h)?;
        if n != self.size {
            return Err(ReleaseError::Mismatch(format!("{}: {n} bytes, manifest says {}", self.name, self.size)));
        }
        if hex(&h.finalize()) != self.sha256 {
            return Err(ReleaseError::Mismatch(format!("{}: SHA-256 differs from the manifest", self.name)));
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ReleaseError> {
        let n = &self.name;
        if n.is_empty() || n == "." || n == ".." || n.contains(['/', '\\', '\0']) || n.starts_with('.') {
            return Err(invalid(format!("file name {n:?} is not a plain file name")));
        }
        if self.size > MAX_FILE_BYTES {
            return Err(invalid(format!("{n}: size {} is too large", self.size)));
        }
        if unhex32(&self.sha256).is_none() {
            return Err(invalid(format!("{n}: sha256 must be 64 lowercase hex digits")));
        }
        let mut total = 0u64;
        let last = self.chunks.len().saturating_sub(1);
        for (i, c) in self.chunks.iter().enumerate() {
            if c.len == 0 || c.len > CHUNK_MAX || (i < last && c.len < CHUNK_MIN) {
                return Err(invalid(format!("{n}: chunk {i} has length {}", c.len)));
            }
            total += u64::from(c.len);
        }
        if total != self.size {
            return Err(invalid(format!("{n}: chunks cover {total} bytes, size is {}", self.size)));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chunking {
    pub algorithm: String,
    pub min: u32,
    pub avg: u32,
    pub max: u32,
    pub hash: String,
}

impl Default for Chunking {
    fn default() -> Self {
        Self { algorithm: "fastcdc-2020".into(), min: CHUNK_MIN, avg: CHUNK_AVG, max: CHUNK_MAX, hash: "blake3".into() }
    }
}

/// `manifest.json`. `version` is the product version the files report
/// (`1.0.0`); `tag` is the GitHub release that carries them (`v1.0.0-rc.2`
/// on `test`, `v1.0.0` once promoted). Promotion changes only `tag`,
/// `channel` and the signature; the files are the same bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema: u32,
    pub app: String,
    pub version: String,
    pub tag: String,
    pub channel: Channel,
    pub chunking: Chunking,
    pub files: Vec<FileEntry>,
}

impl ReleaseManifest {
    pub fn build(app: &str, version: &str, tag: &str, channel: Channel, paths: &[PathBuf]) -> Result<Self, ReleaseError> {
        let files = paths.iter().map(|p| FileEntry::from_path(p)).collect::<Result<Vec<_>, _>>()?;
        let m = Self { schema: SCHEMA, app: app.into(), version: version.into(), tag: tag.into(), channel, chunking: Chunking::default(), files };
        m.validate()?;
        Ok(m)
    }

    /// Parses and validates; never panics on hostile input (fuzzed).
    pub fn from_json(bytes: &[u8]) -> Result<Self, ReleaseError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("manifest is too large"));
        }
        let m: Self = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn to_json(&self) -> Vec<u8> {
        let mut v = serde_json::to_vec_pretty(self).expect("manifest serializes");
        v.push(b'\n');
        v
    }

    pub fn file(&self, name: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.name == name)
    }

    fn validate(&self) -> Result<(), ReleaseError> {
        if self.schema != SCHEMA {
            return Err(invalid(format!("schema {} is not supported (expected {SCHEMA})", self.schema)));
        }
        if self.chunking != Chunking::default() {
            return Err(invalid("unsupported chunking parameters"));
        }
        for (field, v) in [("app", &self.app), ("version", &self.version), ("tag", &self.tag)] {
            if v.is_empty() || v.len() > 128 || !v.chars().all(|c| c.is_ascii_graphic()) {
                return Err(invalid(format!("{field} must be 1-128 printable ASCII characters without spaces")));
            }
        }
        let mut names = HashSet::new();
        for f in &self.files {
            f.validate()?;
            if !names.insert(f.name.as_str()) {
                return Err(invalid(format!("{} is listed twice", f.name)));
            }
        }
        Ok(())
    }
}

/// Splits data into FastCDC (2020) chunks with the manifest's parameters.
pub fn chunks_of(data: &[u8]) -> Vec<Chunk> {
    fastcdc::v2020::FastCDC::new(data, CHUNK_MIN, CHUNK_AVG, CHUNK_MAX)
        .map(|c| Chunk { len: c.length as u32, hash: *blake3::hash(&data[c.offset..c.offset + c.length]).as_bytes() })
        .collect()
}

// ---------------------------------------------------------------------------
// Signing (minisign-compatible ed25519)

const SIG_ALG: &[u8; 2] = b"Ed";
const SIG_ALG_PREHASHED: &[u8; 2] = b"ED";
const KDF_NONE: &[u8; 2] = b"\0\0";
const CKSUM_ALG: &[u8; 2] = b"B2";

#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub key_id: [u8; 8],
    key: ed25519_dalek::VerifyingKey,
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.to_base64())
    }
}

impl PublicKey {
    /// Accepts the one-line base64 key (`RW…`) or a whole minisign `.pub` file.
    pub fn parse(text: &str) -> Result<Self, ReleaseError> {
        let line = last_payload_line(text).ok_or_else(|| ReleaseError::Signature("empty public key".into()))?;
        let raw = B64.decode(line).map_err(|_| ReleaseError::Signature("public key is not base64".into()))?;
        if raw.len() != 42 || &raw[..2] != SIG_ALG {
            return Err(ReleaseError::Signature("not a minisign ed25519 public key".into()));
        }
        let key = ed25519_dalek::VerifyingKey::from_bytes(raw[10..42].try_into().expect("32 bytes"))
            .map_err(|_| ReleaseError::Signature("invalid ed25519 public key".into()))?;
        Ok(Self { key_id: raw[2..10].try_into().expect("8 bytes"), key })
    }

    pub fn to_base64(&self) -> String {
        let mut raw = Vec::with_capacity(42);
        raw.extend_from_slice(SIG_ALG);
        raw.extend_from_slice(&self.key_id);
        raw.extend_from_slice(self.key.as_bytes());
        B64.encode(raw)
    }

    /// The `.pub` file minisign writes.
    pub fn to_minisign(&self) -> String {
        format!("untrusted comment: minisign public key {}\n{}\n", key_id_hex(&self.key_id), self.to_base64())
    }
}

pub struct SecretKey {
    key_id: [u8; 8],
    key: ed25519_dalek::SigningKey,
}

impl SecretKey {
    pub fn generate() -> Result<Self, ReleaseError> {
        let mut seed = [0u8; 32];
        let mut key_id = [0u8; 8];
        getrandom::fill(&mut seed).map_err(|e| ReleaseError::Io(io::Error::other(e.to_string())))?;
        getrandom::fill(&mut key_id).map_err(|e| ReleaseError::Io(io::Error::other(e.to_string())))?;
        Ok(Self { key_id, key: ed25519_dalek::SigningKey::from_bytes(&seed) })
    }

    pub fn public(&self) -> PublicKey {
        PublicKey { key_id: self.key_id, key: self.key.verifying_key() }
    }

    /// An unencrypted minisign secret key (`minisign -G -W` writes the same).
    pub fn parse(text: &str) -> Result<Self, ReleaseError> {
        let bad = |m: &str| ReleaseError::Signature(format!("secret key: {m}"));
        let line = last_payload_line(text).ok_or_else(|| bad("empty"))?;
        let raw = B64.decode(line).map_err(|_| bad("not base64"))?;
        if raw.len() != 158 || &raw[..2] != SIG_ALG || &raw[4..6] != CKSUM_ALG {
            return Err(bad("not a minisign ed25519 secret key"));
        }
        if &raw[2..4] != KDF_NONE {
            return Err(bad("password-protected keys are not supported; create it with `arcade-link keygen`"));
        }
        let key_id: [u8; 8] = raw[54..62].try_into().expect("8 bytes");
        let keypair = &raw[62..126];
        if raw[126..158] != secret_checksum(&key_id, keypair) {
            return Err(bad("checksum mismatch"));
        }
        let key = ed25519_dalek::SigningKey::from_keypair_bytes(keypair.try_into().expect("64 bytes")).map_err(|_| bad("public half does not match"))?;
        Ok(Self { key_id, key })
    }

    pub fn to_minisign(&self) -> String {
        let keypair = self.key.to_keypair_bytes();
        let mut raw = Vec::with_capacity(158);
        raw.extend_from_slice(SIG_ALG);
        raw.extend_from_slice(KDF_NONE);
        raw.extend_from_slice(CKSUM_ALG);
        raw.extend_from_slice(&[0u8; 48]);
        raw.extend_from_slice(&self.key_id);
        raw.extend_from_slice(&keypair);
        raw.extend_from_slice(&secret_checksum(&self.key_id, &keypair));
        format!("untrusted comment: minisign secret key (unencrypted)\n{}\n", B64.encode(raw))
    }

    /// A minisign signature file over `message` (prehashed, `ED`).
    pub fn sign(&self, message: &[u8], trusted_comment: &str) -> String {
        assert!(!trusted_comment.contains('\n'), "trusted comment is one line");
        let sig = self.key.sign(&blake2b512(message));
        let mut sig_line = Vec::with_capacity(74);
        sig_line.extend_from_slice(SIG_ALG_PREHASHED);
        sig_line.extend_from_slice(&self.key_id);
        sig_line.extend_from_slice(&sig.to_bytes());
        let mut global = sig.to_bytes().to_vec();
        global.extend_from_slice(trusted_comment.as_bytes());
        let global = self.key.sign(&global);
        format!(
            "untrusted comment: signature from arcade-link secret key\n{}\ntrusted comment: {trusted_comment}\n{}\n",
            B64.encode(sig_line),
            B64.encode(global.to_bytes())
        )
    }
}

fn secret_checksum(key_id: &[u8; 8], keypair: &[u8]) -> [u8; 32] {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(32).expect("32-byte BLAKE2b");
    h.update(SIG_ALG);
    h.update(key_id);
    h.update(keypair);
    let mut out = [0u8; 32];
    h.finalize_variable(&mut out).expect("output size");
    out
}

fn blake2b512(message: &[u8]) -> [u8; 64] {
    blake2::Blake2b512::digest(message).into()
}

fn last_payload_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("untrusted comment:")).next_back()
}

/// Verifies a minisign signature over `message` with any of the trusted keys
/// (current and next, for rotation). Returns the trusted comment.
pub fn verify_signature(message: &[u8], signature: &str, trusted: &[PublicKey]) -> Result<String, ReleaseError> {
    let bad = |m: &str| ReleaseError::Signature(m.to_string());
    let mut lines = signature.lines().map(str::trim_end).filter(|l| !l.is_empty());
    if !lines.next().is_some_and(|l| l.starts_with("untrusted comment:")) {
        return Err(bad("missing untrusted comment"));
    }
    let sig_line = B64.decode(lines.next().ok_or_else(|| bad("missing signature"))?).map_err(|_| bad("signature is not base64"))?;
    let comment = lines.next().and_then(|l| l.strip_prefix("trusted comment: ")).ok_or_else(|| bad("missing trusted comment"))?;
    let global = B64.decode(lines.next().ok_or_else(|| bad("missing global signature"))?).map_err(|_| bad("global signature is not base64"))?;
    if lines.next().is_some() || sig_line.len() != 74 || global.len() != 64 {
        return Err(bad("malformed signature file"));
    }
    let key_id: [u8; 8] = sig_line[2..10].try_into().expect("8 bytes");
    let key = trusted.iter().find(|k| k.key_id == key_id).ok_or_else(|| ReleaseError::Signature(format!("signed by untrusted key {}", key_id_hex(&key_id))))?;
    let sig = ed25519_dalek::Signature::from_bytes(sig_line[10..74].try_into().expect("64 bytes"));
    let signed = match &sig_line[..2] {
        a if a == SIG_ALG_PREHASHED => key.key.verify(&blake2b512(message), &sig),
        a if a == SIG_ALG => key.key.verify(message, &sig),
        _ => return Err(bad("unknown signature algorithm")),
    };
    signed.map_err(|_| bad("signature does not match the manifest"))?;
    let mut global_msg = sig.to_bytes().to_vec();
    global_msg.extend_from_slice(comment.as_bytes());
    key.key
        .verify(&global_msg, &ed25519_dalek::Signature::from_bytes(global[..].try_into().expect("64 bytes")))
        .map_err(|_| bad("trusted comment was altered"))?;
    Ok(comment.to_string())
}

/// The trusted comment binds the signature to one release, so a valid
/// manifest cannot be replayed under another app, tag or channel.
pub fn trusted_comment(m: &ReleaseManifest) -> String {
    format!("app={} version={} tag={} channel={}", m.app, m.version, m.tag, m.channel.as_str())
}

pub fn sign_manifest(m: &ReleaseManifest, key: &SecretKey) -> (Vec<u8>, String) {
    let json = m.to_json();
    let sig = key.sign(&json, &trusted_comment(m));
    (json, sig)
}

/// Parses a manifest whose signature verifies with one of the trusted keys and
/// whose trusted comment matches its contents.
pub fn verify_manifest(json: &[u8], signature: &str, trusted: &[PublicKey]) -> Result<ReleaseManifest, ReleaseError> {
    let comment = verify_signature(json, signature, trusted)?;
    let m = ReleaseManifest::from_json(json)?;
    if comment != trusted_comment(&m) {
        return Err(ReleaseError::Signature("trusted comment does not match the manifest".into()));
    }
    Ok(m)
}

fn key_id_hex(id: &[u8; 8]) -> String {
    id.iter().rev().map(|b| format!("{b:02X}")).collect()
}

// ---------------------------------------------------------------------------
// Delta planning and assembly

/// Where known chunks can be read: any number of local files (the installed
/// version, other apps, a shared chunk store).
#[derive(Debug, Default)]
pub struct ChunkIndex {
    sources: Vec<PathBuf>,
    chunks: HashMap<[u8; 32], (usize, u64, u32)>,
}

impl ChunkIndex {
    /// Chunks a local file and remembers where each chunk lives.
    pub fn add_file(&mut self, path: &Path) -> io::Result<()> {
        let data = std::fs::read(path)?;
        let source = self.sources.len();
        self.sources.push(path.to_path_buf());
        let mut at = 0u64;
        for c in chunks_of(&data) {
            self.chunks.entry(c.hash).or_insert((source, at, c.len));
            at += u64::from(c.len);
        }
        Ok(())
    }

    /// Indexes a file whose chunk list is already known (e.g. from the
    /// manifest it was installed with), without reading it.
    pub fn add_known(&mut self, path: &Path, entry: &FileEntry) {
        let source = self.sources.len();
        self.sources.push(path.to_path_buf());
        for (r, c) in entry.ranges() {
            self.chunks.entry(c.hash).or_insert((source, r.start, c.len));
        }
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Copy `len` bytes at `offset` of local source `source`.
    Have { source: usize, offset: u64, len: u32 },
    /// Bytes `range` of the new file must be downloaded.
    Fetch { range: Range<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaPlan {
    pub pieces: Vec<Piece>,
}

impl DeltaPlan {
    /// The byte ranges to request, adjacent chunks merged.
    pub fn missing_ranges(&self) -> Vec<Range<u64>> {
        self.pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Fetch { range } => Some(range.clone()),
                Piece::Have { .. } => None,
            })
            .collect()
    }

    pub fn download_bytes(&self) -> u64 {
        self.missing_ranges().iter().map(|r| r.end - r.start).sum()
    }
}

/// Which parts of `entry` are already local and which byte ranges to fetch.
pub fn plan_delta(have: &ChunkIndex, entry: &FileEntry) -> DeltaPlan {
    let mut pieces: Vec<Piece> = Vec::new();
    for (r, c) in entry.ranges() {
        match have.chunks.get(&c.hash) {
            Some(&(source, offset, len)) if len == c.len => pieces.push(Piece::Have { source, offset, len }),
            _ => match pieces.last_mut() {
                Some(Piece::Fetch { range }) if range.end == r.start => range.end = r.end,
                _ => pieces.push(Piece::Fetch { range: r }),
            },
        }
    }
    DeltaPlan { pieces }
}

/// Writes the new file to `out`: local chunks are copied, missing ranges come
/// from `fetch` (called once per range, in order). Every chunk is checked
/// against the manifest, and the whole file against its SHA-256. On any error
/// discard `out` and download the full file.
pub fn assemble<W: Write>(
    entry: &FileEntry,
    plan: &DeltaPlan,
    have: &ChunkIndex,
    fetch: &mut dyn FnMut(Range<u64>) -> io::Result<Vec<u8>>,
    out: W,
) -> Result<(), ReleaseError> {
    let mut out = HashingWriter { inner: out, sha: sha2::Sha256::new(), written: 0 };
    let mut expected = entry.ranges().peekable();
    let mut files: HashMap<usize, File> = HashMap::new();
    let mismatch = |m: String| ReleaseError::Mismatch(format!("{}: {m}", entry.name));
    for piece in &plan.pieces {
        let data = match piece {
            Piece::Have { source, offset, len } => {
                let f = match files.entry(*source) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        let path = have.sources.get(*source).ok_or_else(|| mismatch("unknown local source".into()))?;
                        e.insert(File::open(path)?)
                    }
                };
                f.seek(SeekFrom::Start(*offset))?;
                let mut buf = vec![0u8; *len as usize];
                f.read_exact(&mut buf)?;
                buf
            }
            Piece::Fetch { range } => {
                let data = fetch(range.clone())?;
                if data.len() as u64 != range.end - range.start {
                    return Err(mismatch(format!("range {range:?} returned {} bytes", data.len())));
                }
                data
            }
        };
        let mut at = 0usize;
        while at < data.len() {
            let (r, c) = expected.next().ok_or_else(|| mismatch("more data than chunks".into()))?;
            let end = at + c.len as usize;
            if r.start != out.written || end > data.len() {
                return Err(mismatch(format!("chunk at {} does not line up", r.start)));
            }
            if blake3::hash(&data[at..end]).as_bytes() != &c.hash {
                return Err(mismatch(format!("chunk at {} has the wrong hash", r.start)));
            }
            out.write_all(&data[at..end])?;
            at = end;
        }
    }
    if expected.peek().is_some() || out.written != entry.size {
        return Err(mismatch(format!("assembled {} of {} bytes", out.written, entry.size)));
    }
    if hex(&out.sha.finalize_reset()) != entry.sha256 {
        return Err(mismatch("SHA-256 differs from the manifest".into()));
    }
    out.inner.flush()?;
    Ok(())
}

struct HashingWriter<W> {
    inner: W,
    sha: sha2::Sha256,
    written: u64,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.sha.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}
