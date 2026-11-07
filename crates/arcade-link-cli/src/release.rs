//! `release-manifest`, `promote-manifest`, `verify-manifest` and `keygen`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use arcade_link::release::{self, Channel, PublicKey, ReleaseManifest, SecretKey, MANIFEST_NAME, SIGNATURE_NAME, SIGNING_KEY_ENV};

/// Options that take a value, flags, and positional arguments.
struct Args {
    values: Vec<(String, String)>,
    flags: Vec<String>,
    positional: Vec<String>,
}

impl Args {
    fn parse(args: &[String], with_value: &[&str], flags: &[&str]) -> Result<Self, String> {
        let mut out = Args { values: Vec::new(), flags: Vec::new(), positional: Vec::new() };
        let mut it = args.iter().skip(1);
        while let Some(a) = it.next() {
            if with_value.contains(&a.as_str()) {
                let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                out.values.push((a.clone(), v.clone()));
            } else if flags.contains(&a.as_str()) {
                out.flags.push(a.clone());
            } else if a.starts_with("--") {
                return Err(format!("unknown option {a}"));
            } else {
                out.positional.push(a.clone());
            }
        }
        Ok(out)
    }
    fn value(&self, name: &str) -> Option<&str> {
        self.values.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    fn all(&self, name: &str) -> Vec<&str> {
        self.values.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect()
    }
    fn required(&self, name: &str) -> Result<&str, String> {
        self.value(name).ok_or_else(|| format!("{name} is required"))
    }
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }
}

fn signing_key() -> Result<SecretKey, String> {
    let text = std::env::var(SIGNING_KEY_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("{SIGNING_KEY_ENV} is not set; refusing to publish an unsigned manifest"))?;
    SecretKey::parse(&text).map_err(|e| e.to_string())
}

fn write_out(dir: &Path, m: &ReleaseManifest, sign: bool) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = if sign {
        let (json, sig) = release::sign_manifest(m, &signing_key()?);
        std::fs::write(dir.join(SIGNATURE_NAME), sig).map_err(|e| e.to_string())?;
        json
    } else {
        m.to_json()
    };
    std::fs::write(dir.join(MANIFEST_NAME), json).map_err(|e| e.to_string())?;
    let size: u64 = m.files.iter().map(|f| f.size).sum();
    println!("{} {} ({}): {} files, {size} bytes{}", m.app, m.tag, m.channel.as_str(), m.files.len(), if sign { ", signed" } else { "" });
    Ok(())
}

/// `release-manifest --app ID --version V --tag T --channel C [--out DIR] [--sign] FILE…`
pub fn manifest(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["--app", "--version", "--tag", "--channel", "--out"], &["--sign"])?;
    let channel: Channel = a.required("--channel")?.parse()?;
    if a.positional.is_empty() {
        return Err("release-manifest needs at least one file".into());
    }
    let files: Vec<PathBuf> = a.positional.iter().map(PathBuf::from).collect();
    let m = ReleaseManifest::build(a.required("--app")?, a.required("--version")?, a.required("--tag")?, channel, &files).map_err(|e| e.to_string())?;
    write_out(Path::new(a.value("--out").unwrap_or(".")), &m, a.flag("--sign"))
}

/// `promote-manifest RC_MANIFEST --tag T --files DIR [--channel stable|nightly] [--out DIR]`
///
/// Every file in the rc manifest must be in DIR with the same bytes; the new
/// manifest keeps the file entries and is always signed.
pub fn promote(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["--tag", "--files", "--channel", "--out"], &[])?;
    let [rc] = a.positional.as_slice() else {
        return Err("promote-manifest needs exactly one rc manifest".into());
    };
    let rc = ReleaseManifest::from_json(&std::fs::read(rc).map_err(|e| format!("{rc}: {e}"))?).map_err(|e| e.to_string())?;
    let files = Path::new(a.required("--files")?);
    for f in &rc.files {
        f.verify_file(&files.join(&f.name)).map_err(|e| e.to_string())?;
    }
    let channel: Channel = a.value("--channel").unwrap_or("stable").parse()?;
    if channel == Channel::Test {
        return Err("promotion goes to stable or nightly".into());
    }
    let m = ReleaseManifest { tag: a.required("--tag")?.into(), channel, ..rc };
    write_out(Path::new(a.value("--out").unwrap_or(".")), &m, true)
}

/// `verify-manifest MANIFEST --key PUB [--key PUB] [--sig FILE] [--files DIR] [--unsigned]`
pub fn verify(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["--key", "--sig", "--files"], &["--unsigned"])?;
    let [path] = a.positional.as_slice() else {
        return Err("verify-manifest needs exactly one manifest".into());
    };
    let json = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let m = if a.flag("--unsigned") {
        ReleaseManifest::from_json(&json).map_err(|e| e.to_string())?
    } else {
        let keys = a.all("--key").into_iter().map(PublicKey::parse).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
        if keys.is_empty() {
            return Err("verify-manifest needs --key (or --unsigned)".into());
        }
        let sig_path = a.value("--sig").map(PathBuf::from).unwrap_or_else(|| Path::new(path).with_file_name(SIGNATURE_NAME));
        let sig = std::fs::read_to_string(&sig_path).map_err(|e| format!("{}: {e}", sig_path.display()))?;
        release::verify_manifest(&json, &sig, &keys).map_err(|e| e.to_string())?
    };
    if let Some(dir) = a.value("--files") {
        for f in &m.files {
            f.verify_file(&Path::new(dir).join(&f.name)).map_err(|e| e.to_string())?;
        }
    }
    println!("{} {} {} ({}): ok, {} files", m.app, m.version, m.tag, m.channel.as_str(), m.files.len());
    Ok(())
}

/// `keygen --secret-to-gh OWNER/REPO…` stores a new private key only as the
/// `ARCADE_UPDATE_SIGNING_KEY` Actions secret of each repo (through `gh`) and
/// prints the public key. `--print-secret` prints it instead (tests only).
pub fn keygen(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["--secret-to-gh"], &["--print-secret"])?;
    let repos = a.all("--secret-to-gh");
    if repos.is_empty() && !a.flag("--print-secret") {
        return Err("keygen needs --secret-to-gh OWNER/REPO (repeatable); the private key is never written to disk".into());
    }
    let key = SecretKey::generate().map_err(|e| e.to_string())?;
    let secret = key.to_minisign();
    print!("{}", key.public().to_minisign());
    if a.flag("--print-secret") {
        print!("{secret}");
    }
    for repo in &repos {
        let mut child =
            Command::new("gh").args(["secret", "set", SIGNING_KEY_ENV, "--repo", repo]).stdin(Stdio::piped()).spawn().map_err(|e| format!("gh: {e}"))?;
        child.stdin.take().expect("piped").write_all(secret.as_bytes()).map_err(|e| e.to_string())?;
        if !child.wait().map_err(|e| e.to_string())?.success() {
            return Err(format!("gh secret set failed for {repo}"));
        }
    }
    Ok(())
}
