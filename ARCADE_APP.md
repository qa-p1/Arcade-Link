# The Arcade app standard

Normative. Every Arcade app MUST meet each MUST and SHOULD meet each SHOULD. Each rule has the ID of the
`arcade-link conformance` check that enforces it: `S` checks read the repository or package, `R` checks run the
binary in an isolated HOME, `H` checks run in the desktop harness (`.github/workflows/desktop-e2e.yml`), and `D` rules
are reviewed by hand (no automatic check yet). The how-to for each area, examples and the history behind the rules
are in [`docs/ARCADE_APP_GUIDE.md`](docs/ARCADE_APP_GUIDE.md) (formerly `NEW_APP_SPEC.md`). The wire protocol is
in [`SPEC.md`](SPEC.md).

Create a new app from the template (`template/`, published as `qa-p1/Arcade-template`) with the
[`/new-arcade-app`](skills/new-arcade-app/SKILL.md) skill.

## Identity

| ID | Rule |
|---|---|
| ID-1 (S) | MUST: the Link id is `arcade.<name>` (lower case), the desktop id, window class / Wayland app_id and icon name are `dev.arcade.<Name>` (one spelling, capitalised), and the repo is `Arcade-<Name>`. |
| ID-2 (S) | MUST: exactly one launcher entry (`dev.arcade.<Name>.desktop`) and at most one autostart entry, both written from the install receipt, `Exec=` pointing at the installed AppImage or launcher. |
| ID-3 (S) | MUST: the package carries the app icon at 256 px or as SVG, and a symbolic tray icon. |

## Command line

| ID | Rule |
|---|---|
| CLI-1 (R) | MUST: `--status` prints one line of JSON (`{"app","version","running",...}`) and exits 0 (or 3, the LSB code for "not running"), without opening a window, registering, or starting a resident instance. |
| CLI-2 (R) | MUST: an unknown argument prints usage to stderr and exits 2. `--help` prints usage to stdout and exits 0. |
| CLI-3 (R) | MUST: `--version` prints `<Name> <semver>` and exits 0. |
| CLI-4 (R) | MUST: `--arcade-manifest` prints the Link manifest JSON and creates nothing. |
| CLI-5 (D) | MUST: `--background`, `--settings`, `--quit`, `--restart`, `--install`, `--uninstall [--remove-data]`, `--repair`, `--integration-status` behave as in the guide §4. |

## Process

| ID | Rule |
|---|---|
| RT-1 (H) | MUST: one resident instance per profile. A second launch hands its command to the first and exits 0. Rust: `runtime::acquire_instance`. |
| RT-2 (S,H) | MUST: restart re-executes `$APPIMAGE` (or the installed launcher), never a binary inside the AppImage mount, with a sanitized environment; the successor waits for its predecessor. Rust: `runtime::spawn_successor`, `childenv::sanitize`. |
| RT-3 (H) | MUST: every child process is reaped; none is left as a zombie. Rust: `runtime::spawn_reaped`. |
| RT-4 (H) | MUST: a panic or fatal signal writes a local report under `arcade/<app>/crashes/` (newest 20 kept); nothing is sent anywhere. Rust: `runtime::install_crash_reports`. |
| RT-5 (H) | MUST: trim memory after startup and after each hide (`runtime::trim`, C: `qt/arcade_trim.h`); WebKit apps call `runtime::release_webkit` after the last window closes. |
| RT-6 (H) | MUST: nothing polls in the background without a reason; a timer runs only while its view is visible. Idle CPU ≈ 0 % with bounded wakeups. |

## Files

| ID | Rule |
|---|---|
| FS-1 (R) | MUST: config, data, cache and logs live only under `arcade/<app>` in the platform folders (`paths::app_dirs`); legacy folders are migrated once and retired (`spec/app-dirs.json`). |
| FS-2 (D) | MUST: every settings and data file carries a schema version; forward migrations are tested from every released version; a downgrade keeps unknown fields or restores the backup taken at the switch, and says so. |
| FS-3 (D) | MUST: writes are atomic; a destructive migration keeps a backup until the next successful start. |
| LOG-1 (R) | MUST: logs have levels, rotate, contain no secrets or user content, and drop known-harmless toolkit noise. |

## Link

| ID | Rule |
|---|---|
| LINK-1 (R) | MUST: register with Link (`SPEC.md` §3), speak protocol 1, and send `protocol` in `hello`. |
| LINK-2 (S) | MUST: pin Arcade Link by tag (`v1.x.y`), never by branch or rev. |
| LINK-3 (D) | MUST: work with the previous released version of every peer (N-1). |

## Desktop behaviour

| ID | Rule |
|---|---|
| UI-1 (H) | MUST: popups and overlays close only on Escape, choosing an item, the shortcut again, or an explicit outside click; never on pointer leave or focus change. |
| UI-2 (H) | MUST: Escape goes back one step: a sub-view returns to its parent, a search with text clears it, a destructive dialog is cancelled, and only the top empty view closes. Footer hints say exactly what keys do. |
| UI-3 (H) | MUST: the tray menu is Open <App>, Open Settings, Restart Arcade <App>, separator, Quit Arcade <App>; the tray is handed to Tools when it hosts and taken back within 1 s. |
| UI-4 (D) | MUST: global shortcuts use the portal or Hyprland runtime bindings, warn about clashes, and fall back to a documented manual binding. |
| UI-5 (D) | MUST: use the Link design kit (tokens, fonts, icons, wording); look the same as the fleet in empty, list, settings, error, light and dark states. |
| A11Y-1 (D) | MUST: everything is reachable by keyboard, focus is visible, text and controls meet WCAG AA contrast, layouts work at 125–150 % scaling, and reduced motion is respected. |

## Budgets

| ID | Rule |
|---|---|
| PERF-1 (H) | MUST: background-idle memory within the app's budget (PSS/RSS MB): Tools 30/50, Box 60/80, Look 50/70, Find 42/48, Lens 8/8, Shelf 40/48, Wheel 30/50, Clipboard 105/130; a new app gets a budget in this table before release. All apps together stay under 500/500 in the background. |
| PERF-2 (H) | MUST: cold start, show latency and disk use within the budgets in the app's spec; the installer cache is bounded. |

## Repository and release

| ID | Rule |
|---|---|
| REPO-1 (S) | MUST: `LICENSE` (or `LICENSE-*`), `THIRD_PARTY_NOTICES` (generated, shown in About), `SECURITY.md` and `CHANGELOG.md` (Keep a Changelog) are present. |
| REPO-2 (S) | MUST: CI runs a dependency audit and licence check (`cargo deny`, or the pub/Qt equivalent) and Dependabot or Renovate covers dependencies and GitHub Actions. |
| REL-1 (S) | MUST: the workflows `ci.yml`, `release.yml`, `promote.yml` and `nightly.yml` call Link's reusable workflows; every network step has a timeout and a retry. |
| REL-2 (S) | MUST: releases publish `manifest.json` (and `manifest.json.sig` for stable and nightly) from `arcade-link release-manifest`, next to the `arcade-release.json` and asset names Tools 0.4.x reads. |
| REL-3 (D) | MUST: stable is a byte-for-byte promotion of a verified `X.Y.Z-rc.N` on the hidden `test` channel; nothing is rebuilt for stable. |

## Running the checker

```bash
arcade-link conformance path/to/app-repo --binary path/to/built-app
```

```bash
arcade-link conformance path/to/App-x86_64.AppImage
```

It prints one line per check (`PASS`, `FAIL`, `SKIP` with the reason) and exits 1 if any MUST fails. `--json` prints
the same report as JSON. Checks marked (H) run in the desktop harness and report `SKIP` elsewhere.
