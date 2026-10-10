# Arcade Link

How the Arcade apps (Box, Lens, Look, Wheel, Clipboard and the optional
manager, Tools) recognize each other and work together: a file-based
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
- [`crates/arcade-link-cli`](crates/arcade-link-cli): `arcade-link`, the debug
  CLI and mock peer.
- [`qt/`](qt): `ArcadeLink.{h,cpp}` for Arcade Wheel (vendored into Wheel's
  `src/link/`), with its own tests.
- [`spec/vectors/`](spec/vectors): conformance vectors both implementations
  run.
- [`fixtures/`](fixtures): mock-peer fixtures for each app.
- [`assets/`](assets): app glyphs and `tokens.json` for integration surfaces.
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
```

Every command honors `ARCADE_HOME`, so tests and experiments never touch the
real registry:

```sh
export ARCADE_HOME=$(mktemp -d)
arcade-link mock --as box --actions fixtures/box.json &
arcade-link invoke box box:arcade.text.structured --preset format-json --text '{"a":1}'
```

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

## Testing

```sh
cargo test --workspace                       # unit, vectors, server/client, CLI + mock
cmake -S qt -B qt/build -G Ninja && cmake --build qt/build
ARCADE_LINK_CLI=$PWD/target/debug/arcade-link ctest --test-dir qt/build   # + Rust interop
```

Across the apps, everything runs in a disposable desktop session (private
D-Bus, Xvfb, temporary HOME/XDG/`ARCADE_HOME`), never on the real desktop:

```sh
python3 tools/e2e.py                          # every app's checks (74 on 2026-10-08)
python3 tools/e2e.py --only lens,box          # some groups
python3 tools/e2e.py run -- python3 tools/stress.py   # concurrency, kill mid-burst, memory
python3 tools/e2e.py run -- python3 tools/leak.py     # ten rounds of load, RSS per round
python3 benchmarks/bench.py --json now.json --compare benchmarks/baseline.json
```

## Status

| Platform | Rust crate | Qt module |
|---|---|---|
| Linux | tested (this repository's tests) | tested |
| Windows | native CI build and tests passed | native CI build and tests passed |
| macOS | native CI build and tests passed | native CI build and tests passed |

Interactive Windows/macOS app flows remain unverified. See [current status](docs/STATUS.md)
for evidence and the documentation index.

The apps use the published `v0.2.0` git tag (see the [changelog](CHANGELOG.md));
Wheel and Shelf vendor the matching Qt module. See the [completed ecosystem plan](ARCADE_ECOSYSTEM_PLAN.md) and
[completion report](COMPLETION_REPORT.md) for verification results, performance
measurements and remaining platform limitations.

## Release manifests

`tools/arcade-release.py` writes `arcade-release.json` (schema:
`spec/arcade-release.schema.json`, plan §11) and `SHA256SUMS.txt` for a
directory of release files. Each app's release pipeline vendors it (standard
library only):

```sh
python3 arcade-release.py --id arcade.look --version 0.4.0 --channel stable \
  --notes https://github.com/qa-p1/Arcade-look/releases/tag/v0.4.0 dist/
```
