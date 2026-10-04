# Phase 0 baseline

Recorded 2026-10-05 on the development machine (Arch Linux, 12 threads,
14 GiB), before any Arcade Link code, from these commits:
Box `f5fc8f6` (+ the CI fix, which doesn't touch runtime code paths),
Lens `67f6803`, Look `c526b50`, Wheel `602136e`, Clipboard `a43a6cc`.

Raw numbers: [`baseline.json`](baseline.json). Re-run with
`python3 benchmarks/bench.py --json now.json --compare benchmarks/baseline.json`;
it fails if startup, warm invoke or idle RSS regresses by more than 5 %
(startup and warm invoke get a 5 ms noise floor).

Every run is isolated: private D-Bus session, Xvfb, temporary HOME, XDG and
`ARCADE_*` directories, no Wayland or Hyprland variables. Release builds,
except Wheel, which uses the existing Debug `build/` directory (the plan
says to reuse it). Medians of 5 starts after one warm-up start (first-run
setup excluded).

| App | Startup (ms) | Warm invoke (ms) | Idle RSS (MiB) | Idle CPU (ms / 5 s) |
|---|---:|---:|---:|---:|
| Box (`arcade-desktop`) | 94.6 | 7.5 | 420.7 | 0 |
| Lens (`arcade-lens --background`) | 2.9 | 1.5 | 80.2 | 0 |
| Look (`arcade-look --service`) | 77.4 | 38.2 | 74.2 | 0 |
| Wheel (`arcade-wheel --background`, Debug) | 146.9 | 38.6 | 114.4 | 0 |
| Clipboard (`clipboard --background`) | 187.8 | — | 263.8 | 0 |

## What each figure means

| App | Startup ends when | Warm invoke |
|---|---|---|
| Box | its window exists (`xdotool search --pid`) | `arcadebox run arcade.developer.hash abc --json` (CLI, warm cache) |
| Lens | its existing IPC answers `ping` | `arcade-lens --background` against the running instance |
| Look | it owns `org.gnome.NautilusPreviewer` on the session bus | `arcade-look --service` against the running instance (single-instance plugin) |
| Wheel | its command socket answers `--status` | `arcade-wheel --status` |
| Clipboard | its core holds the profile lock (profile opened) | none: isolated profiles (`ARCADE_DATA_DIR`) run non-unique, so there is no second-instance channel |

- Idle RSS: resident memory of the whole process tree 3 s after ready
  (Box's figure includes its WebKit processes).
- Idle CPU: CPU time of the whole tree during the following 5 s.
- Lens's startup ends early by design: its IPC listener is bound before the
  GUI starts.
