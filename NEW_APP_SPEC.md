# Building a new Arcade app

**Give this file to an implementing agent together with your idea.** Fill in
the [idea template](#12-the-idea-template-fill-this-in) at the end; everything
else here is fixed. The agent builds a complete Arcade app: it works on its
own, joins the other Arcade apps through Arcade Link, ships for Linux,
Windows and macOS, and is tested, documented and released the same way as the
existing apps.

This file is distilled from the five apps that exist (Box, Lens, Look, Wheel,
Clipboard), the manager (Tools) and the protocol ([SPEC.md](SPEC.md)). Where it
says *must*, the existing apps do it and a new app may not differ. Where a
choice is open, it says so and gives the default.

---

## 1. Read first

The implementing agent reads, in this order, before writing code:

1. This file and the filled-in idea.
2. [SPEC.md](SPEC.md): the protocol, locations, content types, errors,
   lifecycle, Connected apps page, tray menu, shortcuts, action catalog.
3. The README and `docs/ARCADE_LINK.md` (or `docs/arcade-link.md`) of the two
   existing apps closest to the idea. Lens (egui, overlay utility) and Look
   (Tauri, preview window) are the cleanest references; Wheel for Qt.
4. `tools/e2e.py` and one `tools/e2e_checks/<app>.py` in this repository.

## 2. What an Arcade app is

A small, fast desktop utility that **owns one verb** and does it well. The
family divides the verbs; a new app takes a verb nobody owns and uses the
owners for everything else:

| App | Owns | Use it for |
|---|---|---|
| Box | transforming content; pipelines | conversions, compression, text/data tools, multi-step jobs (`box:<tool>`, `box.pipeline.run`) |
| Lens | understanding the screen | region selection, OCR, codes, colors (`lens.capture`, `lens.recognize`) |
| Look | previewing files | showing any file (`look.preview`, `look.inspect`) |
| Wheel | invoking actions | putting an action on the radial launcher (`wheel.add_action`) |
| Clipboard | carrying content across devices | send to my devices, history (`clipboard.add`, `clipboard.pick`) |
| Tools | installing the apps | "Get" buttons (`tools.install`) |

If the idea overlaps an owned verb, the new app calls the owner when it is
present and keeps a minimal fallback of its own for when it is not.

## 3. Principles (non-negotiable)

1. **Standalone first.** The app is complete with no other Arcade app
   installed. Every integration is additive and invisible when the peer is
   missing, disabled or unavailable; never show a broken entry.
2. **Light.**
   - Nothing heavy is bundled. A capability needing a large engine finds the
     user's installation first and, if missing, offers a per-user download
     the user starts (see Link's `engines` feature and SPEC §2). Never
     install silently; never ship a browser, an office suite, an ML model or
     a language runtime inside the app.
   - Use what the OS already has before anything else: native OCR, the
     system voice, the system webview, the user's own browser, portals.
   - No Python, Node or JVM at runtime. Python is fine for tests and build
     scripts only.
   - An engine used by only one or two features must justify its size; prefer
     a small built-in implementation (Box's built-in images-to-PDF and
     document-to-PDF replaced img2pdf and LibreOffice).
3. **Zero idle cost.** No polling, no timers, no periodic wakeups while idle.
   Watch the registry and files through OS notifications. An idle app has at
   most one thread blocked in `accept`.
4. **Never block the UI thread** on disk, network, IPC, process launches or
   engine checks. Startup work that isn't needed for the first frame happens
   after it, on a worker.
5. **Local and private.** No account, no telemetry, no analytics. Network
   only where the feature inherently needs it, labelled in the UI before
   anything leaves the machine. Secrets found in content are never sent.
6. **Safe by default.** Never overwrite a user's file (write a new one). Never
   run shell strings (program + argument array). Persistence and outbound
   actions are confirmed in the owner's UI or marked ↗ with a payload preview.
   Requests arriving over the Link pass the same checks as the app's own UI.
7. **Honest.** No claim in UI or docs without evidence. A capability that
   doesn't work on a platform is hidden or explained, never a dead button.

## 4. Platforms and stack

Targets: **Linux** (X11 and Wayland; Hyprland is the owner's desktop and is
first-class), **Windows 10/11 x64**, **macOS** (Apple Silicon and Intel).
Install per user, without administrator rights.

The core is always **Rust** (or C++20 if the UI is Qt). Pick the UI by what
the app is; measured idle memory of the existing apps is the guide:

| UI | Use when | Example (idle RSS) |
|---|---|---|
| **egui/eframe** (Rust) | overlays, palettes, small settings windows, anything summoned by a shortcut | Lens (~80 MiB) |
| **Qt 6 Quick** (C++) | animated launcher-style overlays, Wayland layer-shell | Wheel (~115 MiB) |
| **Tauri 2 + vanilla TypeScript**, lazy-loaded chunks, no framework runtime | document-like windows that render rich content | Look (~75 MiB) |
| Tauri 2 + a UI framework | only for a large dashboard; it costs memory | Box (~430 MiB) |
| Flutter | only if phones are a target | Clipboard (~265 MiB) |

Default: **egui** unless the idea clearly needs rich document rendering
(Tauri + vanilla TS) or a layer-shell overlay (Qt). Never Electron. On Linux
with glibc, cap malloc arenas at two (`mallopt(M_ARENA_MAX, 2)`) at the top
of `main` if the app runs work on thread pools.

## 5. The standard app surface

Every item below is required. File names in parentheses are where the
existing apps implement them.

### 5.1 Process and command line

- One resident instance. A second launch hands its command to the running
  instance over a private channel (loopback socket plus a random token in a
  0600 file, or a local socket) and exits.
- Flags (SPEC §8.1): `--version`, `--background` (resident, no window; what
  login uses), `--settings`, `--quit`, `--restart`, `--arcade-manifest`
  (print the manifest, no side effects), `--arcade-invoke` (one-shot Link
  request on stdin) if the app has headless actions. No argument opens the
  main window or Settings.
- `app.status` reports `status.mode`: `background` or `foreground`, as
  started.
- A test override for every data location (`ARCADE_<APP>_HOME`, like
  `ARCADE_LENS_HOME`), honoured alongside `ARCADE_HOME`. Instances started
  with it, or from a cargo `target` directory, never touch login items or
  the applications menu.

### 5.2 Tray

SPEC §8.3, exactly: a click opens Settings; the menu is **Open <App>**,
**Open Settings**, **Restart Arcade <App>**, separator, **Quit Arcade <App>**.
StatusNotifierItem on Linux (show the icon when a tray host appears later;
at login the app often starts before the panel), notification area on
Windows, menu bar on macOS.

### 5.3 Login and installation

- Start at login is a Settings switch, on by default after the first run of
  an installed build: XDG autostart on Linux, the Run key on Windows, a
  LaunchAgent or `SMAppService` on macOS. Reject temporary executable
  locations (an AppImage mount, a download folder).
- On Linux, a desktop entry in the applications menu, kept pointing at the
  current executable.

### 5.4 Settings

One window with these pages (add the app's own where they belong):

- **General**: the app's options, start at login, theme (system/light/dark).
- **Shortcut**: a recorder with a reset to default, and the clash warning
  "Used by <app>" from the registry's cached `shortcuts` (no IPC).
- **Connected apps** (SPEC §8.2), exactly: master switch "Connect with other
  Arcade apps", one row per Arcade app with glyph, name, state and "Use with
  Arcade <App>", Get for missing apps (through `tools.install`, else the
  releases page), and a diagnostics expander.
- **About**: version, config and data paths, open-folder buttons, licenses.

Settings save atomically (write a temp file, rename), carry a schema
version, migrate older files, and back up an unreadable file before
falling back to defaults.

### 5.5 Global shortcut

Choose a default not in SPEC §8.5's table and record it there. Per platform:
X11 key grab, Windows `RegisterHotKey`, macOS Carbon hot key, Wayland through
the GlobalShortcuts portal, and on Hyprland a runtime binding added with
`hyprctl` (`hl.bind` in Lua configs, `keyword bind` in legacy ones) that is
never written to the user's config and is re-added after a config reload.
Where no global shortcut is possible, document the command to bind.

### 5.6 Look and feel

Calm and native-feeling; keyboard-first (every action has a key, `?` lists
them); light and dark themes; no flash of the wrong theme. Integration
surfaces (badges, Connected apps) use `assets/tokens.json` and the app's
glyph in `assets/glyphs/`. Peer entries are named as verbs ("Quick Look",
"Send to my devices ↗") with the owner's monochrome glyph, never "Powered by".

## 6. Arcade Link

- Canonical ID `arcade.<name>`; actions are `<name>.<verb>`.
- Depend on the protocol by tag. Rust:
  `arcade-link = { git = "https://github.com/qa-p1/Arcade-Link", tag = "<latest tag>", features = ["watch"] }`
  (add `"engines"` if the app needs helper programs). Qt: vendor `qt/` with a
  `VENDORED.json` pin and a vendor check in CI. Any other stack implements
  SPEC.md and passes `spec/vectors/`.
- Publish the manifest and start the listener **after the first frame**, on a
  background thread, through `Presence`. With the master switch off: no
  listener, no actions in the manifest.
- **Expose** the app's verb as actions with accurate `accepts`, `produces`,
  `effects`, `interactive`, `maxBytes`, platform list and `available`. Headless
  actions also work one-shot so callers needn't start the app.
- **Consume** peers from a registry cache refreshed by the directory watcher
  and `app.changed`; opening a menu never does disk access or IPC. Offer the
  owners' verbs (section 2) where the app's content fits them: Quick Look for
  files, Box presets and pipelines, Send to my devices, Add to Wheel, Lens
  selection or OCR.
- Content moves by reference (paths, handoff files), never inline bulk data.
  Results from peers are new files the app owns.
- Jobs report progress, can be cancelled (a caller disconnecting cancels),
  have a deadline, and clean up partial outputs.

Adding a sixth app also changes the shared repositories; the agent makes these
changes and releases a new Link tag:

| Repository | Change |
|---|---|
| Arcade-link | `ids` constant and `APPS`, `app_name`, `app_pitch`, `releases_url` (Rust) and the Qt `Ids`; `assets/glyphs/arcade.<name>.svg`; an accent in `assets/tokens.json`; `fixtures/<name>.json`; `tools/e2e.py` `APPS` and `tools/e2e_checks/<name>.py`; `benchmarks/bench.py` `APPS`; the SPEC §8.5 shortcut row and §11 action rows; a new tag |
| Arcade-tools | install locations and data folders (`src/paths.rs`); the app list itself comes from Link's `ids::APPS` |
| The other apps | bump the Link tag; Rust apps list peers from `ids::APPS`, while code that names the apps by hand (Wheel's Qt module, any frontend list: search for `arcade.clipboard`) gains the new app. Add any connected action that uses the new app's verb |

## 7. Engines and heavy capabilities

For each capability the idea needs, choose in this order and write the
choice in `docs/STATUS.md`:

1. Built into the OS (native OCR, system voice, webview, portal, file
   manager integration).
2. A small built-in implementation in the app's own language.
3. The user's installed program, found on `PATH` and the platform's usual
   install folders (and `<data>/arcade/engines/bin`), identity- and
   version-checked before use, run with an argument array, a timeout,
   cancellation and an output limit.
4. A per-user download the user starts, pinned and SHA-256-checked, into the
   shared engines folder (extend Link's `engines` module).

Engine checks run off the UI thread and are cached; at launch only engines
last seen missing are checked again, and the Settings page that lists them
re-checks all. A feature whose engine is missing says so and how to get it.

## 8. Testing

Nothing is tested on the owner's real desktop. Everything runs in Arcade
Link's disposable session (private D-Bus, Xvfb, temporary HOME, XDG
directories and `ARCADE_HOME`).

- **Unit tests** for all logic, including the platform-independent parts of
  platform code (path candidates, shortcut parsing, config migration).
- **Link tests** against `arcade-link mock` peers: absent and late peers,
  toggles, size limits, progress, cancellation, timeout, a peer crash,
  Private-mode and secret refusals.
- **An e2e group** (`tools/e2e_checks/<name>.py`) that drives the real binary:
  standalone behavior, every exposed action resident and one-shot, the
  Connected apps page, shortcut clash warning, the tray-less start, and each
  cross-app flow with the real peers. Screenshots go to `ARCADE_E2E_SHOTS`.
  The full run (`python3 tools/e2e.py`) must stay green for every app.
- **Benchmark** entry with a readiness signal: startup, warm invoke, idle RSS
  after 3 s, idle CPU over 5 s. Budgets: idle CPU 0, no idle wakeups,
  startup and memory in line with the comparable app in section 4.
- **Stress** (`tools/stress.py`): concurrent invocations, killing the app
  mid-burst, ten rounds of load with RSS that plateaus.
- **CI** on Linux, Windows and macOS: format check, lint with warnings as
  errors, tests, package build (and a package smoke test). Linux runs the
  isolated-session tests.

## 9. Packaging and release

- GitHub repository `qa-p1/Arcade-<name>`, MIT license, default branch
  `main`.
- Packages: Linux AppImage (or a tarball with an install script), Windows
  per-user installer (NSIS or Inno, silent flags documented), macOS dmg.
  Builds are unsigned until signing exists; say so in the README.
- Every release carries `arcade-release.json` and `SHA256SUMS.txt` made with
  the vendored `tools/arcade-release.py` (schema
  `spec/arcade-release.schema.json`), so Arcade Tools can install it.
- Pushes to `main` that pass CI publish: a new `v<version>` when the version
  hasn't been released yet, otherwise the rolling `nightly` prerelease.
  Feature work happens on branches; merging to `main` releases.

## 10. Documentation

Plain, short sentences; present tense; no marketing. Every status claim is
dated and backed by a check. Use the same words as the other apps: *tested*
(ran here), *CI-built and tested* (CI ran it), *not run interactively*,
*build only* (compiled, never executed).

| File | Contents |
|---|---|
| `README.md` | What it does (one line, then a paragraph); install per platform; using it (keys table); works with other Arcade apps; platform table; building; known limits |
| `docs/ARCHITECTURE.md` | Crates or modules, process model, data flow, threading, platform layer |
| `docs/ARCADE_LINK.md` | Exposed actions table, consumed actions, settings keys, command line, verification commands, platform table |
| `docs/STATUS.md` | Implemented, verification table (checks and results, with the CI commit), limits, document index; dated |
| `CHANGELOG.md` | Changes by version |
| `VENDORED` | Anything copied from another repository, with its pin |

## 11. Definition of done

- [ ] Every item in section 5 works on Linux X11 and Hyprland, and is built
      and tested in CI on Windows and macOS.
- [ ] The app's verb works with no other Arcade app installed.
- [ ] Exposed actions pass resident and one-shot e2e checks; every consumed
      peer action is hidden when the peer is missing, disabled or
      unavailable.
- [ ] No bundled heavy engine; every engine follows section 7.
- [ ] Idle CPU 0 and no idle wakeups in the benchmark; memory plateaus under
      the stress run.
- [ ] Full `tools/e2e.py` run green for all apps; CI green on all three OSes.
- [ ] Shared-repository changes from section 6 made, a new Link tag
      published, the other apps bumped to it.
- [ ] README, ARCHITECTURE, ARCADE_LINK, STATUS and CHANGELOG written and
      accurate; limits stated plainly.
- [ ] A final report: what was built, how it was verified (commands and
      results), what is not done and why.

### How the agent works

Work in phases and commit each: (1) the standalone core and its tests; (2) the
UI; (3) platform integration (tray, login, shortcut, single instance);
(4) Arcade Link, exposed then consumed; (5) the e2e group, benchmark and
stress; (6) packaging and CI; (7) documentation and STATUS; (8) the report.
Verify every claim by running it. If something can't be done on this
machine (an interactive Windows run, a signing key), say so in STATUS
instead of claiming it. Never kill processes by name pattern; stop only the
PIDs you started.

---

## 12. The idea template (fill this in)

```markdown
# Idea: Arcade <Name>

**One line:** <the verb this app owns, e.g. "Record any window as a GIF">

**Problem:** <what is slow or annoying today, for whom>

**How it's summoned:** <global shortcut / tray / file-manager action / another app / CLI>

**Main flow:** <3–6 steps from summon to result>

**Inputs and outputs:** <content types it takes and produces, e.g. file/image → file/video>

**Exposes to other apps:** <actions, e.g. <name>.record (interactive), <name>.inspect (headless)>

**Uses from other apps:** <e.g. Lens selection to pick a window, Look to preview the result, Clipboard to send it>

**Heavy capabilities:** <anything that needs an engine; say if an OS feature or the user's install will do>

**Data it keeps:** <settings, history, caches; how long>

**Network:** <none / what and when, and how it's labelled>

**Platforms that matter most:** <e.g. Hyprland first, then Windows>

**UI style:** <overlay / small window / document window; any reference>

**Out of scope:** <what it should not do>
```
