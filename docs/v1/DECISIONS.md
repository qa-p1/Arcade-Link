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

## D6 — Shared runtime: inventory and what moved into Link

Inventory of the per-app copies (2026-11-07, main of each repo):

| Piece | Already in Link | Per-app copies found | Link v1 |
|---|---|---|---|
| Environment sanitizing, mount handles | `childenv` | none left (all call `childenv`) | kept |
| Restart via `$APPIMAGE` | `manifest::current_executable` | Box `lib.rs:1929` restart + `wait_for_predecessor`; Look, Find, Lens, Tools each restart in their own way | `runtime::spawn_successor` / `wait_for_predecessor` (RT-2) |
| Single instance | — | Find `instance.rs:113` (fs2 flock), Tools overlay `instance.rs:126` | `runtime::acquire_instance[_waiting]` (RT-1) |
| Reaped children | — | none; Lens leaks zombies (LN-3) | `runtime::spawn_reaped` / `reap` (RT-3) |
| Crash reports | — | none (panics go to stderr logs only) | `runtime::install_crash_reports` (RT-4): panic hook + async-signal-safe handler for SIGSEGV/BUS/ILL/FPE/ABRT, newest 20 kept under `<data>/crashes/` |
| Memory trim (W0/W1/W3b) | — | Tools `resident/memory.rs` (malloc_trim + PAGEOUT of `.so`, measured ineffective), Box `lib.rs:1511` (malloc_trim only) | `runtime::trim`, `release_webkit`, `page_out_hidden` (RT-5); C twin `qt/arcade_trim.h`; OPEN1 preload `packaging/linux/arcade-trim-preload.c` |
| Logs | `logs` (stderr capture) | each app filters its own toolkit noise | stays; level/rotation/noise filter tracked in Link `logs` (see D7) |
| Tray host/client | `trayhost` | — | kept |
| XDG paths + legacy retirement | `paths::migrate_app_dirs` | — | kept |

Instance-socket command forwarding (Find's `instance::serve`) stays in the app: it is app protocol, and Link's
own endpoint already serves activation.

The crash signal handler chains to the previous handler and then forces the default action, because Rust's own
SIGSEGV handler returns (to re-fault) for anything that isn't a stack overflow.

## D7 — Template lockfile and Link pin

The template pins Arcade Link by `tag = "v1.0.0"` (LINK-2), which cannot resolve before that tag exists, and
`[patch]` still fetches the pinned source. Link CI therefore swaps the line for a path (`template/use-local-link.sh`)
before building; the committed `Cargo.toml` keeps the tag. `template/Cargo.lock` is committed right after `v1.0.0`
is tagged, so a repo made from the template builds with `--locked`.

## D8 — Design kit: emoji fallback; contrast pairs were already checked

`design/generate.py --check` already enforces WCAG AA (4.5:1) on every text/surface pair and the accent pairs in
both themes, and CI runs it, so nothing new was needed for the "a11y contrast pairs" item. For CB-10 the tokens gain
`type.family.emoji` (Noto Color Emoji, Apple Color Emoji, Segoe UI Emoji, Twemoji Mozilla): the CSS sans stack ends
with them and every Dart text style carries them as `fontFamilyFallback`. Clipboard still has to bundle or depend on
a colour emoji font on Linux (its spec).

## D9 — Dead code and the untangle pass (Link)

Dead code was judged against every consumer, not just this repo: the public Rust items were grepped across Link and
the six Rust apps (Tools, Box, Look, Find, Lens, Clipboard's core). Removed as unused anywhere: `Registry::offers_for`,
`childenv::sanitized_vars`, `logs::stderr_is_terminal`, `Action::takes_no_input`, `Action::offer_for_type`.
`cargo machete` finds no unused dependencies. Kept although unused in Rust: the generated design tokens
(`design/tokens.rs`), which mirror the CSS, Dart and QML kits one to one; and the v1 APIs added for the app specs.

Untangle findings (module graph in this file's history; skill steps 2–5):
- `paths` uses `manifest::ids` for the known-app list and `accelerator::Platform`; `runtime` uses
  `manifest::current_executable`. Kept for 1.0: moving `current_executable` would rename a call every Rust app makes
  while apps ship one at a time on mixed Link versions. Logged for 1.1.
- `ManifestAdditions` is a v0.3 source-compatibility split of `Manifest`. Kept for the same reason; merging it is a
  breaking change for every app's struct literals.
- `server` calls `client::probe` to refuse a second server for a live app id: a real dependency (one instance per id).
- New v1 code (release, runtime, conformance, desktop driver) was written without restating comments; the release
  tests exercise behaviour (tamper, replay, delta) rather than internals.

## D10 — Headless Hyprland in CI: 0.56.1 from its flake, cached

The owner runs 0.56.x. Hyprland's flake at `v0.56.2` does not build (its CMake ignores the flake's glaze 8.0 and
tries to download glaze 7.2 without network), and neither 0.56.x tag is in hyprland.cachix.org, so CI builds
`v0.56.1` from its flake once (about 25 minutes) and keeps the Nix store in the Actions cache
(`nix-community/cache-nix-action`); later runs restore it in about a minute. The harness boots it with
`HYPRLAND_HEADLESS_ONLY=1` (as Hyprland's own `hyprtester` does) and two virtual outputs (multi-monitor tests).
Tier 2 (sway, KWin, Mutter, X11) uses Ubuntu packages and was green on the first run.

## D11 — Untangle extras on Link's existing code

- **Comments:** a scan of every Rust, C++ and Python source for comments that restate the next line found one
  candidate (`qt/ArcadeTrayHost.cpp:197`), which explains an ordering requirement and stays. The remaining comments
  describe external facts (Windows pipe semantics, inotify feedback loops, AppImage mounts), which the skill keeps.
- **Tests:** the 70+ unit tests and the integration suites are named for and assert behaviour (contrast on rendered
  pixels, migration idempotence, handle inheritance across a real exec, socket path limits); none repeats the
  implementation or cannot fail, so none was removed. The new suites were checked to fail without their fix
  (peer UID, QML Escape) or are adversarial by construction (release manifests).
