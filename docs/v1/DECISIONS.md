# Arcade Link v1 decisions

Decisions the plan left open, made by the v1 session. Each one cites the part of `dev/v1/README.md` it follows.
Newest last.

## D1 — Signing secret cannot be created from this session (blocker)

The session has no GitHub credential that can write Actions secrets: the `gh` token is rejected
(`gh auth status`: "The token in GH_TOKEN is invalid") and the GitHub tools available here cannot manage
secrets. A workflow's `GITHUB_TOKEN` cannot write secrets either.

So no `ARCADE_UPDATE_SIGNING_KEY` keypair was generated: a private key that cannot be stored as a secret would
have to live somewhere else, which the owner forbade. Consequences, following "never publish unsigned stable":
- `release-manifest` signing, verification and the reusable `promote.yml` are built and tested with throwaway test
  keys generated inside each test run.
- `promote.yml` fails when the secret is missing, so no stable promotion of Tools or an app can happen until the
  owner runs (once, on their machine):
  `arcade-link keygen --secret-to-gh qa-p1/<repo>...` (prints only the public key) — see `docs/SIGNING.md`.
- Link itself is a source library released by tag; it has no update manifest, so the blocker does not hold back the
  Link `v1.0.0` tag.

## D2 — Template repo lives in Link until it can be created

`specs/LINK.md` §5.3 asks for a new GitHub repo `qa-p1/Arcade-template`. This session may not create repositories.
The template is kept in this repo under `template/`, built and checked by Link CI (build, tests, conformance). Creating
the GitHub repo from it is a copy of that folder plus "Template repository" in Settings; it is listed as an owner
action in `dev/v1/PROGRESS.md`.

## D3 — Branch and merge flow

All work is developed on `claude/tender-feynman-3vt7dg` and lands on `main` through PRs that this session merges once
CI is green (PLAN.md "Full autonomy"). `main` is never force-pushed.

## D4 — Release manifest format and promotion

- `manifest.json` lists `version` (the product version the files report, `1.0.0`) separately from `tag` (the
  release carrying them: `v1.0.0-rc.2`, then `v1.0.0`). rc builds are built as the final version, so stable is the
  rc's exact bytes; promotion writes a new manifest with the same file entries and a new `tag`/`channel`, and signs it.
  `promote-manifest` refuses unless every file matches the rc manifest byte for byte.
- Chunks are serialized as `[length, "blake3hex"]`; offsets follow from order. Chunk parameters are fixed
  (FastCDC 2020, 8/16/32 KB) and validated, so a manifest cannot make the updater use other boundaries.
- Signatures are minisign's prehashed `ED` format, verified in CI by the real `minisign`. The signing key in the
  secret is an unencrypted minisign secret key (`minisign -G -W` keys work too). The trusted comment names app,
  version, tag and channel, and must match the manifest, preventing replay across releases.
- Test-channel manifests are unsigned (`specs/LINK.md` §4). Nightly and stable are signed.

## D5 — Protocol version already exists; legacy hello accepted as protocol 1

`specs/LINK.md` §2 assumes 0.4.x peers send no protocol version. They do: the Rust client, the Qt kit and Shelf's
own client all send `"protocol": [1]` in `hello`, and servers already negotiate and answer `version_mismatch` for
unknown versions. So the existing negotiation is kept. Added, as the spec asks: a `hello` with no `protocol` field is
treated as protocol 1 (legacy) instead of being refused, and an N-1 job runs the previous Link release's CLI and mock
against the new ones in both directions.
