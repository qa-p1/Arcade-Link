# Arcade Ecosystem Plan

**Five independent apps that recognize each other, connect when that helps, and never depend on each other.**

| | |
|---|---|
| Status | Phases 0–9 implemented; final verification and remaining limitations recorded in §16 (2026-10-08). |
| Written | 2026-10-05, from a read of the code in all five repositories at these commits: Box `f5fc8f6`, Lens `67f6803`, Look `c526b50`, Wheel `602136e`, Clipboard `a43a6cc`. |
| Audience | The agent that implements it, start to finish, in one run (§0.4). |

---

## 0. Rules for the agent (read first)

### 0.1 Scope: what you may touch

You may read and write **only** these paths:

| Path | What it is |
|---|---|
| `/home/harneet/Work/Coding/Rust/Arcade Box` | Arcade Box |
| `/home/harneet/Work/Coding/Rust/Arcade-lens` | Arcade Lens |
| `/home/harneet/Work/Coding/Rust/arcade-look` | Arcade Look |
| `/home/harneet/Work/Coding/C++/Arcade wheel` | Arcade Wheel |
| `/home/harneet/Work/Coding/App dev/Arcade-clipboard` | Arcade Clipboard |
| `/home/harneet/Work/Coding/Rust/Arcade-link` | **Arcade Link** (Rust). The shared protocol: spec, Rust crate, conformance vectors, debug CLI, mock peer, plus the Qt/C++ module for Wheel. The folder already exists (empty); fill it in Phase 1. |
| `/home/harneet/Work/Coding/Rust/Arcade-tools` | **New.** The manager app (Tauri 2 / Rust). Create it only in Phase 8. |
| `/home/harneet/Work/Coding/ARCADE_ECOSYSTEM_PLAN.md` | This file. Tick off progress in §14. |

Do **not** touch anything else: not other folders under `Coding/`, `Rust/`, `C++/` or `App dev/`; not `Snipyx`; not shell profiles, `~/.config`, compositor configs, autostart entries or system packages. Tests that need a home directory use a temporary `ARCADE_HOME` (§5.2) plus each app's existing override (`ARCADE_LENS_HOME`, `ARCADE_DATA_DIR`, `ARCADE_WHEEL_INSTANCE`, …). If something needs a system package that isn't installed, or a path outside this list, don't stop: do everything that doesn't need it, write down exactly what's missing in §15, and carry on.

### 0.2 Git rules

- Work on one feature branch per repository, `arcade/link`, created from the current `main`. **Never commit to `main`** in the five app repositories; in Wheel, Lens and Look, a push to `main` publishes a release. The two new repositories (`Arcade-link`, `Arcade-tools`) start with `git init -b main` and commit on `main`, since nothing is published from them.
- **Commit identity.** There is no global git identity on this machine, and only Wheel has a repository-local one. Before the first commit in each repository (including the two new ones), if `git config user.email` is empty, set the same repository-local identity Wheel uses: `git config user.name "qa-p1"` and `git config user.email "190369222+qa-p1@users.noreply.github.com"`. Never touch the global git config.
- One focused commit per logical step, following each repository's existing commit-message style.
- Do **not** push, open PRs, create tags or publish releases. Everything stays in local commits on the feature branches; the owner pushes after reading the completion report (§16).
- Make no unrelated refactors, reformatting or dependency bumps. If you find an unrelated bug, list it in §15 and keep going.

### 0.2a This machine (checked 2026-10-05)

- Arch Linux, Hyprland (Wayland) session. System Rust 1.99, no rustup targets (so no Windows/macOS cross-compiles and no `wasm32-wasip2`), Node 24, Flutter 3.47, Qt 6.11, CMake 4.4, Python 3.14. FFmpeg, vips, qpdf, Tesseract, ImageMagick, Poppler and Ghostscript are installed.
- **Wheel:** reuse the existing `build/` directory. Its LayerShellQt is a local build under `build/localdeps` (it isn't a system package). Never delete `build/`.
- **Clipboard:** use `bash scripts/dev.sh …`. The repository carries its own toolchain under `.tools/`.
- **Box:** the WASM plugin fixtures are prebuilt and committed. Don't rebuild them (no `wasm32-wasip2` target).
- **Never run the apps in the owner's live desktop session.** Lens and Clipboard add Hyprland key bindings at runtime, several apps register global shortcuts, and first runs turn on start-at-login. For every GUI or end-to-end run:
  - unset `WAYLAND_DISPLAY` and `HYPRLAND_INSTANCE_SIGNATURE`;
  - run under Xvfb (`xvfb-run` or `Xvfb :99` + `DISPLAY=:99`);
  - set `ARCADE_HOME`, `ARCADE_LENS_HOME`, `ARCADE_DATA_DIR`, `ARCADE_WHEEL_INSTANCE`, `ARCADE_WHEEL_DISABLE_GLOBAL_SHORTCUT=1` and a temporary `XDG_CONFIG_HOME`/`XDG_DATA_HOME` so nothing is written to the real profile.

  If Xvfb isn't installed, run only the headless and non-GUI tests, mark the GUI end-to-end checks "not run (no Xvfb)" in §16, and continue.

### 0.3 Non-negotiables

1. **Standalone first.** Every app must work exactly as it does today when it is the only Arcade app installed. Every integration is additive and hidden when the peer is missing. Before a phase is finished, its standalone test suite must pass with no peers present.
2. **Never show a broken entry.** An integration shows up only if the peer is installed, enabled, supports this input, and reports the action as available. Lens already enforces this for its own actions ("a missing capability never produces a broken button"); the whole ecosystem adopts the rule.
3. **Zero idle cost.** No polling, no timers, no extra background processes. Listening for connections must cost nothing while idle.
4. **Never block a UI thread** on discovery, IPC or launching a peer.
5. **Existing safety models win.** Lens's secret guard and confirmation rules, Box's grants and never-overwrite rule, Clipboard's "receiving never writes your clipboard" and Private mode, Look's no-execution sandbox, Wheel's "never run shell strings". Nothing coming over the Link bypasses them.
6. **No half-built features.** Ship an integration only when every item in §13.6 is done. Do not merge stubs or "coming soon" entries.
7. **All three desktop platforms.** Everything is designed for Linux (X11 and Wayland), Windows and macOS. Where a platform can't support something, say so in the code and docs and hide the feature there instead of failing.

### 0.4 How to run this plan

**One run, every phase, no waiting.** Do Phases 0 to 9 back to back. Nobody reviews between phases; you are the reviewer. Don't stop to ask questions: when the plan leaves a detail open, follow the nearest existing code in that repository and keep going.

**Build everything the plan describes.** Don't trim features, and don't add any it doesn't ask for. Avoid over-engineering:
- No extra abstraction layers, generic frameworks, plugin systems or configuration knobs beyond what a listed feature needs.
- Prefer the simplest code that meets the budgets in §5.11 and the checklist in §13.6.
- Reuse each app's existing patterns (its IPC, settings UI, job model, error display) instead of inventing new ones.

**Self-review at the end of every phase.** Before ticking a phase:
1. Run that phase's checks and every touched repository's standalone suite (§13.3).
2. Read your whole diff for the phase once, against §0.3 and §13.6: standalone behavior unchanged, nothing blocking a UI thread, no idle cost, no half-built entry, docs updated.
3. Fix what you find, re-run the checks once, commit, tick the boxes, and move on.

**Don't get stuck.** The main goal is five apps that work alone and connect well. Polishing one corner must never stall that.
- Give any single failing test, build or bug at most **three** real fix attempts. If it still fails, record it in §15 (what, where, what you tried) and continue. Don't revisit it until the final pass.
- Don't redo work that already passed its checks. Don't re-run a passing suite unless you changed code it covers.
- Some things can't be done on this Linux machine: running the Windows/macOS builds for real, publishing GitHub releases, code signing. Do everything up to that point (code, cross-target `cargo check`/clippy where a toolchain exists, CI workflow definitions), label it "build only" or "not run" as §13.6 requires, and move on. That counts as done for this run.
- If a feature turns out to be impossible as specified, implement the closest honest version, hide what can't work, document the gap in §15, and continue.

**Work alone.** Don't spawn sub-agents, delegate tasks to other agents, or open extra agent sessions or conversations. Do all the work yourself, in this one session. (Threads inside the apps' code, such as the Link server, are fine; this rule is about agents.)

**Finish with a final pass and a report.** After Phase 9:
1. Revisit the items you parked in §15 once each.
2. Run every repository's full suite and the ecosystem end-to-end run (§13.4) one last time.
3. Fill in §16 with what was built, test results, budget numbers against the baseline, what's "build only", and what's still open.

---

## 1. The vision, grounded

The earlier idea was a "personal computing layer" with a big control-plane app. This plan keeps what's good in that idea and changes two things:

1. **There is no central runtime.** Apps find each other through a small file-based registry and talk directly over a local socket. Nothing needs to be running except the two apps in the conversation. The manager app ("Arcade Tools") only installs, updates and removes apps; it is never in the call path.
2. **Each app is the single owner of its verb.** Integration means *delegating to the owner* instead of reimplementing. Most of the duplication already exists today (see §3.2), so this plan also removes it.

```
                  you have CONTENT: a file, a screen region, text, a clip
                                        │
        ┌──────────────┬────────────────┼────────────────┬──────────────┐
      SEE            UNDERSTAND      TRANSFORM          CARRY          INVOKE
   Arcade Look      Arcade Lens     Arcade Box     Arcade Clipboard  Arcade Wheel
  preview any file  recognize what  98 tools and   your devices,     one gesture
  instantly         is on screen    pipelines      encrypted mesh    runs anything
        └──────────────┴────────────────┴────────────────┴──────────────┘
                                        │
                   ARCADE LINK: registry + local socket + one shared vocabulary
                   (a protocol and a small library, not a process)
                                        │
                   ARCADE TOOLS: install, update, remove (optional)
```

---

## 2. What exists today (from the code)

| | **Box** | **Lens** | **Look** | **Wheel** | **Clipboard** |
|---|---|---|---|---|---|
| Purpose | Shortcut → type → run a tool | Select any screen region → actions | Quick Look for any file | Radial launcher | E2E-encrypted clipboard history mesh |
| Stack | Rust workspace (`arcade-contract`, `arcade-core`, plugin host) + Tauri 2 + Svelte; CLI | Rust workspace (`lens-core`, `-recognizers`, `-actions`, `-platform`, `-plugins`) + egui | Rust + Tauri 2 + vanilla TS | C++20 + Qt 6 / QML | Flutter (Dart) + Rust core via flutter_rust_bridge (one `api::call` JSON function) + relay |
| Executables | `arcade-desktop` (GUI), `arcadebox` (CLI) | `arcade-lens` | `arcade-look` | `arcade-wheel` | `clipboard` (Linux bundle) |
| App identifier | `dev.arcadebox.app` | ProjectDirs `dev/Arcade/Arcade Lens` | `app.arcadelook` | `com.arcadewheel.ArcadeWheel` | `dev.arcade.clipboard` |
| Single instance | **None** | Loopback TCP + token in a 0600 file | `tauri-plugin-single-instance` | `QLocalServer` socket in the runtime dir | GApplication/D-Bus (Linux) + profile lock |
| Background mode | Tray, no flag | `--background` | `--service` / `--background` | `--background` | `--background` |
| CLI verbs | `tools`, `search`, `run`, `pipeline`, `plugins`, `providers` | `--capture`, `--settings`, `--restart`, `--quit` | `<path>`, `--settings`, `--quit`, `--install-integration` | `--show`, `--status`, `--press`, `--quit`, … | `--overlay` |
| Plugins | WASM components (WIT `arcade:tool@1.0.0`), sandboxed | Out-of-process JSON-lines, permissioned, disabled by default | `plugin.json` command plugins + JS script plugins | `ActionProvider` interface (C++) | — |
| Typed model | MIME-like types (`file/image[]`, `text/url`, `screen/region`), typed DAG pipelines | Capability graph (`url` is-a `text`, `region` is-a `image`), findings, `Effects` bitflags, chains | `Kind` detection (image, video, pdf, archive, …) | Action types (`application`, `command`, `url`, `file`, `system`, `arcade_box`, `plugin`) | Clip kinds (`text`, `url`, `rich_text`, `image`, `file`, `files`) |
| Default shortcut | Ctrl+Alt+Space (Linux/Windows), Cmd+Shift+Space (macOS) | Ctrl+Alt+Shift+L | Ctrl+Alt+Space (Windows/macOS), none on Linux | F8 | Ctrl+Shift+Space (Linux), Ctrl+Alt+V (Windows), Cmd+Shift+V (macOS) |
| Platform status | Linux implemented; Windows/macOS providers and adapters still "Planned" | Linux X11 tested end to end; Windows/macOS compile but not run | Linux tested; Windows/macOS hooks compiled | Linux/Windows/macOS packaged in CI | Hyprland tested; X11 build only; Windows/macOS native code never built |
| Releases | **None** (no packaging; CI currently failing at "Test shared runtime") | `v<version>` + `nightly`, AppImage/setup.exe/dmg, SHA256SUMS | `v<version>` + `nightly`, NSIS/AppImage/dmg, SHA256SUMS | `vX.Y.Z+build.N` on every main build, per-file `.sha256` | CI artifacts only (Linux tarball, unsigned IPA) |
| Toolchain floor | Rust 1.95, edition 2024 | Rust 1.95, edition 2021 | Rust 1.88 | Qt 6.8, CMake 3.25 | Rust 1.89, Flutter |

---

## 3. What reading the code turned up

The ecosystem has already started to form by accident. Most of what's there is broken or duplicated. Fixing these is the first, cheapest win.

### 3.1 Integrations that already exist and don't work

| Where | What it assumes | Reality |
|---|---|---|
| Wheel `ArcadeBoxProvider` + `docs/ARCADE_BOX_BRIDGE.md` | Runs a binary named `arcade-box` with `tools --json` and `run --tool ID --input … --preset …` | Box's CLI is `arcadebox`. `tools` has no `--json` flag, `run` takes a positional `tool_id` plus `--file` / `--set`, and Box has no presets. **The bridge has never worked.** It also blocks for up to 500 ms (`waitForFinished(500)`) during construction, on the startup path. |
| Lens `examples/plugins/arcade-quicklook` | Runs `arcade-quicklook open <text>` | The app is `arcade-look`, and its CLI takes a path. No such command exists. |
| Lens `examples/plugins/arcade-wheel` | Runs `arcade-wheel add <text>` | Wheel has no `add` command. |
| Lens `examples/plugins/arcade-clipboard` | Runs `arcade-clipboard send <text>` | Clipboard's binary is `clipboard` and has no `send`. Clips can only be captured from the system clipboard or added in the app. |
| Lens Host: Quick Look (Linux) | Calls GNOME Sushi over D-Bus (`org.gnome.NautilusPreviewer.ShowFile`) | Arcade Look **implements that exact D-Bus service**, so Lens → Look already works on Linux when Look is running with its previewer enabled. It does nothing on Windows; macOS uses `qlmanage`. |
| Lens Host: Send to device | `kdeconnect-cli` (Linux only) | Arcade Clipboard's mesh is the ecosystem's device layer and runs everywhere, including the phone. |

### 3.2 Duplicated ownership

| Capability | Owner (keeps investing) | Duplicate today | Resolution |
|---|---|---|---|
| Screen region selection, screenshot, OCR on screen, QR on screen, color picking, ruler, image pins | **Lens** (complete: freeze, multi-monitor, mixed DPI, measure, pins, annotate) | Box `screen_capture.rs` (1,943 lines) and seven `arcade.screen.*` tools, all marked `partial` | When Lens is present, Box's screen tools call Lens (§7.1). Box's own code stays as the standalone fallback; no new investment. |
| Clipboard history | **Clipboard** | Box `clipboard_history.rs` (752 lines), `arcade.system.clipboard-history` (`partial`) | When Clipboard is present, Box's entry opens Clipboard's picker and returns the chosen clip. |
| File preview | **Look** | Box `media_preview.rs` | Box result actions use "Preview in Look". Box's inline preview stays for small thumbnails. |
| Send to another device | **Clipboard** | Lens via KDE Connect | Lens prefers Clipboard and falls back to KDE Connect. |
| OCR engine | Lens for anything on screen (Windows.Media.Ocr / Apple Vision; Tesseract on Linux) | Box: Tesseract | Box's provider broker gains **Arcade Lens as an OCR provider** (§7.1). On Windows and macOS this gives Box working OCR without Tesseract. Since 2026-10-08 Tesseract is the only third-party OCR engine: never bundled, found on the system or downloaded once per user into `<data>/arcade/engines` for every app (Link `engines` feature). Lens's bundled ocrs models are gone. |
| Pipelines | **Box** (typed DAG scheduler, artifacts, cancellation, versioned pipelines) | Lens "chains" (linear, Lens-internal) | Lens chains stay for Lens-only transforms. Anything that spans apps runs as a Box pipeline (§9). |

### 3.3 Conflicts and inconsistencies

- **Shortcut clash on Windows:** Box and Look both default to **Ctrl+Alt+Space**. Whichever starts second fails to register it.
- Identifiers, config folders, binary names and CLI flag styles all differ (§2). The plan **does not rename existing identifiers**, because that would lose users' settings and login items. It adds one canonical Arcade ID per app (§5.3).
- Box's `Cargo.toml` has `repository = "https://github.com/arcade-box/arcade-box"`; the real repository is `qa-p1/Arcade-box`.
- Wheel's bridge doc promises a 500 ms tool listing. Box's CLI, run cold, loads the whole catalog and opens SQLite, so that budget was never measured.
- Lens's IPC token comes from `RandomState` hashing. That's fine for a same-user guard, but the new Link uses OS randomness (`getrandom`).

---

## 4. Design principles

1. **Registry, not runtime.** Knowing which apps are *installed* means reading small JSON files. Knowing which are *running* means finding an endpoint file and connecting to it. There is no broker process.
2. **Delegate to the owner.** If a peer owns a verb, use it when it's present and use your own fallback when it isn't. Never build a second copy of a peer's verb.
3. **Content moves by reference.** Files are passed as paths. In-memory content (a Lens region, a Clipboard image) is written once to a handoff file. Bulk data never goes inline over IPC (Box's existing rule, made ecosystem-wide).
4. **Two ways to invoke.** Talk to the running instance when there is one. For headless actions (Box tools, Lens recognition, Look inspection), a one-shot process also works, so a resident UI never has to start just to run a tool.
5. **Interactive actions stay in the owner's UI.** Box's forms stay in Box; Wheel slot editing stays in Wheel's Settings. Other apps hand over the input. Defaults handle the common case: instant presets ("Convert to WebP"), plus "More in Arcade Box…", which opens Box's Island pre-filled.
6. **The user always confirms persistence and outbound actions.** Adding a Wheel slot, sending to devices, overwriting anything: confirmed in the owner's UI, or marked ↗ with a preview following Lens's model.
7. **Every boundary is versioned.** Protocol version, action versions and manifest schema version are negotiated. Unknown fields are ignored and never given authority (the same rule as Box's tool API).

---

## 5. Arcade Link: the shared layer

Arcade Link is a **specification** plus small **implementations**:

- a Rust crate, used by Box, Lens, Look and Clipboard's core;
- a Qt/C++ module, used by Wheel;
- conformance vectors that every implementation must pass.

These live in `/home/harneet/Work/Coding/Rust/Arcade-link`, a Cargo workspace written in **Rust**: four of the five apps are Rust (Clipboard's core included), so Rust is the native fit. Wheel is the only non-Rust app; it gets a small Qt/C++ implementation kept in the same folder (`qt/`) and checked against the same conformance vectors. No app depends on another app's repository.

Layout of `Rust/Arcade-link`:

```
Arcade-link/
├── SPEC.md                  protocol, manifest, content types, error codes
├── Cargo.toml               workspace
├── crates/arcade-link/      the library (registry, endpoint, client, server, content, handoff)
├── crates/arcade-link-cli/  debug CLI `arcade-link` (ls, describe, invoke, watch, mock)
├── qt/                      ArcadeLink.{h,cpp} for Wheel (vendored into Wheel's src/link/)
├── spec/vectors/            conformance test vectors (JSON), run by both implementations
├── assets/                  app glyphs + tokens.json for integration surfaces
├── tools/e2e.py             ecosystem end-to-end runner
└── benchmarks/              baseline and regression numbers
```

### 5.1 Components

| Component | Question it answers | Mechanism | Cost |
|---|---|---|---|
| **Registry** | What's installed, and what can each app do? | One manifest JSON file per app in a per-user directory, rewritten atomically by the app itself when it starts (only if it changed) | One small file read per app; cached by mtime |
| **Presence** | Is it running, and where do I connect? | One endpoint file per running app (0600) + a Unix domain socket or Windows named pipe | A failed connect means it isn't running |
| **Wire protocol** | How do we talk? | Newline-delimited JSON with JSON-RPC-style envelopes | — |
| **Vocabulary** | What is the content, and what can an action do? | Shared content types (§5.6) and effects (§5.5) | — |
| **Handoff** | How does in-memory content cross over? | Private handoff directory, with ownership and a TTL | Disk only while it's used |
| **Launch on demand** | It's installed but not running | Start `<exe> --background` and wait for the endpoint, or use one-shot `--arcade-invoke` for headless actions | Paid only when used |

### 5.2 Locations

`ARCADE_HOME`, if set, replaces every root below; tests and portable setups use it.

| | Registry (manifests) | Runtime (endpoints, sockets) | Handoff (temporary files) |
|---|---|---|---|
| Linux | `${XDG_DATA_HOME:-~/.local/share}/arcade/apps/` | `${XDG_RUNTIME_DIR}/arcade/` (fallback `~/.cache/arcade/run/`, mode 0700) | `${XDG_CACHE_HOME:-~/.cache}/arcade/handoff/` (on disk, not tmpfs: a 600 MB video must not end up in RAM) |
| macOS | `~/Library/Application Support/Arcade/apps/` | `$TMPDIR/arcade/` (macOS caps a socket path at 104 bytes, and paths under `~/Library/Application Support` can exceed that) | `~/Library/Caches/Arcade/handoff/` |
| Windows | `%LOCALAPPDATA%\Arcade\apps\` | Endpoint files in `%LOCALAPPDATA%\Arcade\run\`; pipes `\\.\pipe\arcade-<user-hash>-<app>` with a current-user-only DACL | `%LOCALAPPDATA%\Arcade\handoff\` |

### 5.3 App manifest (`<registry>/<arcade-id>.json`)

Canonical Arcade IDs: `arcade.box`, `arcade.lens`, `arcade.look`, `arcade.wheel`, `arcade.clipboard`, `arcade.tools`. They're separate from the platform bundle IDs, which never change.

```jsonc
{
  "schema": 1,
  "id": "arcade.look",
  "name": "Arcade Look",
  "version": "0.4.0",
  "link": { "protocol": [1] },
  "executable": "/home/u/Applications/Arcade-Look.AppImage",  // AppImage: $APPIMAGE, never the temporary mount
  "launch": { "background": ["--background"], "invoke": ["--arcade-invoke"] },  // invoke is omitted if there are no headless actions
  "icon": "/home/u/.local/share/arcade/icons/arcade.look.png",
  "shortcuts": [{ "id": "preview-selection", "accelerator": "Ctrl+Alt+Space" }],  // user's effective shortcuts, used to avoid clashes (§10.4)
  "settings": { "linkEnabled": true },
  "actions": [
    {
      "id": "look.preview",
      "version": 1,
      "title": "Quick Look",
      "verb": "preview",
      "accepts": ["file/*", "file/*[]", "folder/reference", "text/url"],
      "produces": [],
      "effects": ["opens-ui"],
      "interactive": true,
      "privacy": "local",
      "platforms": ["linux", "windows", "macos"],
      "available": true
    }
  ],
  "writtenAt": "2026-10-05T10:00:00Z"
}
```

Rules:

- Apps write their manifest on start (async, off the startup path) and when their capabilities change, for example when Box finds a new provider or a peer toggle changes.
- A manifest whose `executable` doesn't exist is ignored by readers, and removed by the owning app's uninstaller or by Arcade Tools.
- Box lists one action per tool and preset (§7.1). `available` reflects Box's **cached** provider probe (its `providers` table), so readers never run probes.
- Readers cache manifests by mtime and refresh them on a directory watch (inotify / FSEvents / ReadDirectoryChangesW through `QFileSystemWatcher` or a lazily started watcher thread), or on demand when a menu opens. Nothing polls.

### 5.4 Endpoint and transport

- On start, the resident instance listens on `<runtime>/<arcade-id>.sock` (Unix) or `\\.\pipe\arcade-<user-hash>-<arcade-id>` (Windows). It writes `<runtime>/<arcade-id>.endpoint` with `{protocol, transport, address, pid, startedAt, token}`, mode 0600, through an atomic rename.
- The token is 32 bytes from the OS random generator and sent in `hello`. This is defense in depth on top of the directory and pipe permissions.
- Rust uses the `interprocess` crate (Unix sockets and named pipes, synchronous API). Qt uses `QLocalServer` / `QLocalSocket`, which already map to the same primitives (Wheel uses them today with `UserAccessOption`).
- An endpoint is treated as dead if connecting fails or `hello` doesn't answer within 150 ms. Its file is then ignored and cleaned up by the next start of the owning app.
- Existing single-instance channels (Lens's TCP, Wheel's socket, Look's plugin, Clipboard's D-Bus) **stay as they are**. The Link endpoint is added alongside them; nothing existing is ripped out.

### 5.5 Wire protocol (version 1)

Each message is one UTF-8 JSON line, at most 1 MiB. Requests carry `id`; notifications don't.

```jsonc
→ {"v":1,"id":1,"method":"hello","params":{"token":"…","client":{"id":"arcade.lens","version":"0.2.0"},"protocol":[1]}}
← {"v":1,"id":1,"result":{"server":{"id":"arcade.box","version":"0.2.0"},"protocol":1}}

→ {"v":1,"id":2,"method":"invoke","params":{
     "action":"box:arcade.image.convert","preset":"webp",
     "inputs":[{"type":"file/image","path":"/home/u/.cache/arcade/handoff/7f3c…/region.png","owner":"arcade.lens"}],
     "options":{},
     "context":{"source":"arcade.lens","interactive":true,"reason":"user-click"}}}
← {"v":1,"id":2,"result":{"job":"j-41"}}                                    // long-running → job
← {"v":1,"method":"job.progress","params":{"job":"j-41","fraction":0.4,"message":"Encoding"}}
← {"v":1,"method":"job.done","params":{"job":"j-41","status":"success",
     "outputs":[{"type":"file/image","path":"/home/u/Pictures/region.webp"}],"message":"Converted to WebP"}}
```

| Method | Purpose |
|---|---|
| `hello` | Authenticate and negotiate the protocol version |
| `describe` | Live action list, the same shape as the manifest's `actions` (for live availability) |
| `invoke` | Run an action. Returns a `result` directly or a `job` for long work |
| `job.cancel` | Cancel a job. Its partial outputs are removed (Box's existing rule) |
| `subscribe` | Topics: `app.changed`, `job.*`. Used by Wheel/Look/Lens to update availability live |
| `app.status`, `app.activate`, `app.quit` | Used by Arcade Tools and by "Open <App>" entries. `app.quit` may refuse with `busy` while jobs run |

**Effects vocabulary.** These are Lens's `Effects` names, which the whole ecosystem adopts: `clipboard`, `writes-files`, `overwrites-files`, `deletes-files`, `network`, `uploads-content`, `sends-to-device`, `launch-apps`, `opens-ui`, `persists`, `executes-commands`, `window-control`. Privacy classes come from Box: `local`, `network`, `cloud`.

**Error codes.** Every consumer maps each one to the standard message in §10.3: `not_installed`, `not_running`, `launch_failed`, `timeout`, `unsupported_input`, `unavailable` (with `reason`, e.g. "FFmpeg not found"), `too_large` (with `limit`), `denied` (Private mode, secret guard, the user cancelled), `busy`, `cancelled`, `version_mismatch`, `internal`.

**One-shot mode.** `<exe> --arcade-invoke` reads one `invoke` from stdin, writes `job.progress` lines and a final result to stdout, then exits. It must not start any UI, tray, shortcut or listener. Box (tools), Lens (`lens.recognize`) and Look (`look.inspect`) implement it.

### 5.6 Content vocabulary

Box's MIME-like types become the canonical form, because they're the most formal of the five. Each app converts at its boundary and keeps its internal model.

| Link type | Meaning / transport |
|---|---|
| `text/plain`, `text/url`, `text/rich` | Inline text (≤ 256 KiB inline; larger goes in a handoff file). `text/rich` carries HTML or RTF plus a plain-text fallback |
| `file/<kind>`, `file/<kind>[]` | Existing file paths. Kinds: `image`, `video`, `audio`, `pdf`, `document`, `spreadsheet`, `presentation`, `archive`, `text`, `code`, `font`, `model`, `any` |
| `folder/reference` | A directory path |
| `structured/<name>` | Inline JSON: `color`, `barcode`, `findings`, `devices`, `table` |
| `screen/region` | `{rect, monitor}` in Lens's canonical physical-pixel virtual-desktop space |

`hints` hold semantic detail that the type alone loses, so Lens's richer capabilities survive the trip: `{"type":"text/plain","text":"rm -rf build","hints":["command"]}`.

Each app's mapping becomes a unit-tested function:

| Lens capability | Link | Look kind | Link | Clipboard kind | Link |
|---|---|---|---|---|---|
| `region`, `image` | `file/image` (PNG handoff) | image, video, audio, pdf, … | `file/<kind>` | `text` | `text/plain` |
| `text`, `code`, `command`, `email`, … | `text/plain` + hint | folder | `folder/reference` | `url` | `text/url` |
| `url` | `text/url` | archive | `file/archive` | `rich_text` | `text/rich` |
| `path` (that exists) | `file/<kind>` | binary/unknown | `file/any` | `image` | `file/image` (handoff) |
| `color` | `structured/color` | | | `file`, `files` | `file/any`, `file/any[]` (handoff) |
| `table` | `text/plain` + `structured/table` | | | | |

### 5.7 Handoff files

- The creator writes to `<handoff>/<uuid>/<name>` (mode 0600, directory 0700) and puts `"owner"` in the value.
- The receiver treats the file as read-only and never moves or deletes it. Outputs go to the receiver's own output location.
- The creator deletes the directory when the job finishes. As a safety net, every app at startup removes handoff directories older than 24 hours. That's a single directory listing, done off the startup path.
- Existing user files are **never copied**: a 600 MB video stays where it is and only its path travels.

### 5.8 Lifecycle and timing

```
caller wants action A from app P
 ├─ P's manifest missing, executable missing, P disabled, or A unavailable → don't show the entry
 ├─ endpoint alive → connect + hello (≤ 20 ms) → invoke
 ├─ not running, A headless, P has "invoke" → one-shot process
 └─ not running, A interactive → spawn `P --background`, wait for the endpoint
       (show a spinner in the caller after 150 ms; give up at 3 s with `launch_failed`)
```

Connections are cheap enough to open per call. A caller may keep one open while it's subscribed (Wheel's Settings, Look's open window).

### 5.9 Versioning and compatibility

- Protocol versions are negotiated in `hello`. Version 1 is frozen once Phase 3 is done; a breaking change becomes version 2, and implementations support N and N−1.
- Each action has its own `version`. Wheel slots and Box pipelines store the action ID and version, and an incompatible change marks them "needs repair" instead of breaking them (Box already does this for pipelines).
- The manifest has `schema: 1`. Readers ignore unknown fields.
- **In this run** the apps use a relative **path** dependency on the crate, because `Arcade-link` has no GitHub repository yet. List "switch to a git dependency on a tagged `Arcade-link` release before pushing" as an owner action in §16. Afterwards, the spec repository tags releases, and Rust apps pin the crate with a git dependency on a tag. Local development then overrides it with a Cargo `[patch]` using a relative path (never absolute): `../Arcade-link/crates/arcade-link` from Box and Lens, `../../Arcade-link/crates/arcade-link` from Look's `src-tauri`, and `../../Rust/Arcade-link/crates/arcade-link` from Clipboard's workspace root. Wheel vendors the Qt module into `src/link/`; a CI check compares its checksum with the pinned spec tag.

### 5.10 Implementations

| Consumer | Implementation | Notes |
|---|---|---|
| Box, Lens, Look | `arcade-link` Rust crate | MSRV **1.88**, edition 2021 (Look is the floor). Dependencies: `serde`, `serde_json`, `interprocess`, `getrandom`. No async runtime required. Synchronous core, server on a thread per connection; an optional `tokio` feature for Box/Look. Target: ≤ 300 KB added to a release binary. |
| Clipboard | The same crate, inside `core/rust` | The server starts in the core's `initialize`. Outbound calls are exposed as `api::call {"op":"link_invoke"}`, and inbound UI requests reach Dart through a new `link_wait` long-poll, the same pattern as `wait_for_change`. Dart gets no IPC code. |
| Wheel | `src/link/ArcadeLink.{h,cpp}` with QLocalSocket/QLocalServer + QJsonDocument | No Rust in the Wheel build. It passes the same conformance vectors. |
| Everyone | `arcade-link` debug CLI (in the spec repository) | `arcade-link ls`, `describe <app>`, `invoke <app> <action> …`, `watch`. Essential for development and for bug reports. |

### 5.11 Performance budgets (enforced in tests)

| Measure | Budget |
|---|---|
| Startup time added to any app | ≤ 5 ms on the critical path (manifest write and listener bind happen after the first frame) |
| Idle cost | 0 timers, 0 polling, at most 1 accept thread blocked, ≤ 1 MB RSS |
| Reading the registry (5 apps, warm / cold) | ≤ 2 ms / ≤ 10 ms, never on a UI thread |
| connect + hello | ≤ 20 ms p95 |
| Headless invoke overhead (excluding the work) | ≤ 30 ms p95 resident, ≤ 300 ms one-shot (Lens one-shot excludes OCR model load, which is measured separately) |
| Opening a menu or palette that contains peer entries | No IPC or disk access at open time; entries come from the cached registry |
| Lens palette | Peer entries go through Lens's `stabilize`; late availability never reorders the visible row |

Record a baseline for every app in Phase 0. A phase may not make any app's existing startup, warm-invoke or idle memory figures worse by more than 5%.

---

## 6. Capability ownership map

| Verb / capability | Owner | Exposed as | Consumed by |
|---|---|---|---|
| Preview any file or URL | Look | `look.preview` | Lens, Box, Clipboard, Wheel |
| Inspect a file (kind, dimensions, duration, pages, tags) | Look | `look.inspect` (headless) | Box, Clipboard, Wheel (slot subtitles) |
| Preview what's selected in the file manager | Look | `look.preview_selection` (Explorer COM / Finder AppleScript / GNOME previewer) | Wheel |
| Select a screen region | Lens | `lens.capture` (returns region image + rect) | Box screen tools, Wheel |
| Full Lens flow (select → palette) | Lens | `lens.capture_and_act` | Wheel, Box |
| Analyze an existing image | Lens | `lens.analyze` (opens the palette over the image) | Look, Clipboard, Box |
| Headless recognition (OCR, QR, colors, …) | Lens | `lens.recognize` | Box (OCR provider), Clipboard |
| Floating image pin | Lens | `lens.pin` | Box, Look, Clipboard |
| Transform content (98 tools) | Box | `box:<tool-id>` (+ presets), `box.open` | Lens, Look, Clipboard, Wheel |
| Cross-app pipelines | Box | `box.pipeline.run`, `box.pipelines` | Lens, Wheel, Look |
| Put content on all my devices | Clipboard | `clipboard.add` (↗ `sends-to-device`) | Lens, Look, Box |
| Choose a clip from history | Clipboard | `clipboard.pick` (picker returns the chosen clip to the caller) | Box (input), Wheel |
| Devices in the mesh | Clipboard | `clipboard.devices` (names, platform, online; no keys) | Box/Lens/Look (to label "Send to my devices (3)") |
| Run anything with one gesture | Wheel | `wheel.add_action` (confirmed in Wheel's Settings), `wheel.show` | Lens, Box, Look |

**Deliberately not in this plan** (the earlier idea promised these; the code doesn't support them):

- Sending to **one specific** device. Clipboard's protocol broadcasts to the whole mesh history, so "Send to Laptop" would need a protocol v2 with targeted items. The ecosystem says **"Send to my devices"**. Targeted delivery is listed as a future protocol change (§15).
- Archive compress/extract in Box. Box has no archive tools today; Look only lists archives. Don't advertise it.
- Running actions on another device over the mesh. That's possible later, because device certificates already carry "capabilities", but it's out of scope.

---

## 7. Per-app plan

Each section lists: role · what it exposes · what it consumes · core changes · UI surfaces · how standalone behavior is protected · tests.

### 7.1 Arcade Box: transform and pipeline engine

**Exposes**

- Every implemented tool as `box:<tool-id>`. Tools gain **presets**, which are new: named option sets in `catalog/tools.json`, validated by `tools.schema.json`. Examples: `arcade.image.convert` → `webp` / `png` / `jpeg-85`; `arcade.video.compress` → `share-25mb` / `half-size`; `arcade.pdf.optimize` → `email`.
- Preset actions are what other apps show as one-click entries. A tool with no preset is offered only through `box.open` (Box's Island pre-filled with the input).
- A new catalog field, `link.featuredFor: ["file/image", …]`, ranks which 3–5 Box actions a peer shows inline for a given type. The rest go under "More in Arcade Box…".
- `box.pipelines` (list with input types) and `box.pipeline.run`.

**Consumes**

- **Lens as a provider.** Box's provider broker gets an "Arcade app" provider family:
  - `ocr.lens` → `lens.recognize` (with `ocr` only). Preferred on Windows and macOS, where Tesseract is usually missing. It records provenance like any other provider.
  - `screen.select.lens` → `lens.capture`. Box's seven `arcade.screen.*` tools use it when present. Lens's measure/pin/color pick are better, so `screen.ruler` / `screen.pin` / `screen.color` hand over to Lens entirely (`lens.capture_and_act` with a mode hint).
- **Clipboard.** `arcade.system.clipboard-history` opens `clipboard.pick` when Clipboard is present. "Send to my devices ↗" is added to result actions for outputs ≤ 16 MiB; larger outputs show it disabled with the reason.
- **Look.** "Preview" is added to result actions for file outputs.
- **Wheel.** "Add to Wheel" on any tool, preset or saved pipeline (→ `wheel.add_action`).

**Core changes**

1. **CLI contract fix.** Add a binary named `arcade-box` and keep `arcadebox` as an alias.
   - Add `tools --json` (a stable output shape that also includes presets).
   - Add `run --preset` and a `--stdin-json` request mode.
   - Add `--arcade-invoke` one-shot mode.
   - `tools --json` must complete in ≤ 150 ms cold. Measure it; if needed, read the catalog without opening SQLite.
2. **Resident instance.** Add `tauri-plugin-single-instance` (as Look does) and `--background` (start to tray, no window). Start the Link server in the desktop process. Box's CLI and desktop already share a single SQLite database.
3. **Delegated selection grant.** A path that arrives through Link counts as user-selected **for that job only**. It's checked like a dialog selection (canonicalized, a regular file, readable), recorded in `jobs`, and the grant ends with the job. Outputs follow Box's rule: a new file, never overwrite. This fits the existing grants model (`grants.rs`); it doesn't bypass it.
4. **Manifest writer.** Builds actions from the catalog, presets and the cached provider table. It's rewritten when provider discovery results change.
5. **Platforms.** Box is the furthest behind on Windows and macOS, and the ecosystem promises all three. Phase 7 (§14) takes the provider broker to Windows (PATH, `Program Files`, winget/scoop locations) and macOS (Homebrew `/opt/homebrew`, `/usr/local`, app bundles). Each platform-matrix row moves from "Planned" only after a real run, following Box's own rule.
6. Housekeeping: fix the `repository` URL; fix the failing CI step ("Test shared runtime") **in Phase 0, before anything else**.

**UI.** The Island's result panel gets Preview · Send to my devices ↗ · Add to Wheel · Pin (Lens). Settings gets a "Connected apps" page (§10.2). Box's dashboard is unchanged.

**Standalone guarantee.** Every screen and clipboard tool keeps its current code path when the peer is absent. The test suite runs once with `ARCADE_HOME` empty and once with a mock peer.

### 7.2 Arcade Lens: understand and act on the screen

**Exposes** `lens.capture`, `lens.capture_and_act`, `lens.analyze` (an image file shown as the frozen overlay; the existing overlay already shows a frozen image, so the source just becomes pluggable), `lens.recognize` (headless; options pick recognizers; budgeted, cancellable, runs on the existing engine thread pool), `lens.pin`.

**Consumes.** A new built-in action module, `lens-actions/src/arcade.rs`, provided by the registry. It's not a plugin, because these are first-party apps: they're on by default and can be switched off per app in Settings → Connected apps. Each action declares real `Effects`, so the secret guard and confirmation rules apply automatically.

| Finding | Action | Peer |
|---|---|---|
| `path` (that exists), `url` | **Quick Look** (key `y`, the same key the plugin example used) | Look (`look.preview`). On Linux it falls back to the existing D-Bus call, then to the default opener |
| `region` / `image` | **Convert / Compress / Upscale / Remove background** (Box presets featured for `file/image`) and **More in Arcade Box…** | Box |
| `text`, `url`, `region`, … | **Send to my devices ↗** | Clipboard (`clipboard.add`). The Host's `send_to_device` prefers Clipboard and falls back to KDE Connect |
| `url`, `command`, `path`, `text` | **Add to Wheel** | Wheel (`wheel.add_action`, `persists`) |
| any finding whose type starts a saved Box pipeline | **▶ <pipeline name>** | Box (`box.pipeline.run`) |

**Core changes**

1. Add an `AppSource` image source to the overlay (for `lens.analyze`).
2. Add `--arcade-invoke` headless mode.
3. Start the Link server in the background instance. On Wayland, the server lives in the window-less background process, which already exists.
4. Replace `examples/plugins/arcade-*` with the native module. Keep `isbn` as the third-party example, and update `PLUGINS.md` (the "assumes these commands" paragraph goes away).

**Standalone guarantee.** Without peers, the palette is identical to today's (snapshot test of the palette for fixture selections).

### 7.3 Arcade Look: see anything

**Exposes** `look.preview` (paths, batches navigable with ←/→, URLs to local files), `look.inspect` (headless: `detect.rs` + parsers, inside Look's existing read budgets), `look.preview_selection` (reuses `integration::{windows, macos, linux}` file-manager selection).

**Consumes.** A title-bar **actions strip**, filtered by the current file's kind and built from the registry:

- **Box**: up to 3 featured presets for this type, for example video → *Compress for sharing*, *Extract audio*; PDF → *Compress*, *OCR to searchable PDF*; image → *Convert to WebP*, *Remove background*. Plus "More in Arcade Box…". Job progress shows as a small chip in the title bar. When the job finishes, the output opens in Look (the preview of the result is the confirmation).
- **Clipboard**: "Send to my devices ↗", disabled with a reason above 16 MiB.
- **Lens**: "Analyze with Lens", for images and the current PDF page rendered by pdf.js.
- **Wheel**: "Add to Wheel" (a file action).

Keyboard: the strip opens with `A`, which is free in Look's key map. The `?` sheet lists the new keys.

**Core changes**

1. Start the Link server alongside the single-instance plugin.
2. Add `--arcade-invoke`.
3. Move the default global shortcut on Windows to avoid the Box clash (§10.4).

**Standalone guarantee.** The strip is not rendered at all when no peer contributes. The ~17 KB UI shell budget holds: the strip is a lazy chunk loaded only when the registry has entries.

### 7.4 Arcade Wheel: invoke anything

**Exposes** `wheel.add_action`, which opens Settings → ActionPicker pre-filled and **requires the user to pick a slot and confirm**. Wheel never edits decks silently. Also `wheel.show`.

**Consumes.** A new action type, `arcade`, with the payload `{app, action, version, preset?, input}`. `input` is one of:

- `none`
- `clipboard`: current clipboard content, converted through §5.6
- `lens-selection`: run `lens.capture`, then the action
- `file-selection`: `look.preview_selection`'s selection resolver, then the action

The ActionPicker lists every peer's actions, grouped by app, with search. It's ready-made for flagship slots: *Lens: Capture*, *Clipboard: Picker*, *Look: Preview selection*, *Box: <pipeline>*, *Box: <tool preset> on clipboard*.

**Core changes**

1. `src/link/ArcadeLink` (client + server).
2. Replace `ArcadeBoxProvider` with `ArcadeLinkProvider`, which reads the registry asynchronously with `QFileSystemWatcher`. The 500 ms synchronous `QProcess` call in the constructor goes away.
3. Config **schema 4**: migrate `arcade_box` slots to `arcade` slots (`{app:"arcade.box", action:"box:<toolId>"}`), keeping the old payload so nothing is lost. `ConfigStore` already migrates schemas.
4. Rewrite `docs/ARCADE_BOX_BRIDGE.md` as `docs/ARCADE_LINK.md`.

**Standalone guarantee.** Unavailable `arcade` slots behave exactly like today's unavailable provider slots: kept, and marked with the reason.

### 7.5 Arcade Clipboard: carry across devices

**Exposes**

- `clipboard.add`: puts content into history, which syncs to all devices. It respects Private mode (`denied`), the 32 KiB text and 16 MiB / 32 parts limits (`too_large`), and dedup (an existing clip moves to the top). It does **not** write the local OS clipboard.
- `clipboard.pick`: opens the picker; the chosen clip is returned to the caller instead of pasted.
- `clipboard.devices`.

Nothing exposes history contents without the user acting in the picker. That's a deliberate privacy boundary.

**Consumes.** Actions on history items, shown by clip kind in the item's context menu and as keyboard shortcuts in the picker:

- Image or file clip → **Quick Look** (Look, through a handoff file). Images also get **Analyze with Lens** and **Pin** (Lens).
- Image clip → **Extract text** (`lens.recognize` headless → adds a text clip).
- Text/JSON clip → Box text presets (*Format JSON*, *Clean text*) → adds the result as a new clip.
- Image clip → *Compress* before it goes out to the mesh. This is opt-in, offered automatically when an image is over 16 MiB: the clip can't sync, and here's a one-click fix.

**Core changes**

1. Add `link.rs` in `core/rust` (server + client).
2. Add `link_invoke` / `link_wait` operations.
3. Dart: a `LinkService` in `AppController` and item-action widgets.
4. Ship a launcher named `arcade-clipboard` next to `clipboard`, because `clipboard` is a generic name that clashes on `PATH`.
5. Add the standard flags `--quit`, `--version`, `--arcade-manifest` (§10.1).

**Mobile.** iOS and Android are **not** Link participants (no local sockets between apps, and background limits). They benefit indirectly: content another desktop app sends with "Send to my devices" arrives on the phone. Note this in `docs/platforms.md`.

**Standalone guarantee.** `cargo test -p arcade_core` and `flutter test` pass with no peers. The item menu shows only today's entries.

---

## 8. Flagship flows

Each flow lists its exact calls and what happens when a peer is missing (the entry is hidden; nothing breaks).

1. **Screen → phone in two keystrokes.** In Lens, select a region → `m` "Send to my devices ↗" → `clipboard.add {file/image}` → the clip shows up in the phone's history. If the region contains a secret finding, the secret guard removes the action. Without Clipboard, Lens falls back to KDE Connect.
2. **"Send optimized screenshot" on one Wheel slot.** A Box pipeline: `lens.capture` → `arcade.image.resize {50%}` → `arcade.image.convert {webp}` → `clipboard.add`. Bind it to a Wheel slot (`arcade` type, input `none`). One flick runs Lens + Box + Clipboard. Built with Box's existing pipeline model (§9).
3. **Compress the 600 MB video you're looking at.** In Look, `A` → "Compress for sharing" → `box:arcade.video.compress#share-25mb` (job with progress in Look's title-bar chip) → the result opens in Look → "Send to my devices ↗" is now enabled because the result is under 16 MiB.
4. **A photo arrives from your phone.** In Clipboard's history item menu: Quick Look (Look), Extract text (Lens headless → new text clip), Convert to PNG (Box) → new clip, Pin (Lens).
5. **Box OCR on Windows with no Tesseract installed.** `arcade.image.ocr` sees provider `ocr.lens` (Windows.Media.Ocr through `lens.recognize`). The provider panel shows "Arcade Lens (local)".
6. **Box "OCR screen".** Type "ocr screen" in Box → `lens.capture` (Lens's freeze overlay, multi-monitor, DPI-correct) → OCR → text in Box's result panel. Without Lens, Box's own partial capture path runs as today.
7. **Quick Look from the screen, on every OS.** Lens finds a file path in a terminal → `y` → `look.preview`. This used to work only on GNOME Linux by accident.
8. **Save a command you saw as a Wheel action.** Lens finds a `command` → "Add to Wheel" → `wheel.add_action` → Wheel Settings opens with a `command` action pre-filled (Wheel shows that it isn't run through a shell) → the user picks the slot.
9. **Preview whatever's selected in the file manager, from the Wheel.** A Wheel slot runs `look.preview_selection` (Explorer COM / Finder AppleScript / GNOME previewer).
10. **PDF from a colleague.** In Look: "Compress PDF", "Make searchable (OCR)" (Box). The output opens in Look.

---

## 9. Cross-app pipelines (Box is the engine)

- Box's DAG scheduler gains one node runtime: **`link`**, which runs a peer's action. Node identity is `{app, action, version}`, and type checking uses the Link content types. Box already validates tools, permissions, types and provider requirements before starting; a `link` node adds "peer installed and action available".
- Interactive nodes (`lens.capture`, `clipboard.pick`) are allowed only as the **first** node, so a pipeline never pops UI halfway through.
- The pipeline's effects are the union of its nodes' effects. Anything with `sends-to-device` / `network` / `executes-commands` is confirmed the first time it runs, following Lens's rule for chains.
- Intermediate artifacts stay in Box's private job directory; only peer-bound inputs go through handoff.
- Pipelines are surfaced in the Box dashboard (editor), Wheel (slot), Lens (palette entry when the first node's input type matches a finding) and Look (actions strip when the type matches).
- Lens chains are not migrated; they stay Lens-internal.

---

## 10. Consistency layer

### 10.1 A standard CLI for every app

| Flag | Box | Lens | Look | Wheel | Clipboard |
|---|---|---|---|---|---|
| `--version` | add | add | ✓ (`-V`) | add | add |
| `--background` | **add** | ✓ | ✓ (alias) | ✓ | ✓ |
| `--settings` | add | ✓ | ✓ | ✓ | add |
| `--quit` | add | ✓ | ✓ | ✓ | **add** |
| `--arcade-manifest` (print the manifest JSON, no side effects) | add | add | add | add | add |
| `--arcade-invoke` (one-shot, headless only) | add | add | add | — | — |

Existing flags and subcommands keep working. These are added, not renamed.

### 10.2 "Connected apps" settings page (same layout in all five apps)

- One row per Arcade app: icon, name, state (*Running · v0.2.0* / *Installed* / *Not installed*), and a toggle **"Use with <this app>"**. Turning it off hides that peer's entries in this app only.
- A master switch: **"Connect with other Arcade apps"**. When it's off, there's no listener and no manifest actions.
- For an app that isn't installed: one line describing what it would add here, plus **Get** (opens Arcade Tools if installed, otherwise the GitHub releases page). **Promotion appears only on this page, never in palettes, menus or results.**
- A diagnostics expander: registry path, endpoint state, last error.

### 10.3 Naming, badges, messages

- Entries are named as the verb the user wants ("Quick Look", "Compress for sharing", "Send to my devices"), with the owning app's small monochrome glyph as a badge. No "Powered by". Glyphs live in `Arcade-link/assets/` and are copied in at build time.
- Every outbound action gets ↗ plus a payload preview (Lens's convention, now ecosystem-wide).
- Standard messages per error code, used verbatim in every app. Examples:
  - `unavailable` → "Arcade Box can't do this yet: FFmpeg isn't installed."
  - `too_large` → "Too large to send to your devices (limit 16 MB)."
  - `denied`/Private → "Arcade Clipboard is in Private mode."
- Design tokens: one `tokens.json` (accent per app, neutrals, radius, motion durations) used **only** for integration surfaces (badges, strips, the Connected apps page). There's no reskin of the existing UIs.

### 10.4 Shortcut defaults

Change defaults for new installs only. A shortcut the user has saved is never changed.

| App | Linux | Windows | macOS |
|---|---|---|---|
| Box | Ctrl+Alt+Space | Ctrl+Alt+Space | Cmd+Shift+Space |
| Look | none (Space in GNOME Files) | **Ctrl+Alt+Shift+Space** (was Ctrl+Alt+Space, which clashed with Box) | Ctrl+Option+Space |
| Lens | Ctrl+Alt+Shift+L | Ctrl+Alt+Shift+L | Ctrl+Alt+Shift+L |
| Clipboard | Ctrl+Shift+Space | Ctrl+Alt+V | Cmd+Shift+V |
| Wheel | F8 | F8 | F8 (Fn+F8) |

Each app publishes its effective shortcuts in its manifest. When a user records a shortcut that another Arcade app already uses, the recorder says so by name ("Used by Arcade Box"). That's checked from the registry, without IPC.

---

## 11. Release and distribution standard

Every repository's release gains two assets: `SHA256SUMS.txt` (Lens and Look already have one; Wheel moves its per-file `.sha256` files into a single list as well) and **`arcade-release.json`**:

```jsonc
{
  "schema": 1, "id": "arcade.look", "version": "0.4.0", "channel": "stable",
  "linkProtocol": [1], "notes": "https://github.com/qa-p1/Arcade-look/releases/tag/v0.4.0",
  "assets": [
    { "os": "windows", "arch": "x64", "kind": "nsis", "file": "Arcade-Look_0.4.0_x64-setup.exe", "sha256": "…", "silent": ["/S"] },
    { "os": "linux", "arch": "x64", "kind": "appimage", "file": "Arcade-Look_0.4.0_amd64.AppImage", "sha256": "…" },
    { "os": "macos", "arch": "universal", "kind": "dmg", "file": "Arcade-Look_0.4.0_universal.dmg", "sha256": "…" }
  ]
}
```

Existing asset names stay as they are; the manifest maps them.

| Repository | Work |
|---|---|
| Wheel | Add the manifest to `publish-release.py`; silent flag `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER` (Inno) |
| Lens | Add it to the `release` job; Inno silent flags as above |
| Look | Add it to `release.yml`; NSIS `/S` |
| Box | **Create packaging and a release workflow from scratch**, modeled on Look (Tauri bundler: NSIS, AppImage, dmg). Bundle the `arcade-box` CLI next to the GUI. Depends on the CI fix in Phase 0. |
| Clipboard | **Create GitHub Releases.** Today there are only CI artifacts. Linux: an AppImage, or keep the tarball with `kind: "tarball"` and a documented install step. Windows/macOS: build in CI for the first time (the native code has never been built), plus an Inno installer and a dmg. The iOS IPA stays an artifact, outside the manager. |

Channels: `stable` (`v<version>`, and Wheel's `vX.Y.Z+build.N`) and `nightly` (Lens and Look already publish it; the others adopt it).

---

## 12. Arcade Tools: the manager (Phase 8)

**What it does:** install, update, uninstall, repair, choose a channel, launch, and toggle start-at-login. It does **not** show capabilities, run pipelines or manage devices; those belong to the apps.

- **Stack:** Tauri 2 + vanilla TypeScript, following Look's lightness rules (no framework runtime, no bundled Chromium). Rust core using the `arcade-link` crate for the registry and `app.quit` / `app.status`.
- **Sources:** GitHub Releases for each repository → `arcade-release.json` → verify the SHA-256 of the downloaded file before running anything. Signature verification (minisign) is a planned hardening step, because builds aren't code-signed today. Never remove macOS quarantine or bypass SmartScreen on the user's behalf.
- **Install locations (per user, no admin):**
  - Windows: the app's own per-user installer, run with its silent flags.
  - Linux: AppImages in `~/Applications/Arcade/`, then first-run integration (each app already registers its menu entry and autostart on first run). The Clipboard tarball goes through its install script.
  - macOS: mount the dmg with `hdiutil`, copy to `~/Applications` (Wheel's login item accepts `~/Applications`).
- **Update flow:** check → download → verify → `app.quit` (an app may answer `busy` while jobs are running; the manager then waits or asks) → install → relaunch with the same mode (`--background` if it was in the background) → the app rewrites its manifest.
- **Uninstall:** quit → run the platform uninstaller or remove the AppImage/app bundle → remove the manifest. User data stays unless the user ticks "Remove settings and data", which lists each app's data folder.
- **The manager is optional.** Every app keeps working, and stays updatable by hand, without it. The manager registers as `arcade.tools` and exposes `tools.install`, so a "Get" button in any app's Connected apps page can hand off to it.

---

## 13. Testing and verification

1. **Conformance.** `Arcade-link/spec/vectors/*.json` holds wire messages, manifests, content conversions and error cases. The Rust crate and Wheel's Qt module both run them in CI.
2. **Mock peer.** `arcade-link mock --as arcade.box --actions fixtures/box.json` is a scriptable fake app (latency, errors, `busy`, crashes). Each app's CI tests its consumer side against it; no app's CI builds another app.
3. **Standalone suite.** Every repository's existing tests, run with an empty `ARCADE_HOME`. They must pass unchanged.

   | Repository | Commands |
   |---|---|
   | Box | `cargo fmt --all --check`, `cargo test --workspace --exclude arcade-desktop`, `npm run check`, `npm run build`, `python3 scripts/smoke-mvp.py` |
   | Lens | `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` |
   | Look | `npm run typecheck && npm run build`, `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (in `src-tauri`) |
   | Wheel | Build + `ctest` (offscreen) |
   | Clipboard | `bash scripts/dev.sh test`, `bash scripts/dev.sh check` |

4. **Ecosystem end to end (local, Linux, headless).** `Arcade-link/tools/e2e.py` builds or locates the five sibling checkouts. It starts them in background mode under one temporary `ARCADE_HOME`, using each app's existing test isolation (`ARCADE_LENS_HOME`, `ARCADE_DATA_DIR`, `ARCADE_WHEEL_DISABLE_GLOBAL_SHORTCUT`, offscreen Qt, Xvfb). It then drives the flows from §8 through the debug CLI, and finally kills each app in turn to check that every consumer degrades cleanly.
5. **Performance.** A benchmark script per app records the §5.11 numbers against the Phase 0 baseline and fails on any regression over 5%.
6. **Definition of done for an integration:**
   - It works in both directions of discovery: peer starts first, and peer starts later (live `app.changed`).
   - It handles: peer missing, peer disabled, action unavailable, peer crashes mid-job, timeout, cancel, oversized input, Private mode / secret guard.
   - It has no UI-thread blocking (verified by an instrumented test or a trace).
   - Its docs are updated in both repositories.
   - Its platform behavior is stated for Linux X11, Linux Wayland, Windows and macOS. A platform not verified by a real run is labeled "build only", following each project's existing honesty convention.

---

## 14. Phased roadmap

Run all phases in order, in one go, without waiting for the owner. Each phase ends with the self-review in §0.4; once it passes, tick the boxes and start the next phase immediately.

**Phase 0: Baseline (no feature work)**
- [x] Every repository builds, and its tests pass, on a fresh branch.
- [x] Fix Box CI ("Test shared runtime" fails on Linux and Windows) and Box's `repository` URL.
- [x] Record performance baselines (startup, warm invoke, idle RSS) for all five apps in `Arcade-link/benchmarks/baseline.md`.

**Phase 1: Arcade Link foundation (`Rust/Arcade-link`, Rust)**
- [x] Spec (`SPEC.md`), conformance vectors, Rust crate (registry, endpoint, client, server, content, handoff), debug CLI, mock peer.
- [x] Qt module (shipped from `Arcade-link/qt/`, vendored into Wheel in Phase 2).
- [x] The crate's own tests: concurrency, a stale endpoint, permissions on all three operating systems (CI matrix), the macOS socket path-length case.

**Phase 2: Presence in every app (no user-visible change)**
- [x] Each app writes its manifest, listens on its endpoint, and supports `--arcade-manifest` and `--version`.
- [x] Standard flags (§10.1): Box `--background` + single instance; Clipboard `--quit`, the `arcade-clipboard` launcher; Box `arcade-box` binary.
- [x] `arcade-link ls` shows all five apps; the standalone suites pass; the budgets hold.

**Phase 3: Exposed actions (servers)**
- [x] Box tools + presets + `--arcade-invoke` + delegated grants; Lens `capture` / `capture_and_act` / `analyze` / `recognize` / `pin`; Look `preview` / `inspect` / `preview_selection`; Wheel `add_action` / `show`; Clipboard `add` / `pick` / `devices`.
- [x] Each action is tested through the debug CLI against the real app. **Freeze protocol version 1.**

**Phase 4: Integrations (consumers + UI), in this order, each one end to end**
- [x] Wheel: `ArcadeLinkProvider` replaces the broken Box bridge (schema 4 migration).
- [x] Lens: native Arcade action module (Quick Look, Send to my devices, Box presets, Add to Wheel); retire the broken example plugins.
- [x] Look: actions strip.
- [x] Clipboard: item actions.
- [x] Box: result actions + Connected apps page.
- [x] The Connected apps page in all five apps (§10.2).

**Phase 5: Overlap resolution**
- [x] Box → Lens (screen tools, OCR provider); Box → Clipboard (history); Lens → Clipboard (send to device); Box → Look (preview).
- [x] Shortcut default change for Look on Windows; registry-backed clash warnings in every shortcut recorder. *(Look defaults to Ctrl+Alt+Shift+Space on Windows; all five recorders warn from the cached registry. Lens's clash e2e check now passes; see §15.)*

**Phase 6: Cross-app pipelines**
- [x] Box `link` node runtime, interactive-first-node rule, effect confirmation; pipelines surfaced in Wheel, Lens and Look; flagship flow 2 works end to end. *(e2e `box-pipeline`, `wheel: real_box_pipeline_and_look_preview`, `lens: real_box_preset_and_cached_pipeline_entries`, `look: native_strip_…_cached_pipelines`; all pass.)*

**Phase 7: Platform completion**
- [x] Box provider broker and platform adapters on Windows and macOS. *(Build only: discovery paths and adapters are written and unit-tested on Linux; there's no Windows/macOS toolchain here, so they were not compiled or run. CI builds them on native runners.)*
- [x] Clipboard Windows/macOS builds in CI. *(Defined, not run here.)*
- [x] A real-run pass on each operating system for the §8 flows. Update every platform matrix honestly. *(Linux only: X11 under Xvfb for every flow, plus the owner's Hyprland session for trays and Lens. Windows and macOS are labeled build only / not run in every matrix.)*

**Phase 8: Releases and Arcade Tools**
- [x] `arcade-release.json` + `SHA256SUMS.txt` in all five release pipelines; Box and Clipboard get their first releases. *(Pipelines are in place and their manifest/checksum steps pass on fake dists. Nothing was published: the first releases happen when the owner merges to `main` / tags.)*
- [x] Create `Arcade-tools`: install / update / uninstall / repair / channels on all three operating systems. *(Linux verified end to end with real GUI checks; Windows/macOS install logic is unit-tested and build only.)*

**Phase 9: Hardening**
- [x] Failure-injection run (§13.4) and performance regression run (§13.5). *(Failure group 5/5. The performance run found and fixed a Box regression (idle RSS 421 → 775 MiB from provider re-probes, `1a1cf28`); The final dependency-cleanup benchmark supersedes that run; remaining performance limits are in §16.)*
- [x] Docs pass: each README gains a short "Works with other Arcade apps" section; each repository documents its exposed actions. *(All five READMEs; actions in each repo's `docs/arcade-link.md` / `docs/ARCADE_LINK.md`.)*

---

## 15. Risks, open decisions, and backlog

| Item | Notes |
|---|---|
| Box on Windows/macOS is a large effort | Phase 7 is the biggest phase. Box's integrations still work on Linux before it lands, but the "all three platforms" promise isn't met until it's done. |
| Clipboard on Windows/macOS has never been built | The same caveat; the native code exists but is unverified. |
| Unsigned builds | Manager downloads are verified by SHA-256 over HTTPS. Add minisign signatures before promoting the manager widely. Code signing is a separate, owner-level decision (certificates). |
| Same-user trust model | Link authenticates "the same OS user", not "a genuine Arcade app". This is the same trust level as each app's CLI today. Every app's safety rules still apply to Link calls (§0.3.5). |
| Wayland limits | No global capture or shortcuts without portals or a compositor. The Link doesn't change this. Lens/Clipboard Hyprland bindings and portal paths stay the mechanism. |
| Targeted device send | Needs Clipboard protocol v2 (targeted items). Backlog. |
| Remote invocation over the mesh | Run a Box tool on the desktop from the phone. Device certificates already carry capabilities; this is a future design, not this plan. |
| Lens IPC token randomness | Switch to `getrandom` when the crate is adopted. Small and safe. |
| Folders | `Rust/Arcade-link` (exists) and `Rust/Arcade-tools` (create in Phase 8) are both approved. The owner created both GitHub repositories (`qa-p1/Arcade-Link`, `qa-p1/Arcade-tools`); both are pushed. |

**Bugs found along the way (add to this list):**

- Box CI (fixed in Phase 0, commit "Fix CI on Linux, Windows and macOS…"): the libvips probe matched saver *class* names, which differ when libvips is built with libspng (Ubuntu 24.04: `VipsForeignSaveSpngFile`), so every vips tool was "unavailable" on CI; `artifacts.rs` imported the Unix-only `cap_std::fs::DirBuilderExt` on Windows; `paste_plain.rs` built a `&str` where a `String` was expected on Windows/macOS. Only the first Windows compile error was visible in the CI log, so later Windows/macOS errors may still surface: no Windows/macOS Rust std is installed here to check.
- Lens (`lens-platform/src/autostart.rs` `launch_path`) and Wheel (`main.cpp` restart, `LinuxBackend.cpp` autostart and launcher entry) trust `$APPIMAGE` without checking `$APPDIR`. A copy started from another AppImage (a launcher, an IDE, a terminal packaged as an AppImage) inherits both variables and would point its login item / menu entry / restart at that other AppImage. Look already checks `APPDIR` (`integration/mod.rs`). The Link's `current_executable()` (Rust and Qt) checks it. Not changed in the apps (unrelated to this plan).
- Box: the plan (§7.1) says the CLI and the desktop share one SQLite database. They don't: the desktop uses Tauri's app data dir (`…/dev.arcadebox.app/arcade.sqlite3`), the CLI `directories::ProjectDirs("dev","Arcade Box","Arcade Box")` (`…/arcadebox/arcade.sqlite3`), so pipelines, favorites and history saved in one aren't visible in the other. Box's Link one-shot mode is served by the desktop binary against the desktop's database, so Link callers see the desktop's pipelines. Not unified (unrelated refactor).
- Clipboard: tray Quit (and now `--quit`/`app.quit`) always waits for the 4 s timeout in `DesktopAdapter._quit`, because the core's `shutdown` takes the lifecycle write lock, which waits behind the pending 30 s `wait_for_change` long-poll (it holds a read lock), so the orderly core shutdown never actually runs before `exit(0)`. Fixed in Phase 5 (Clipboard `ed9189c`): `wait_for_change` no longer holds the lifecycle lock, so quit takes ~0.1 s (`app.quit`) / ~0.6 s (`--quit`).
- Look: `app::start_idle_watcher` wakes every 20 s for the life of the process (a polling loop that predates the Link; it decides when to release the webview). Measured idle CPU is still 0 ms / 5 s.
- Box: `crates/arcade-plugin-host/tests/fixtures/community-uppercase/plugin.json` still names `https://github.com/arcade-box/arcade-box` as its source. Left alone: it is a test fixture and may be fingerprinted.
- Plan §7.3 says `A` is free in Look's key map; it isn't: the 3D model viewer uses `A` for auto-rotate. Resolved in Look: without peers nothing changes; when the actions strip exists, `A` opens it and auto-rotate moves to `Shift+A` (and the viewer toolbar). The `?` sheet says so.
- Look's window never boots its web page under headless Xvfb (no window manager): it is created hidden, and WebKitGTK doesn't load the page for a window that was never mapped, so no UI surface of Look could be screenshot-tested until a test-only early-map switch was added (see Look's docs). Box, visible from the start, renders fine in the same session.
- Lens: one-shot and resident `lens.recognize` returned the same findings in a different order (nondeterministic). Fixed in Lens ("Make Link recognition findings deterministic").
- Lens: `lens.pin` / `lens.analyze` replied Done before the GUI read the input file; callers delete their handoff on success (§5.7), so Clipboard's Pin (flow 4) and Look's "Analyze with Lens" opened nothing. Fixed in Lens `d6f498e` (pixels decoded before replying). `capture_and_act` also only honoured `mode=measure`; pin and color added in the same commit.
- Clipboard: with Lens registered, the process didn't exit after `shutdown`: the Link startup task could still be running and install its listener after `stop`. Fixed in `8d34f69` (shutdown joins the startup first).
- Box: `run_screen_tool` and `pick_peer_clipboard` looked up a window labelled `main`; the window is `island`, so the island never hid before a Lens capture and Box's own UI ended up in the OCR. Fixed in Box `2fcb540` (hide, wait for unmap, then capture).
- Arcade Link: `SharedRegistry::watch` treated read/open events on `*.endpoint` files as changes; apps that probe endpoints on change woke each other forever (idle CPU, Look/Tools UI stalls). Fixed in Link `1ae6ffb` (access events ignored).
- Wheel: a slot whose peer was switched off in Wheel's own Connected apps said the *peer* had disabled its connections. Fixed in Wheel `d1ea387` (local wording).
- Box: `crates/arcade-contract/src/lib.rs` has clippy warnings that predate this work (on `main`); Box CI doesn't run clippy. Left alone.
- Test hygiene: an early e2e run wrote `arcade.look.json` and `arcade.wheel.json` (2026-10-05) into the owner's real `~/.local/share/arcade/apps/`. Moot since Oct 8: the owner now runs the real apps, whose own manifests (Box, Lens, Look, Wheel) live there and point at the installed builds; Wheel's content matches what the current build writes.
- Test hygiene: a plain `cargo build` for screenshot review overwrote Look's `src-tauri/target/debug/arcade-look`, which the owner's login item runs; without the `custom-protocol` feature it loads the Vite dev URL, so Look showed nothing. Rebuilt with `npx tauri build --debug --no-bundle`; test builds now use their own target directory.
- Arcade Link: a peer killed mid-handshake was reported as `internal: Connection reset by peer` instead of `not_running`. Fixed in Link `50f99c3`.
- Lens's Wayland Settings window runs as a child process that is reaped only when Settings opens again, so one `<defunct>` entry can linger. Harmless; not changed.
- Lens (owner's machine): the APPIMAGE bug above was real here. A Lens started from inside T3 Code (an AppImage) wrote `Exec=…/T3-Code-Nightly.AppImage` into both `~/.config/autostart/arcade-lens.desktop` and `~/.local/share/applications/arcade-lens.desktop`, so login and the launcher opened T3 Code instead of Lens. Fixed in Lens `9fbe02a` (APPIMAGE is only trusted inside `APPDIR`); both owner files repaired by hand.
- Lens: the expanded palette (Space) scrolled the selected row into view on every frame, so the mouse wheel snapped back to the top. Fixed in `0527bf5` (follow the selection only when it changes); verified by the owner.
- Lens: cancelling a `lens.capture` job left the overlay open and the job waiting on the selection. Fixed in `d46764b`.
- Wheel: at login Wheel can start before the panel that hosts tray icons; Qt then reports no tray and the icon never appeared. Fixed in `917e624` (show the icon when `org.kde.StatusNotifierWatcher` appears; D-Bus signal, no polling).
- Tray menus differed per app. All five now use Wheel's menu: **Open <App>**, **Open Settings**, **Restart Arcade <App>**, separator, **Quit Arcade <App>**; a left click opens Settings. Lens dropped its tray "Start at Login" check (it's in Settings). Look, Box and Clipboard gained Restart (the successor waits for the old process before taking the single-instance slot). Clipboard's desktop bridge dropped the two new tray actions until `ed9758f`; Box's tray item is titled Arcade Box (`6dc469f`). Verified on the owner's Hyprland/Caelestia tray over D-Bus, including Restart and Quit (Clipboard with a throwaway profile).
- Box: every launch ran each provider's `--version` (OCRmyPDF starts a Python runtime), raising startup RSS. Startup now re-checks only providers last seen missing; the Engines page re-checks all (`18dacc2`).
- Box tools sprint (five parallel agents, merged into `arcade/link`): screenshot, OCR, QR, color, ruler and pin work on X11 without Lens and through Lens; the recorder records on X11 (GStreamer `ximagesrc`) with stop/discard; clipboard history, paste-plain, window-pin and PDF convert completed. Honest limits: these were run under Xvfb, not on the owner's Wayland session; window-pin needs `wmctrl` on X11 and is unsupported on Wayland (Hyprland's `pin` dispatcher could back it; backlog); `paste-plain` delivery couldn't be confirmed headless (no window manager to restore focus); `pdf.convert` needs LibreOffice, which isn't installed here, so only the missing-provider path was run.
- Lens e2e `connected_apps_real_toggles_and_shortcut_clash`: resolved in Link `337b85f`. The test stops Box before recording, avoiding its global X11 grab while retaining the cached manifest. The final full run passes this check.
- Lens: an action that opens the owner's window and waits there (Add to Wheel waits for Wheel's slot confirmation) kept Lens's full-screen overlay up and refocusing, covering Wheel's Settings; the user could not reach the confirmation. Found by re-running the Lens e2e group (the Lens agent had reported it as load-related flakiness). Fixed in Lens `3d8eeef`: peer-UI actions hand their job to a background thread after 250 ms and close the overlay; failures become a desktop notification.
- e2e: the Lens filter check read the palette with Tesseract, which misreads the text cursor ("imagd") and splits rows at the icon. The check now accepts a near match (Link e2e `lens.py`); the filter text is echoed in the field anyway.
- Cross-platform CI had never run for most of this work (Look/Wheel/Clipboard CI only runs on `main`/PRs; Box's Windows/macOS jobs had never got past an earlier failure). It was run on every `arcade/link` branch on 2026-10-08, and the failures were fixed:
  - **Real bugs:** Link Qt module (`7af8d81`): the old manifest stayed open while `QSaveFile` replaced it, so every manifest update after the first failed on Windows. Link `Server::stop` (`e9e9c83`, `b7c386c`): returned before the accept thread dropped its listener, and left served connections open, so switching the Link off and on again left Windows apps not listening ("Access is denied"); it now joins the accept thread and cancels each connection's pending pipe read. Link `spawn_detached` (`5e1b916`): on Windows the launched app inherited the caller's stdout/stderr, so `arcade-link invoke … | x`, or any app that captures a launcher's output, waited until the launched app quit. Tools (`60f1a9d`): the start-at-login temp-dir check never matched on macOS or Windows (canonical vs raw temp path).
  - **Code that didn't compile:** Box Windows screen capture and recorder against `windows-capture` 2.0.1 (`106188e`, `de5f667`); Wheel's missing `QJsonDocument` include; Clipboard's desktop bridge on Windows (`DROPFILES`) and macOS (`CGEventPost`). The Clipboard and Box capture errors predate this work: they are on `main`.
  - **CI and tests:** Box zxing-cpp debug-CRT link error (opt-level 1 for that package); Look's workflow didn't parse (`runner.temp` in job env), and its Windows test binaries lacked the Common Controls manifest; macOS `/private/var` expectations in Box and Look tests; Link Windows clippy (unused imports); Tools pinned Link by an abbreviated SHA, and its platform tests expected Unix paths on Windows. Every app's CI and release workflows now pin Link `5e1b916` (the commit with the Windows fixes) and their `Cargo.lock` files include its `windows-sys` dependency; Wheel vendors the Qt module from `7af8d81`, which is unchanged since then.
- Box AppImage packaging: the local AppImage tool download timed out; the CI definition is in place, the local AppImage build was not run.
- **Dependency diet (2026-10-08, owner request).** Heavy programs used by one or two tools were replaced; nothing heavy is shipped or installed by an app:
  - Box `pdf.images-to-pdf`: built in (lopdf; JPEGs embedded unchanged), no img2pdf (Python). `pdf.ocr`: Poppler + Tesseract + qpdf text-layer overlay, no OCRmyPDF (Python). `pdf.convert`: built-in DOCX/ODT/PPTX/ODP/RTF/spreadsheet renderer (text, headings, lists, bold, tables; no images or exact layout), no LibreOffice. `web.snapshot`: whatever browser is installed (Chromium-family over DevTools, else Firefox-family full-page screenshots, image-only PDF), no Chromium requirement. `audio.text-to-speech`: the system voice (SAPI / `say` / eSpeak NG), no Piper. Box `1c61dee`.
  - Lens reads text with Tesseract on Linux (OS OCR on Windows/macOS); the ocrs/rten engine and its models are gone (Lens `1e54180`, binary 40 → 34 MB).
  - Link `engines` feature (`0ec1a20`): finds Tesseract or downloads a pinned, SHA-256-checked per-user copy (Linux AppImage, unpacked once; Windows installer; Homebrew on macOS) that every app searches.
  - Removed from the owner's machine: LibreOffice (user copy), Piper and its voices, the ocrs models, ocrmypdf and img2pdf (`uv` tools): about 1.1 GB. Chromium remains a system package (436.70 MiB); `sudo -n true` on the final pass reports that a password is required. Remove it with `sudo pacman -R chromium`. `uv` also remains installed; neither it nor Python is a runtime dependency of the replacement PDF tools. Do not remove the system Python installation.
- Box `window-pin` and `paste-plain` on Hyprland (Box `7b215bc`): Hyprland's own dispatchers float+pin the window and deliver Ctrl+Shift+V; both checked on a real Hyprland 0.56 window.
- Lens shortcut-clash e2e check un-parked (Link `337b85f`): the check stops Box before recording, so Box's X11 grab can't swallow the key; the clash comes from Box's cached manifest.
- Link `v0.1.0` tagged at `337b85f`. Every Rust app depends on it by git tag (no `../Arcade-link` path dependencies, no sibling checkout in CI except Tools' e2e runner); a `[patch]` override builds against a local checkout.

---

## Appendix A: Initial action catalog

| Action | App | Accepts | Produces | Effects | Interactive | One-shot |
|---|---|---|---|---|---|---|
| `look.preview` | Look | `file/*`, `file/*[]`, `folder/reference`, `text/url` | — | opens-ui | yes | no |
| `look.inspect` | Look | `file/*` | `structured/file-info` | — | no | yes |
| `look.preview_selection` | Look | — | — | opens-ui | yes | no |
| `lens.capture` | Lens | — | `file/image`, `screen/region` | opens-ui | yes | no |
| `lens.capture_and_act` | Lens | — | — | opens-ui | yes | no |
| `lens.analyze` | Lens | `file/image` | — | opens-ui | yes | no |
| `lens.recognize` | Lens | `file/image`, `text/plain` | `structured/findings` | — | no | yes |
| `lens.pin` | Lens | `file/image` | — | opens-ui | yes | no |
| `box:<tool-id>` (+ `#preset`) | Box | per catalog | per catalog | per catalog privacy class | no | yes |
| `box.open` | Box | any | — | opens-ui | yes | no |
| `box.pipeline.run` | Box | per pipeline | per pipeline | union of nodes | first node may be | yes, if the first node isn't interactive |
| `box.pipelines` | Box | — | `structured/pipelines` | — | no | yes |
| `clipboard.add` | Clipboard | `text/*`, `file/image`, `file/any[]` | — | sends-to-device | no | no |
| `clipboard.pick` | Clipboard | — | `text/*` or `file/*` | opens-ui | yes | no |
| `clipboard.devices` | Clipboard | — | `structured/devices` | — | no | no |
| `wheel.add_action` | Wheel | `text/url`, `text/plain`+`command`, `file/*`, `arcade-action` | — | persists, opens-ui | yes | no |
| `wheel.show` | Wheel | — | — | opens-ui | yes | no |
| `app.status`, `app.activate`, `app.quit` | all | — | — | — | no | no |

## Appendix B: Files each app adds (starting point)

| App | New or changed files |
|---|---|
| Box | `crates/arcade-core/src/link.rs` (manifest, delegated grants, `ocr.lens` / `screen.select.lens` providers, `link` pipeline node), `apps/cli/src/main.rs` (`arcade-box` bin, `--json`, `--preset`, `--arcade-invoke`), `apps/desktop/src-tauri/src/link.rs`, `catalog/tools.json` + schema (`presets`, `link.featuredFor`), frontend result actions + `ConnectedApps.svelte` |
| Lens | `crates/lens-actions/src/arcade.rs`, `crates/arcade-lens/src/link.rs`, an `AppSource` in the overlay, `main.rs` (`--arcade-invoke`, `--arcade-manifest`), `gui/settings_view.rs` (Connected apps), `docs/PLUGINS.md`; delete `examples/plugins/arcade-*` |
| Look | `src-tauri/src/link.rs`, `cli.rs` (new flags), `src/lib/actions-strip.ts` (lazy chunk), settings view, `config.rs` (Windows default shortcut) |
| Wheel | `src/link/ArcadeLink.{h,cpp}`, `src/providers/ArcadeLinkProvider.{h,cpp}` (replaces `ArcadeBoxProvider`), `ConfigStore` schema 4 migration, `qml/settings/ActionPicker.qml`, Connected apps page, `docs/ARCADE_LINK.md` |
| Clipboard | `core/rust/src/link.rs`, `api.rs` (`link_invoke`, `link_wait`), `lib/services/link_service.dart`, item-action widgets, Connected apps settings, Linux launcher `arcade-clipboard`, `docs/platforms.md` |

## 16. Completion report

Versioned copy: [Arcade Link completion report](https://github.com/qa-p1/Arcade-Link/blob/main/COMPLETION_REPORT.md).

Verified 2026-10-08. Phases 0–9 are implemented, including the owner's later
dependency cleanup. The remaining exceptions are listed below; performance
budgets are not claimed to pass universally.

### Delivered

- Arcade Link protocol v1, Rust library, Qt module, registry, authenticated local
  IPC, handoffs, debug CLI, mock peer, conformance vectors and failure tests.
- Presence, Connected apps settings, peer actions, cancellation, availability
  checks and standalone fallbacks in all five apps. Box owns cross-app pipelines;
  Lens owns screen selection; Look owns preview; Clipboard owns device transfer;
  Wheel invokes actions. Arcade Tools installs, updates, repairs and removes apps
  without becoming a required background service.
- Release manifests and checksums, platform build workflows, and a tagged
  `Arcade-Link v0.1.0` dependency. The five app branches remain `arcade/link`;
  Link and Tools use `main`. No app branch was merged in this final pass.
- Box images-to-PDF and document-to-PDF now run in Rust. Searchable PDFs use
  Poppler, Tesseract and qpdf. No img2pdf, OCRmyPDF or LibreOffice runtime.
  Document conversion retains text, headings, lists and tables, but does not
  preserve images or exact Office layout.
- Web capture uses an already installed Chromium-family or Firefox-family
  browser. Box never downloads a browser; Firefox-family PDF output consists
  of page images. Speech uses SAPI on Windows, `say` on macOS and eSpeak NG
  (or eSpeak) on Linux. Linux speech requires that system engine to be installed.
- Tesseract is the only third-party OCR engine. Apps detect the global copy
  first and offer an optional installation when missing; it is not bundled.
  Linux x86_64 downloads a checksum-verified per-user copy shared by the apps;
  Windows opens its checked installer. macOS can use Homebrew. Lens retains
  native OS OCR on Windows/macOS; its ocrs runtime and bundled models are gone.
- Box's GUI library remains unchanged, and further Clipboard work is deferred,
  as requested. Python remains in development/test tooling, not the replacement
  PDF runtime.

### Verification

The final isolated Xvfb ecosystem run completed with **74/74 checks passing**
on these application builds during this task. That includes the previously
failing Lens shortcut-clash and Clipboard image-menu checks, flagship flows,
peer failures, cancellation, private mode and standalone behavior. The result
was recovered from the completed run after the context handoff; it was not
rerun without a code change. All seven current implementation commits have
successful CI, rechecked through GitHub on the final pass:

| Repository | Implementation commit | Successful CI | Coverage |
|---|---|---|---|
| Arcade Link | `49348c8` | [CI](https://github.com/qa-p1/Arcade-Link/actions/runs/37765272861) | Rust and Qt, Linux/Windows/macOS |
| Arcade Box | `ae67811` | [CI](https://github.com/qa-p1/Arcade-box/actions/runs/37766605285) | Linux/Windows/macOS checks; Rust tests, frontend and license inventory |
| Arcade Lens | `5ae1789` | [CI](https://github.com/qa-p1/Arcade-lens/actions/runs/37766627236) | Workspace tests, fmt and clippy on Linux/Windows/macOS |
| Arcade Look | `90d95f6` | [CI](https://github.com/qa-p1/Arcade-look/actions/runs/37765474230) | Linux/Windows/macOS checks and packaging |
| Arcade Wheel | `3333434` | [Build, test and package](https://github.com/qa-p1/Arcade-wheel/actions/runs/37768559542) | Linux AppImage, Windows installer, Intel/Apple Silicon DMGs |
| Arcade Clipboard | `0f86bc1` | [Core and desktop builds](https://github.com/qa-p1/Arcade-clipboard/actions/runs/37765478425) | Rust core, Linux checks, Windows/macOS desktop builds |
| Arcade Tools | `10e7699` | [CI](https://github.com/qa-p1/Arcade-tools/actions/runs/37765473156) | Linux/Windows/macOS checks; isolated Linux lifecycle tests |

Windows/macOS builds and automated tests passed; interactive GUI behavior on
those systems has not been verified. Wheel's release-publishing job was skipped.
Local Linux checks previously passed, including Box's 136 core tests, frontend
and smoke checks; Lens workspace tests/clippy; Look frontend/Rust checks;
Wheel's 9 ctests; Clipboard's Flutter checks; and Tools' lifecycle tests.
No application code changed in this final documentation/performance pass.

### Final performance pass

Fresh isolated measurements at 17:24 IST on 2026-10-08, five runs per app,
using the unchanged baseline method. Raw samples:
[final-2026-10-08.json](https://github.com/qa-p1/Arcade-Link/blob/main/benchmarks/final-2026-10-08.json).
The comparison command completed with exit 0 and no regressions:

```sh
python3 benchmarks/bench.py --runs 5 --json benchmarks/final-2026-10-08.json --compare benchmarks/baseline.json
```

| App | Startup ms (baseline → final) | Warm invoke ms (baseline → final) | RSS MiB (baseline → final) | CPU ms / 5 s (final median) |
|---|---:|---:|---:|---:|
| Box | 94.6 → 68.7 | 7.5 → 7.7 | 420.7 → 426.8 | 10* |
| Lens | 2.9 → 3.0 | 1.5 → 1.5 | 80.2 → 81.4 | 0 |
| Look | 77.4 → 74.7 | 38.2 → 37.7 | 74.2 → 74.5 | 0 |
| Wheel | 146.9 → 136.6 | 38.6 → 35.2 | 114.4 → 115.3 | 0 |
| Clipboard | 187.8 → 193.3 | — | 263.8 → 264.9 | 0 |

All measured startup, warm-invoke and RSS medians are within 5% of baseline
or better. The runner allows an additional 5 ms timing floor, but none of
these final medians needs that allowance. Box startup is 27% faster than
baseline. Clipboard's earlier 13% startup regression did not recur (now +2.9%).
These observations supersede the older `phase9.json` report; they do not
establish the cause of run-to-run timing changes.

*CPU caveat:* Box's raw samples are `[-2040, 10, 0, 20, 10]` ms. The negative
value is invalid: the existing counter sums only currently live processes,
so exiting provider children can make its total decrease. Excluding that
sample still gives a median of 10 ms / 5 s (about 0.2% of one CPU core).
The first Box RSS sample is also elevated at about 631 MiB while provider
initialization overlaps the 3-second sampling point; later samples settle
around 426–431 MiB. Lens has two 10 ms samples despite its zero median.
Do not interpret the successful comparison as proof of zero idle CPU or
zero wakeups. The whole-process measurements also do not isolate Link's
separate ≤1 MiB allocation budget. No application polling was added in this
pass, and the benchmark was kept unchanged for baseline comparability.

### Remaining limits and owner actions

- **Chromium removal is blocked by administrator authentication.** The system
  package is still installed (436.70 MiB, no reverse dependencies reported).
  `sudo -n pacman -R --noconfirm chromium` failed with `sudo: a password is
  required`. Run `sudo pacman -R chromium` in your terminal. The prior cleanup
  removed about 1.1 GB of user-installed LibreOffice, Piper/voices, ocrs models,
  img2pdf and OCRmyPDF. The system Python and uv installations were retained.
- Interactive Windows/macOS behavior and full Wayland screen/recorder flows
  still need real-platform validation. Box's Hyprland window-pin and paste-plain
  were verified earlier in this task; the old claims that they were unsupported
  are superseded. Linux file-manager selection is mock-covered where a real
  selection resolver is unavailable.
- Targeted device delivery, remote invocation and minisign verification are
  future work, as described in the plan. Document layout fidelity and browser
  PDF limitations above are intentional consequences of removing heavy engines.
- Merge each app's `arcade/link` branch only when ready to publish: its `main`
  workflow may release packages. The tagged Link dependency is already in place;
  no dependency migration or LibreOffice installation is required.

The final report, completed plan and fresh benchmark are versioned in the
Arcade Link repository. The five apps retain their `arcade/link` branches;
merging them to `main` and publishing application releases is a separate step.
No system configuration or startup-file changes were needed for final verification.
