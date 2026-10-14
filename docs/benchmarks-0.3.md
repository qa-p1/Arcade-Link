# Tools hub benchmarks (Link 0.3)

Plan §18 budgets, measured with `tools/real_bench.py` in an isolated session.
That means Xvfb, a private D-Bus bus and temporary `HOME`/`XDG_*`/`ARCADE_HOME`.
No live profile or real desktop was involved. Anything not measured here is
marked **not measured**, with the reason. No number on this page is an
estimate.

## Method

```sh
cargo build -p arcade-link --example startup-probe --features install,trayhost --locked
cmake --build qt/build --target startup-probe
ARCADE_TOOLS_REPO=<Tools arcade/tools-hub checkout> python3 tools/real_apps.py   # Tools core adapter
ARCADE_TOOLS_REPO=... ARCADE_CATALOG_REPO=<Arcade-catalog checkout> \
  python3 tools/e2e.py run -- python3 tools/real_bench.py --runs 20 --idle-seconds 30 \
    --json /tmp/arcade-bench/bench.json --only linkIdle,protocolTrayHandback,startup,qtStartup,shortcutFixtureIndex
```

Each probe writes its method, its raw samples and the build heads into the
JSON. The probes are:

| Probe | What it times | What it excludes |
|---|---|---|
| `startup` | Rust `install::detect()` plus `TrayHostWatcher::start` returning, on the caller's thread (debug build) | worker completion, icon paint, whole-app startup |
| `qtStartup` | Qt `detect()` plus `ArcadeTrayHost` construction/start returning | worker completion, icon paint |
| `protocolTrayHandback` | released Rust tray watcher; SIGKILL of a CLI tray host until the `own` notification | process startup, real app icons, the Tools GUI |
| `linkIdle` | the released Link server (`arcade-link mock --as find`), process-tree RSS and CPU ticks from `/proc` over 30 s, after a 3 s settle, with no probes during the window | real app GUIs; this isn't the Tools process |
| `shortcutFixtureIndex` | Tools' actual index builder (adapter from Tools `a786164`) over the eight app shortcut documents, catalog sheets and the Hyprland Lua + `hyprctl binds -j` fixture | adapter process startup; live app manifests |
| `trayHandback`, `toolsIdle`, `shortcutIndex`, `overlay`, `toolsWindow` | the same budgets against the real running fleet and Tools | they need current builds of every app and Tools; see "Not measured" |

## Results: 2026-10-14

The harness ran at Link `95790c6` plus the probe changes committed as `b8bdc8c` (the shipped Link v0.3.0 code is unchanged), from a debug build under Xvfb on a 12-thread
x86_64 machine (CachyOS, kernel 7.2.8). n = 20 unless noted. "p95" is the
19th of 20 sorted samples.

| §18 budget | Probe | Median | p95 | Max | Budget | Met |
|---|---|---|---|---|---|---|
| Startup added by tray watcher + `detect()` ≤ 5 ms | `startup` (Rust, combined) | 0.129 ms | 0.152 ms | 0.176 ms | 5 ms | yes |
| | of which `detect()` | 0.067 ms | | 0.112 ms | | |
| | `qtStartup` (Qt, combined) | 0.212 ms | 0.232 ms | 0.253 ms | 5 ms | yes |
| | of which `detect()` | 0.141 ms | | 0.174 ms | | |
| Tray hand-back ≤ 1 s (target ≤ 200 ms) | `protocolTrayHandback` | 0.257 ms | 0.317 ms | 0.321 ms | 1000 ms | yes (protocol only) |
| Zero idle cost | `linkIdle` (Link server only) | | | | 0 ticks | 0 CPU ticks over 30.0 s, RSS 5.5 MiB |
| Shortcut index rebuild ≤ 150 ms | `shortcutFixtureIndex` | 19.9 ms | 20.4 ms | 29.3 ms | 150 ms | yes (fixture sources) |

These numbers bound Link's share of each budget. They don't show that the
whole product meets it. The hand-back figure in particular stops at the
watcher callback: an app repainting its own icon isn't included.

## Not measured

| §18 budget | Why it isn't measured |
|---|---|
| Tray hand-back with the real fleet (`trayHandback`) | Needs current native builds of all seven apps and Tools in one session. The local worktree builds are stale against their sources, and Tools publishes no Linux branch AppImage. The e2e group `trayhost` (`real_apps_hand_back_within_one_second_after_tools_sigkill`, `<app>_hosting_crash_opt_out`) asserts ≤ 1 s with the real apps when they're available, but it isn't a benchmark. |
| Tools idle (≤ 40 MiB RSS, 0 ticks / 30 s) | Needs a current Tools build. Not re-measured for this report. An earlier run of the same probe (Tools `a7d0904`, debug build, update checks off, webview closed) recorded 188.6 MiB RSS and 0 CPU ticks over 30 s. That's over the 40 MiB budget, but it was a debug build and the result hasn't been re-verified. Re-run `toolsIdle` on a release build before drawing conclusions. |
| Tools window → first paint ≤ 400 ms warm (`toolsWindow`) | Needs a current Tools build. The earlier attempt got no DOM `requestAnimationFrame` callback under Xvfb. |
| Overlay key press → first frame ≤ 80 ms p95 (`overlay`) | Needs a current `arcade-tools-overlay` build. Xvfb also can't measure a physical compositor's display latency. The owner's §17.3 item 3 covers it on the real desktop. |
| Full live-fleet index (`shortcutIndex`) | Needs all seven apps running. Only the fixture-based index above was timed. |
| Existing app budgets (no regression > 5%) | `benchmarks/bench.py --compare` needs before/after builds of every app; not run for this PR. |
| Linux Wayland, Windows, macOS | Not measured. All numbers are Linux X11 under Xvfb. |
