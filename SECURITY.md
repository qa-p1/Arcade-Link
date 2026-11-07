# Security policy

## Reporting

Report a vulnerability privately through GitHub's "Report a vulnerability" (Security tab of this repository), not in
a public issue. Expect an answer within a week. Fixes ship as a patch release of Arcade Link and of every app that
pins it, through the normal rc → promote path (target: under a day for a serious issue).

## What Arcade Link guards, and how

| Surface | Protection | Tested by |
|---|---|---|
| Link IPC sockets (Linux/macOS) | The socket is 0600 inside a 0700 per-user folder; on accept, the peer's UID (`SO_PEERCRED` / `getpeereid`) must equal ours or the connection is dropped before it is read. Every connection must present the endpoint's random 256-bit token in `hello` (compared in constant time). | `tests/peer_uid.rs` (as root in CI), `tests/link.rs` |
| Link IPC pipes (Windows) | Security descriptor admits only the owner and SYSTEM; token as above. | `tests/link.rs` on Windows CI |
| Wire input | Lines are size-limited and parsed as JSON-RPC; unknown methods answer `unsupported_method`. | `tests/link.rs`, `tests/methods.rs` |
| Release manifests | ed25519 signatures (minisign format) bound to app, version, tag and channel; several trusted keys for rotation; strict parser (fixed chunk parameters, plain file names, sizes add up, no unknown fields); every chunk BLAKE3 and every file SHA-256 checked; any mismatch means a full download. The private key exists only as an Actions secret. | `tests/release.rs` (tamper, reorder, wrong key, truncation, replay, mutation fuzzing), `fuzz/`, CI minisign interop |
| Child processes | Environment sanitized (`childenv`), AppImage descriptors not inherited, children reaped. | `tests/runtime.rs`, `childenv` tests |
| Crash reports | Written locally only (`arcade/<app>/crashes`), private files, newest 20 kept; never uploaded. | `tests/runtime.rs` |

## Dependencies

`cargo deny check` (advisories, licences, bans, sources) runs in CI; Dependabot watches Cargo and GitHub Actions.
Accepted advisories are listed with a reason in `deny.toml`.
