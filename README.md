# Arcade Link

How the seven independent Arcade apps (Box, Lens, Look, Wheel, Clipboard,
Find and Shelf) and the optional host, Tools, recognize each other and work together: a file-based
registry, one local socket per running app, one shared vocabulary. A protocol
and a small library, not a process.

- [`NEW_APP_SPEC.md`](NEW_APP_SPEC.md): how to build a new Arcade app; give it to an agent with your idea.
- [`SPEC.md`](SPEC.md): the protocol (version 1, frozen), manifests, content
  types, errors and standard messages, lifecycle, the shared CLI flags and
  the Connected apps page.
- [`crates/arcade-link`](crates/arcade-link): the Rust library (registry,
  endpoint, client, server, content, handoff, one-shot). Used by Box, Lens,
  Look, Clipboard's core and Arcade Tools. MSRV 1.88; dependencies: `serde`,
  `serde_json`, `interprocess`, `getrandom` (+ `notify` with the `watch`
  feature, `sha2` with `engines`). The `engines` feature finds helper programs
  the apps use but never ship (Tesseract) and downloads a checksummed per-user
  copy into `<data>/arcade/engines`, which every app searches.
- Rust v0.3 adds optional app control methods, canonical accelerators,
  shortcut documents/sheets and receipts. The `trayhost` feature adds the
  background tray watcher and Tools subscriber helper; `install` adds
  explicit-path installation, repair, uninstall, native helpers and shared
  dialog strings. Protocol and manifest schema stay 1; v0.2 public structs,
  methods and feature defaults remain source compatible.
- [`crates/arcade-link-cli`](crates/arcade-link-cli): `arcade-link`, the debug
  CLI and mock peer.
- [`qt/`](qt/README.md): the C++20/Qt 6.8 port for Wheel and Shelf, including
  optional methods, `ArcadeTrayHost`, `ArcadeInstall`, accelerator and shortcut
  helpers, with shared-vector and Rust interop tests. Use
  [`tools/vendor_qt.py`](tools/vendor_qt.py) for complete commit/checksum pins.
- [`spec/vectors/`](spec/vectors): conformance vectors both implementations
  run.
- [`fixtures/`](fixtures): mock-peer fixtures for each app.
- [`design/`](design/README.md): the Arcade design kit (tokens, the CSS, QML,
  Dart and Rust kits, Inter, icons, app marks and launcher icons, default
  shortcuts) and [`design/STANDARD.md`](design/STANDARD.md), the UI and project
  standard every app follows.
- [`assets/`](assets/README.md): generated copies of the glyphs and the 0.3
  `tokens.json` keys for older integration surfaces.
- [`assets/strings/install.json`](assets/strings/install.json): verbatim
  shared first-run strings, exposed through `install::strings`.
- [`packaging/windows`](packaging/windows/README.md): checksum-pinned Inno
  and NSIS includes, PATH/receipt helper and silent-install CI samples.
- [`tools/validate_shortcuts.py`](tools/validate_shortcuts.py): stdlib-only
  validator and Markdown generator for apps and catalog sheets.
- [`tools/e2e.py`](tools/e2e.py): the ecosystem end-to-end run; [`tools/stress.py`](tools/stress.py) and [`tools/leak.py`](tools/leak.py), the stress and memory runs.
- [`benchmarks/`](benchmarks): the Phase 0 baseline and the regression runner.

## The debug CLI

```sh
cargo build -p arcade-link-cli
arcade-link ls                                   # installed apps and their state
arcade-link describe box                         # actions (live if running)
arcade-link invoke box box:arcade.image.convert --preset webp --file shot.png
arcade-link invoke look look.preview --file report.pdf
arcade-link watch                                # registry and app.changed events
arcade-link mock --as box --actions fixtures/box.json   # a scriptable fake Box
arcade-link tray find                            # prints own/hosted/none changes
arcade-link tray host                            # a fake Tools host, stdin control
arcade-link shortcuts                            # effective globals and conflicts
arcade-link shortcuts validate docs/user/shortcuts.json --manifest manifest.json
arcade-link shortcuts markdown docs/user/shortcuts.json
arcade-link settings find
arcade-link restart find
arcade-link call find app.menu --params '{}'
```

Every command honors `ARCADE_HOME`, so tests and experiments never touch the
real registry:

```sh
export ARCADE_HOME=$(mktemp -d)
arcade-link mock --as box --actions fixtures/box.json &
arcade-link invoke box box:arcade.text.structured --preset format-json --text '{"a":1}'
```

`ls` also shows receipt/install method and live tray state (unknown if an
older app omits it). The fake host accepts `hosted true`, `hosted false`,
`restarting`, `exclude arcade.find`, `include arcade.find`, `quit`, and
`crash` on stdin; `subscribers` reports connected app IDs for deterministic
test synchronization. The app-side `tray` command accepts `link off`, `link on`
and `quit`. Mock fixtures accept `menu`, `methods` (restrict optional
methods), and `mockMethods` (script a method's `result` or `error`). Menu
toggles and shortcut changes update the manifest; export/import use private
handoff files and back up imported settings. The Tools fixture can act as
a tray host with the same stdin commands.

## Using the crate

```rust
use arcade_link::{ids, Action, Locations, Manifest, Presence};

// After the first frame, on a background thread:
let mut manifest = Manifest::new(ids::LOOK, env!("CARGO_PKG_VERSION"), &arcade_link::manifest::current_executable());
manifest.actions.push(Action::new("look.preview", "Quick Look", "preview").accepts(&["file/*"]).effects(&["opens-ui"]).interactive(true));
let presence = Presence::start(Locations::discover(), manifest, handler);
```

Calling a peer (from a worker thread, never the UI thread):

```rust
let registry = arcade_link::Registry::load(&locations);
let box_app = registry.get(ids::BOX).unwrap();
let request = InvokeRequest::new("box:arcade.image.convert", ids::LENS).preset(Some("webp")).input(Content::file(path));
let result = arcade_link::invoke_action(&locations, &me, box_app, &request, Default::default())?;
```

New APIs are opt-in:

- `Handler::methods()` advertises optional methods. Override its settings,
  restart/readiness, menu/invoke, shortcuts_set, settings_export/import
  callbacks. They run on connection workers; marshal UI operations to the
  toolkit. `Client` exposes matching typed helpers, `describe_full()` and
  `LinkError::is_unsupported()` for old peers.
- `manifest::ManifestDocument` wraps the unchanged `Manifest` with
  `ManifestAdditions` (menu/logs/docs/install and nested settings.trayHost).
  Use `Presence::start_document`/`update_document`; existing start/update
  keep compiling. `Registry::document`/`additions` expose the extension cache.
- `accelerator::{normalize,display,conflicts}` implement SPEC §8.5, including
  sequences, modifier taps, XKB/Hyprland aliases and platform display.
  `shortcuts::{Document,Sheet}` validate and generate Markdown; app validation
  can check rebindable IDs against a manifest.
- `receipt::{Receipt,Store}` provide validated, atomic private receipts,
  independently of the install feature. Dev receipts cannot be written.
- `install::{Environment,Runtime,detect,install,repair,uninstall,
  integration_status,reexec}` separate explicit filesystem paths, detection
  and launch policy. `Environment::under(temp_root)` isolates tests;
  `discover()` honors ARCADE_HOME and XDG. Linux installs are reversible
  on integration failure, retain the prior AppImage, and never edit profiles.
  Native helpers in `install::platform` provide Windows user PATH/shims and
  macOS translocation/bundle/Applications/CLI checks and moves.
- With `trayhost`, `TrayHostWatcher::start(config, callback)` is nonblocking.
  Callbacks arrive on a background worker. Use state for app.status, and
  set_link_enabled for the app switch. Keep the tray object and hide/show it.
  Tools uses `Server::start_with_tray_host` to attach a `TrayHostServer`
  before accepting subscribers or advertising hosting,
  uses set_hosted/set_excluded/announce_restarting, and calls shutdown on quit.
  Only the 300 ms startup and 5 s restart deadlines use timers; idle blocks.

There are no default features. `trayhost` enables `watch`; `install`,
`watch` and `engines` can each be enabled independently. Installation never
launches implicitly; the app owns consent, version comparison, UI and the
restart successor's old-process wait. Qt mirrors the same JSON and state
transitions; its watcher emits queued signals on the object's owning thread.
Qt server callbacks run on that thread, so apps must dispatch long operations
to workers. Client calls and install file operations belong on worker threads.
See [the Qt API and vendoring guide](qt/README.md).

## Testing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features
cargo test -p arcade-link --no-default-features
# Also run with --no-default-features --features install/trayhost/watch/engines, one at a time.
python3 -m unittest discover -s tools -p 'test_*.py'
cargo build -p arcade-link-cli
cmake -S qt -B /tmp/arcade-link-qt-core -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/arcade-link-qt-core
ARCADE_LINK_CLI=$PWD/target/debug/arcade-link ctest --test-dir /tmp/arcade-link-qt-core --output-on-failure
```

Across the apps, everything runs in a disposable desktop session (private
D-Bus, Xvfb, temporary HOME/XDG/`ARCADE_HOME`), never on the real desktop:

```sh
python3 tools/e2e.py                          # every app's checks (74 on 2026-10-08)
python3 tools/e2e.py --only lens,box          # some groups
python3 tools/e2e.py --only presence,trayhost,install,shortcuts # headless Phase 1 scaffold
python3 tools/e2e.py run -- python3 tools/stress.py   # concurrency, kill mid-burst, memory
python3 tools/e2e.py run -- python3 tools/leak.py     # ten rounds of load, RSS per round
python3 benchmarks/bench.py --json now.json --compare benchmarks/baseline.json
python3 benchmarks/bench.py --trayhost --runs 10 --json tray-handback.json
python3 benchmarks/bench.py --hub-scaffold --runs 10 --json hub-scaffold.json
```

## Status

Rust, Qt, Python and the headless Phase 1 e2e scaffold are verified locally on
Linux. The Rust 1.88/Qt three-OS matrix and silent Windows installers run in
[PR #3's CI](https://github.com/qa-p1/Arcade-Link/pull/3/checks); this host uses
system Rust 1.99. The new e2e selection uses mock presence, crate-backed tray
watchers, a temporary install harness and sibling shortcut documents/sheets.
Existing real-app groups remain available; their binaries are checked for
missing/stale builds before use. Interactive app install/tray flows and the
`tools-hub` group belong to app/Tools adoption. See [current status](docs/STATUS.md) and the
[v0.3 implementation report](docs/dev/history/V0.3_REPORT.md) for evidence and limits.

The apps currently use the published `v0.2.0` git tag (see the [changelog](CHANGELOG.md));
Wheel and Shelf vendor the matching Qt module. See the [completed ecosystem plan](docs/dev/history/ARCADE_ECOSYSTEM_PLAN.md) and
[completion report](docs/dev/history/COMPLETION_REPORT.md) for verification results, performance
measurements and remaining platform limitations.
This branch prepares v0.3.0 without moving that tag or publishing a release.

## Release manifests

`tools/arcade-release.py` writes `arcade-release.json` (schema:
`spec/arcade-release.schema.json`, plan §11) and `SHA256SUMS.txt` for a
directory of release files. Each app's release pipeline vendors it (standard
library only):

```sh
python3 arcade-release.py --id arcade.look --version 0.4.0 --channel stable \
  --notes https://github.com/qa-p1/Arcade-look/releases/tag/v0.4.0 dist/
```

Linux/macOS assets can carry `installArgs: ["--install","--silent"]`.
Windows portable assets carry `kind: "portable"`; Tools ignores those for
managed installation. Use `--portable FILE` for an otherwise ambiguous
portable asset; existing release fields remain unchanged.

## Licence

The code is available under the MIT licence or the Apache License 2.0, at
your option ([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)).
The bundled Inter typeface in [`design/fonts/`](design/fonts) is © The Inter
Project Authors under the SIL Open Font License 1.1
([`design/fonts/OFL.txt`](design/fonts/OFL.txt)); apps that ship it include
that file with their licence notices.
