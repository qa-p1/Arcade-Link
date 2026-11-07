#![cfg(feature = "release")]

use std::io;
use std::path::{Path, PathBuf};

use arcade_link::release::*;

fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arcade-release-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}

fn manifest(dir: &Path, data: &[u8]) -> ReleaseManifest {
    let p = write(dir, "App-x86_64.AppImage", data);
    ReleaseManifest::build("arcade.test", "1.0.0", "v1.0.0-rc.1", Channel::Test, &[p]).unwrap()
}

/// Old release, and a new one with an insertion and a changed block in the middle.
fn versions() -> (Vec<u8>, Vec<u8>) {
    let old = noise(1, 2_000_000);
    let mut new = old.clone();
    new.splice(700_000..700_000, noise(2, 5_000));
    new[1_500_000..1_520_000].copy_from_slice(&noise(3, 20_000));
    (old, new)
}

#[test]
fn chunks_respect_the_size_bounds_and_cover_the_file() {
    let data = noise(4, 1_000_000);
    let e = FileEntry::from_bytes("f".into(), &data);
    let total: u64 = e.chunks.iter().map(|c| u64::from(c.len)).sum();
    assert_eq!(total, data.len() as u64);
    let last = e.chunks.len() - 1;
    for (i, c) in e.chunks.iter().enumerate() {
        assert!(c.len <= CHUNK_MAX);
        assert!(i == last || c.len >= CHUNK_MIN);
    }
    let avg = data.len() / e.chunks.len();
    assert!((12_000..24_000).contains(&avg), "average chunk {avg}");
}

#[test]
fn signed_manifest_round_trips_and_minisign_text_parses_back() {
    let dir = tmp("roundtrip");
    let m = manifest(&dir, &noise(5, 300_000));
    let key = SecretKey::generate().unwrap();
    let key = SecretKey::parse(&key.to_minisign()).unwrap();
    let public = PublicKey::parse(&key.public().to_minisign()).unwrap();
    let (json, sig) = sign_manifest(&m, &key);
    assert_eq!(verify_manifest(&json, &sig, &[public]).unwrap(), m);
}

#[test]
fn a_rotated_next_key_is_accepted_and_an_unknown_key_is_not() {
    let dir = tmp("rotation");
    let m = manifest(&dir, &noise(6, 100_000));
    let (current, next, stranger) = (SecretKey::generate().unwrap(), SecretKey::generate().unwrap(), SecretKey::generate().unwrap());
    let trusted = [current.public(), next.public()];
    let (json, sig) = sign_manifest(&m, &next);
    verify_manifest(&json, &sig, &trusted).unwrap();
    let (json, sig) = sign_manifest(&m, &stranger);
    let e = verify_manifest(&json, &sig, &trusted).unwrap_err();
    assert!(matches!(e, ReleaseError::Signature(ref m) if m.contains("untrusted key")), "{e}");
}

#[test]
fn any_change_to_the_signed_bytes_is_rejected() {
    let dir = tmp("tamper");
    let mut m = manifest(&dir, &noise(7, 100_000));
    m.files.push(FileEntry::from_bytes("Other.zip".into(), &noise(8, 50_000)));
    let key = SecretKey::generate().unwrap();
    let trusted = [key.public()];
    let (json, sig) = sign_manifest(&m, &key);

    let mut flipped = json.clone();
    let i = flipped.len() / 2;
    flipped[i] ^= 1;
    assert!(verify_manifest(&flipped, &sig, &trusted).is_err());

    let mut reordered = m.clone();
    reordered.files.reverse();
    assert!(verify_manifest(&reordered.to_json(), &sig, &trusted).is_err());

    let mut promoted = m.clone();
    promoted.channel = Channel::Stable;
    assert!(verify_manifest(&promoted.to_json(), &sig, &trusted).is_err());

    let comment = sig.lines().nth(2).unwrap().to_string();
    let forged = sig.replace(&comment, "trusted comment: app=arcade.test version=9.9.9 tag=v9.9.9 channel=stable");
    assert!(verify_manifest(&json, &forged, &trusted).is_err());

    assert!(verify_manifest(&json[..json.len() - 10], &sig, &trusted).is_err());
}

#[test]
fn the_trusted_comment_must_describe_the_signed_manifest() {
    let dir = tmp("comment");
    let m = manifest(&dir, &noise(9, 10_000));
    let key = SecretKey::generate().unwrap();
    let json = m.to_json();
    let sig = key.sign(&json, "app=arcade.test version=1.0.0 tag=v1.0.0 channel=stable");
    let e = verify_manifest(&json, &sig, &[key.public()]).unwrap_err();
    assert!(e.to_string().contains("does not match"), "{e}");
}

#[test]
fn delta_downloads_only_changed_chunks_and_rebuilds_the_exact_file() {
    let dir = tmp("delta");
    let (old, new) = versions();
    let old_path = write(&dir, "old.AppImage", &old);
    let entry = FileEntry::from_bytes("new.AppImage".into(), &new);
    let mut have = ChunkIndex::default();
    have.add_file(&old_path).unwrap();
    let plan = plan_delta(&have, &entry);
    let fetched = plan.download_bytes();
    assert!(fetched > 0 && fetched < 200_000, "fetched {fetched} of {}", new.len());

    let mut calls = 0;
    let mut out = Vec::new();
    assemble(
        &entry,
        &plan,
        &have,
        &mut |r| {
            calls += 1;
            Ok(new[r.start as usize..r.end as usize].to_vec())
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(out, new);
    assert_eq!(calls, plan.missing_ranges().len());
}

#[test]
fn known_chunk_lists_index_a_file_without_rereading_it() {
    let dir = tmp("known");
    let (old, new) = versions();
    let old_path = write(&dir, "old.AppImage", &old);
    let mut by_read = ChunkIndex::default();
    by_read.add_file(&old_path).unwrap();
    let mut by_manifest = ChunkIndex::default();
    by_manifest.add_known(&old_path, &FileEntry::from_bytes("old.AppImage".into(), &old));
    let entry = FileEntry::from_bytes("new.AppImage".into(), &new);
    assert_eq!(plan_delta(&by_read, &entry), plan_delta(&by_manifest, &entry));
}

#[test]
fn a_tampered_downloaded_chunk_is_rejected() {
    let dir = tmp("bad-chunk");
    let (old, new) = versions();
    let mut have = ChunkIndex::default();
    have.add_file(&write(&dir, "old", &old)).unwrap();
    let entry = FileEntry::from_bytes("new".into(), &new);
    let plan = plan_delta(&have, &entry);
    let e = assemble(
        &entry,
        &plan,
        &have,
        &mut |r| {
            let mut d = new[r.start as usize..r.end as usize].to_vec();
            d[0] ^= 0xff;
            Ok(d)
        },
        io::sink(),
    )
    .unwrap_err();
    assert!(matches!(e, ReleaseError::Mismatch(_)), "{e}");
}

#[test]
fn a_short_range_response_is_rejected() {
    let dir = tmp("short");
    let (old, new) = versions();
    let mut have = ChunkIndex::default();
    have.add_file(&write(&dir, "old", &old)).unwrap();
    let entry = FileEntry::from_bytes("new".into(), &new);
    let plan = plan_delta(&have, &entry);
    let e = assemble(&entry, &plan, &have, &mut |r| Ok(new[r.start as usize..r.end as usize - 1].to_vec()), io::sink()).unwrap_err();
    assert!(matches!(e, ReleaseError::Mismatch(_)), "{e}");
}

#[test]
fn a_local_file_changed_after_indexing_is_detected() {
    let dir = tmp("local-changed");
    let (old, new) = versions();
    let old_path = write(&dir, "old", &old);
    let mut have = ChunkIndex::default();
    have.add_file(&old_path).unwrap();
    std::fs::write(&old_path, noise(10, old.len())).unwrap();
    let entry = FileEntry::from_bytes("new".into(), &new);
    let plan = plan_delta(&have, &entry);
    let e = assemble(&entry, &plan, &have, &mut |r| Ok(new[r.start as usize..r.end as usize].to_vec()), io::sink()).unwrap_err();
    assert!(matches!(e, ReleaseError::Mismatch(_)), "{e}");
}

#[test]
fn a_truncated_or_altered_full_download_fails_verification() {
    let dir = tmp("full");
    let data = noise(11, 200_000);
    let entry = FileEntry::from_bytes("f".into(), &data);
    let p = write(&dir, "f", &data);
    entry.verify_file(&p).unwrap();
    std::fs::write(&p, &data[..data.len() - 1]).unwrap();
    assert!(matches!(entry.verify_file(&p), Err(ReleaseError::Mismatch(_))));
    let mut altered = data.clone();
    altered[5] ^= 1;
    std::fs::write(&p, &altered).unwrap();
    assert!(matches!(entry.verify_file(&p), Err(ReleaseError::Mismatch(_))));
}

#[test]
fn the_parser_refuses_unsafe_or_inconsistent_manifests() {
    let dir = tmp("parse");
    let good = manifest(&dir, &noise(12, 100_000));
    ReleaseManifest::from_json(&good.to_json()).unwrap();
    let cases: Vec<fn(&mut ReleaseManifest)> = vec![
        |m| m.files[0].name = "../evil".into(),
        |m| m.files[0].name = "dir/evil".into(),
        |m| m.files[0].name = ".hidden".into(),
        |m| m.files[0].size += 1,
        |m| m.files[0].chunks[0].len = CHUNK_MAX + 1,
        |m| m.files[0].chunks[0].len = 10,
        |m| m.files[0].sha256 = "00".into(),
        |m| m.schema = 2,
        |m| m.chunking.avg = 4096,
        |m| m.tag = "v1 channel=stable".into(),
        |m| {
            let f = m.files[0].clone();
            m.files.push(f)
        },
    ];
    for (i, break_it) in cases.iter().enumerate() {
        let mut m = good.clone();
        break_it(&mut m);
        assert!(ReleaseManifest::from_json(&m.to_json()).is_err(), "case {i} was accepted");
    }
    let mut extra: serde_json::Value = serde_json::from_slice(&good.to_json()).unwrap();
    extra["files"][0]["url"] = "https://example.invalid".into();
    assert!(ReleaseManifest::from_json(extra.to_string().as_bytes()).is_err());
}

/// Cheap, always-on fuzzing; `fuzz/` runs the long version.
#[test]
fn mutated_manifests_and_signatures_never_panic() {
    let dir = tmp("mutate");
    let m = manifest(&dir, &noise(13, 60_000));
    let key = SecretKey::generate().unwrap();
    let (json, sig) = sign_manifest(&m, &key);
    let trusted = [key.public()];
    let mut rng = noise(14, 200_000).into_iter();
    for _ in 0..2_000 {
        let mut j = json.clone();
        let mut s = sig.clone().into_bytes();
        for _ in 0..1 + rng.next().unwrap() % 4 {
            let target = if rng.next().unwrap() % 3 == 0 { &mut s } else { &mut j };
            let at = (usize::from(rng.next().unwrap()) << 8 | usize::from(rng.next().unwrap())) % (target.len() + 1);
            match rng.next().unwrap() % 3 {
                0 if at < target.len() => target[at] = rng.next().unwrap(),
                1 => target.truncate(at),
                _ => target.insert(at, rng.next().unwrap()),
            }
        }
        let _ = ReleaseManifest::from_json(&j);
        let _ = verify_manifest(&j, &String::from_utf8_lossy(&s), &trusted);
    }
}

/// The real `minisign` verifies our signatures. Required in CI
/// (`ARCADE_REQUIRE_MINISIGN=1`), skipped locally when it isn't installed.
#[test]
fn minisign_verifies_our_signature() {
    let found = std::process::Command::new("minisign").arg("-v").output().is_ok();
    if !found {
        assert!(std::env::var_os("ARCADE_REQUIRE_MINISIGN").is_none(), "minisign is required but missing");
        eprintln!("minisign not installed; skipping");
        return;
    }
    let dir = tmp("minisign");
    let m = manifest(&dir, &noise(15, 50_000));
    let key = SecretKey::generate().unwrap();
    let (json, sig) = sign_manifest(&m, &key);
    write(&dir, MANIFEST_NAME, &json);
    write(&dir, SIGNATURE_NAME, sig.as_bytes());
    let run = |pubkey: String| {
        std::process::Command::new("minisign")
            .args(["-V", "-q", "-m"])
            .arg(dir.join(MANIFEST_NAME))
            .arg("-x")
            .arg(dir.join(SIGNATURE_NAME))
            .arg("-P")
            .arg(pubkey)
            .status()
            .unwrap()
    };
    assert!(run(key.public().to_base64()).success());
    assert!(!run(SecretKey::generate().unwrap().public().to_base64()).success());
}
