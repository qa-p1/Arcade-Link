# Update signing

Every stable and nightly release carries `manifest.json` (per file: size, SHA-256, FastCDC chunk list with BLAKE3
hashes) and `manifest.json.sig`, an ed25519 signature in minisign's format. Tools trusts only files and chunks the
signed manifest covers; the GitHub release SHA-256 alone is not trusted. Test-channel (`-rc.N`) manifests are
unsigned and Tools never offers them.

The signature's trusted comment is `app=<id> version=<v> tag=<tag> channel=<channel>`, and verification requires it
to match the manifest, so a signed manifest cannot be replayed for another app, tag or channel.

## The key

- The private key exists only as the `ARCADE_UPDATE_SIGNING_KEY` Actions secret of each app repo. `promote.yml` and
  `nightly.yml` read it; without it they fail and publish nothing.
- Tools embeds the public keys it trusts: a current and a next key, so the key can be rotated.

Create it once, on a machine where `gh` is logged in as an owner of the repos:

```bash
arcade-link keygen --secret-to-gh qa-p1/Arcade-tools --secret-to-gh qa-p1/Arcade-box --secret-to-gh qa-p1/Arcade-clipboard --secret-to-gh qa-p1/Arcade-look --secret-to-gh qa-p1/Arcade-Find --secret-to-gh qa-p1/Arcade-lens --secret-to-gh qa-p1/Arcade-Shelf --secret-to-gh qa-p1/Arcade-wheel
```

It prints the public key (`untrusted comment: minisign public key …` and an `RW…` line) and pipes the private key
straight into `gh secret set`; it is never written to disk. Commit the printed public key to Tools as the current
key.

## Rotation

Generate the next key the same way into a scratch repo you control, commit its public key to Tools as the *next*
key, release Tools, and once that Tools is everywhere set the new private key on the app repos. The old key is
removed from Tools one release later.

## Checking a release by hand

```bash
minisign -Vm manifest.json -x manifest.json.sig -P RW...
```

```bash
arcade-link verify-manifest manifest.json --key RW... --files ./downloaded-assets
```
