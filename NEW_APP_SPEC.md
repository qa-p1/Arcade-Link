# Arcade new-app implementation brief

Attach this file to your app idea. The implementing agent should carry the
idea through working software, tests, packaging, documentation and a completion
report. A short idea is enough: infer routine choices, state assumptions and
ask only for missing information that would materially change the product.
Use the optional template at the end if you want more control.

This is a reusable brief, not a claim that every existing app meets every
quality target below. The user's current instructions take precedence. A
complete implementation and a published release are separate milestones.

## 1. Start with the idea and the current ecosystem

Before coding, inspect the destination repository, its instructions and existing
changes. For a new repository, choose a stable name, scope and primary user
flow. Write a short implementation plan, then carry it through; do not stop at
scaffolding, a mockup, a proposal or a list of things the user could do later.

Read these sources (links work when this file is copied on its own):

- [Arcade Link specification](https://github.com/qa-p1/Arcade-Link/blob/main/SPEC.md):
  protocol, manifests, content, lifecycle, settings and action contracts.
- [Link source and integration assets](https://github.com/qa-p1/Arcade-Link):
  Rust crate, Qt module, glyphs, tokens and conformance vectors.
- [Verification and limitations](https://github.com/qa-p1/Arcade-Link/blob/main/COMPLETION_REPORT.md):
  measured behavior and known gaps; historical plans are not implementation proof.
- The current README, status and Link integration docs of the closest existing
  apps. Inspect the actual manifests and code before copying an example.

| Repository | Responsibility | Relevant actions |
|---|---|---|
| [Box](https://github.com/qa-p1/Arcade-box/tree/arcade/link) | Content transformations and saved workflows | `box:<tool-id>`, `box.pipelines`, `box.pipeline.run` |
| [Lens](https://github.com/qa-p1/Arcade-lens/tree/arcade/link) | Screen selection, recognition, pins | `lens.capture`, `lens.recognize`, `lens.pin` |
| [Look](https://github.com/qa-p1/Arcade-look/tree/arcade/link) | File previews and inspection | `look.preview`, `look.inspect` |
| [Wheel](https://github.com/qa-p1/Arcade-wheel/tree/arcade/link) | Invoking actions from a radial launcher | `wheel.add_action`, `wheel.show` |
| [Clipboard](https://github.com/qa-p1/Arcade-clipboard/tree/arcade/link) | History and sending content to paired devices | `clipboard.add`, `clipboard.pick` |
| [Tools](https://github.com/qa-p1/Arcade-tools) | Per-user app installation and updates | `tools.install` |

These are reference branches as of 2026-10-08. Check their current state and
release tags when starting a new app. Lens has window recording internally but
no recorder action over Link. Clipboard sends to the mesh, not one named device.
New integrations must use capabilities that actually exist.

## 2. Product requirements

- Give the app a clear primary job. Implement the whole path from input to a
  useful result, including empty states, loading, errors, cancellation and recovery.
- Work independently when no other Arcade app is installed. Peer integrations
  add convenience; a missing peer must not break the core flow. If a fallback
  needs an optional engine, explain that dependency and how to obtain it.
- Prefer the existing owner of an adjacent capability when present. Avoid
  duplicating an entire editor, launcher or conversion suite for one small feature.
- Keep the interface focused, keyboard accessible and usable at different DPI
  scales. Provide clear focus, contrast, light/dark/system themes, useful error
  messages and a working primary action. Do not ship placeholder buttons.
- Local processing is the default. No account, analytics or telemetry unless
  the idea explicitly requires them. Explain network processing before sending
  user content; preserve the existing apps' secret and private-mode guards.
- Do not add cloud services, mobile clients, a plugin system or a workflow
  language merely because another Arcade app has one.

## 3. Stack and dependency budget

Use a Rust core by default, or C++20 for a Qt app. Prefer a native or low-level
UI suitable for the idea: egui/eframe for a compact utility, Qt Quick for an
animated overlay or layer-shell surface. Use Tauri with the system webview when
rich document rendering justifies it. Flutter is appropriate when mobile is an
actual requirement. Do not default every app to Box's stack or use Electron.
Record the choice and its tradeoffs briefly.

For perspective, the 2026-10-08 Linux benchmark measured roughly 81 MiB for
Lens, 75 MiB for Look, 115 MiB for Wheel, 427 MiB for Box and 265 MiB for
Clipboard at its three-second sample. These are whole-app measurements from
one machine, not framework overheads or guaranteed budgets for a new app.

Choose capabilities in this order:

1. Suitable OS functionality: native OCR, system speech, portals, file dialogs.
2. A small implementation or library in the app's own language.
3. A user-installed engine, detected off the UI thread and checked for identity,
   version and required capabilities.
4. An optional, pinned, checksum-verified per-user download started by the user.

Never bundle a browser, office suite, Python/Node/JVM runtime, OCR engine or large
model for a minor feature. Prefer native code for app runtime logic. Existing
optional engines that happen to use Python do not justify making Python an app
requirement. Python and Node remain valid development/build tools.

For OCR, use the platform engine where suitable or Tesseract: detect the global
installation first; when absent, offer an explicit download using Link's shared
`engines` support where available. Do not bundle it or revive the removed ocrs
models. Use native OS voices for speech (SAPI, `say`, installed eSpeak/eSpeak NG
on Linux), not a bundled Piper stack. Do not reintroduce img2pdf, OCRmyPDF or
LibreOffice as default dependencies for tasks a small implementation handles.
Box's built-in document-to-PDF conversion has layout limits; document comparable
tradeoffs rather than promising exact fidelity.

Run helpers as executable plus argument array, with timeouts, bounded output,
cancellation and private working directories. Cache availability and refresh on
relevant changes or explicit requests. A missing engine should produce an
honest unavailable state and installation guidance, never a silent download.

## 4. Process, platform and settings behavior

Target Linux, Windows and macOS unless the idea narrows the scope. Cover X11
and Wayland explicitly, including Hyprland where relevant. Specify supported
architectures and minimum OS versions based on dependencies and actual builds.

- Keep one resident instance per profile. A second launch routes its command
  to the existing instance and exits. A feature should stay resident only when
  its shortcut, tray, service or Link behavior requires it.
- Implement `--version`, `--background`, `--settings`, `--quit` and
  `--arcade-manifest`. Add `--arcade-invoke` for headless actions. The manifest
  command prints JSON without creating windows, registrations or login items.
  One-shot calls must terminate and clean up. Add a restart command if useful;
  restarting from the tray must safely hand over to a successor.
- Resident apps follow the shared tray menu: Open <App>, Open Settings,
  Restart Arcade <App>, separator, Quit Arcade <App>. Clicking opens Settings;
  macOS may open the menu. Handle a tray host appearing after startup.
- Expose a Start at login setting. Use stable installed executable paths and
  per-user OS mechanisms. Follow the user's preference; any first-run default
  must be visible and reversible. Development/test builds must not register
  themselves in the real desktop, applications menu or login items.
- Provide General, Shortcut where relevant, Connected apps, and About settings.
  About includes version, data/config paths and licenses. Persist settings
  atomically with schema versions, migration and recovery from malformed files.
- Choose a shortcut without an existing default collision. Warn about clashes
  using cached registry shortcut metadata. Use native hooks or portals; on
  Hyprland prefer runtime bindings and handle reloads. Document a manual command
  binding when an automatic shortcut is unavailable.
- Honor `ARCADE_HOME` for Link and a documented per-app profile override.
  Isolate all app data, locks and registrations in tests; one override alone
  may not isolate the OS keyring or clipboard.

### Standard CLI and integration commands

Implement the shared CLI contract in addition to the base flags above. These
management flags are part of the target standard; list a flag as supported only
after its behavior is implemented and checked:

| Flag | Contract |
|---|---|
| `--restart` | Ask the resident instance to restart. Start a successor and hand over the instance lock safely; the successor waits for the old process/lock to exit before acquiring it. |
| `--install [--silent] [--no-launch] [--channel C]` | Linux AppImage and macOS bundle self-install. `--silent` suppresses the app dialog for an explicitly requested install or a confirmed Tools handoff; it does not silently accept an unsolicited install. |
| `--uninstall [--silent] [--remove-data]` | Uninstall using the receipt only. Preserve app data unless `--remove-data` was explicitly selected. |
| `--repair` | Recreate integration entries from the receipt and current installed executable. |
| `--integration-status` | Print JSON describing the receipt and integration state; make no changes. |

Windows uses its per-user installer interface and native silent switches
(`/VERYSILENT` for Inno or `/S` for NSIS), with the same receipt and integration
effects as the interactive path. Silent install and uninstall paths must be
tested for parity. Do not invent a second Windows CLI installer if the package
already owns that flow.

### Tray hosting and hand-back

Tools may host the standard tray menu; it does not broker app actions. Each app
keeps working standalone and keeps its own tray icon available. The app-side
`TrayHostWatcher` observes Link registry notifications (never a timer or idle
poll), connects to `arcade.tools` when its manifest advertises
`settings.trayHost.enabled`, then subscribes to `tray.host`:

- On `hosted: true`, unregister the app's OS tray icon while retaining its
  icon/menu definition. On `hosted: false` or a closed connection, register the
  app's own icon immediately.
- If Tools reports `restarting: true`, hold the app icon for up to five seconds
  while Tools restarts, then show it if Tools has not returned.
- At startup, wait at most 300 ms for the first host answer before showing the
  app icon. This avoids a flash when Tools is already hosting without delaying
  standalone startup indefinitely.
- If Link is disabled in the app, keep its own icon visible. Expose
  `status.tray` as `own`, `hosted` or `none` for diagnostics.
- Hand the icon back within one second after Tools exits or crashes. While
  hosted, a blocked socket read is allowed; idle polling and repeated timers
  are not.

Some native tray libraries have no hidden-icon constructor. Prepare the icon
and menu data while hosted, but defer OS tray registration until the app owns
the icon; when switching from own to hosted, remove the registered icon and
retain the data needed to recreate it.

On Linux, unregistering means the StatusNotifierWatcher loses the item: drop
the ksni handle, release the item's bus name (Qt's `setVisible(false)` does),
or close the private D-Bus connection that carries it. Setting the item's
Status to Passive is not enough, and neither is removing a Tauri or
libappindicator tray icon, because Caelestia and Waybar still draw Passive
items and watchers forget an item only when its bus name goes away. Copy the
canonical `tests/tray-registration.py` (a real watcher that lists Passive
items) and run it in Linux CI through Link's `tools/e2e.py run --`.

Measure and test host start-after-app, app start-after-host, Tools restart,
Tools crash, Link disabled and per-app host opt-out. Tools quitting must not
change the app's settings, jobs or other behavior.

### Standard Link methods and manifest fields

Implement the methods supported by the pinned Link version and list optional
ones in `describe.methods`. The current target app standard includes:

| Method | Required behavior |
|---|---|
| `app.settings` | `{}` → `{opened:true}`; same as `--settings`. |
| `app.restart` | `{mode?}` → `{restarting:true}`; start a successor and hand over the instance lock safely, with the successor waiting for the old process/lock to exit before acquiring it. Return `busy` for active jobs unless the supported request explicitly forces it. |
| `app.menu` / `app.menu.invoke` | `{}` → `{items:[MenuItem]}` and `{id}` → `{done:true}`. Return/invoke app-specific extras; standard tray entries are implied. Extras use `action`, `toggle` or `separator` items with `checked`, `enabled` and optional `effects`; changes emit `app.changed`. |
| `app.shortcuts.set` | `{id, accelerator}` → `{applied, via, hint?}`. Validate, apply live, persist and rewrite the manifest. `via` is `native`, `portal`, `hyprland-runtime` or `manual`; `manual` includes a binding hint. Reject invalid/reserved accelerators as `denied`; report unsupported registration as `unavailable`. |
| `app.settings.export` / `app.settings.import` | Export `{}` to `{outputs:[{type,path}]}` and import `{inputs:[file]}` to `{imported,restartRequired}`. Export portable settings only; validate import, back up current settings and apply atomically. Exclude secrets, device identity and data the app does not promise to migrate. |

The shared manifest remains schema 1 and may include cached app-specific `menu`
items, a `logs` directory, the carried `docs` version, and the read-only
`install` method (`appimage`, `windows-installer`, `macos-bundle`, `tarball`,
`manual` or `dev`). Tools alone advertises
`settings.trayHost: {enabled, excluded:[ids]}`. Do not add speculative fields or
claim a method exists unless the app and its pinned Link implementation support
it.

Settings export/import requires an explicit per-app allowlist. For example,
Box exports pipelines, favorites and presets but not history; Clipboard never
exports device keys or history; Find excludes its index. Document the exact
portable fields, validate schema and size, never export credentials or device
identity, and leave settings unchanged on a failed import.

### Shared shortcut data and user documentation

Every app ships `docs/user/` with task-focused install/first-run, primary-flow,
settings, shortcut, privacy/data, troubleshooting and platform-limit guidance.
Keep versioned behavior and screenshots accurate; take UI screenshots in the
isolated test session. Each app also ships:

- `docs/user/shortcuts.json`, schema 1, identifying the app and documentation
  version and grouping shortcuts by context. Each shortcut has a stable `id`,
  `title`, OS-specific `keys`, and optional `rebindable`.
- Generated `docs/user/shortcuts.md`, readable without a shortcut browser.
  Regenerate it from JSON and fail CI when it is stale.

The common shape is `{schema:1, app, version, groups:[{title, context,
shortcuts:[{id, keys, title, rebindable?}]}]}`. For example:

```json
{
  "schema": 1,
  "app": "arcade.example",
  "version": "1.0.0",
  "groups": [{
    "title": "Global",
    "context": "global",
    "shortcuts": [{
      "id": "toggle",
      "keys": {"default": "Ctrl+Alt+X"},
      "title": "Show or hide Example",
      "rebindable": true
    }]
  }]
}
```

Use the shared accelerator schema and vectors. Canonical modifiers are ordered
`Ctrl`, `Alt`, `Shift`, `Super`; key sequences are chords separated by spaces
(for example `Ctrl+K Ctrl+S`). Use canonical key names, not localized display
names (`Super` represents Cmd on macOS and Alt represents Option there). A
`keys` value may be one string, an array of alternatives or `null` for an
unsupported OS; use the OS-specific value when present, otherwise `default`.
Two bindings conflict when equal or when one is a strict prefix of the other.
`rebindable: true` is allowed only for a global shortcut whose `id` matches a
manifest `shortcuts[].id`; never mark third-party shortcut sheets rebindable.
In-app shortcuts stay in the versioned docs snapshot, not in the app manifest.

Validate `shortcuts.json` against the shared JSON Schema and vectors in the
app's CI and Link conformance suite. Validate documentation links and the
generated Markdown in CI as well. The manifest contains only effective global
shortcuts; Tools reads in-app shortcut data from the versioned docs snapshot.

Do not block UI events on disk access, IPC, networking, helper probes or process
launches. Use workers and dispatch UI work back to the UI thread. Debounce active
work when useful; avoid periodic idle polling. Measure actual idle behavior.

## 5. Arcade Link integration

Use canonical ID `arcade.<lowercase-name>` and stable platform bundle IDs. New
actions normally use `<name>.<verb>`; keep the frozen v1 wire contract compatible.
Depend on a published immutable Link tag or exact commit and lock it. Use the
latest released tag; while a required Link change is under review, pin its exact
reviewed candidate commit and do not ship a branch dependency. Never assume an
unreleased tag exists:

```toml
arcade-link = { git = "https://github.com/qa-p1/Arcade-Link", tag = "<released-tag>", features = ["watch"] }
```

Add `engines` only if needed. Use only features present in the pinned Link
version; do not assume a `tokio` feature exists.
Qt apps vendor the matching module with provenance and a drift check. Other
stacks must implement the specification and pass its conformance vectors.
Never leave a sibling-directory dependency as the distributable configuration.

Publish the manifest and listener off the first-frame path using the library's
presence mechanism. Keep it alive for the app lifetime. With connections off,
retain a disabled installed manifest, expose no actions and stop listening.

Each exposed action needs truthful accepts/produces, effects, privacy, platforms,
availability/reason, version and size limits. Interactive actions run in the
owner's UI, not one-shot. Headless actions should support both resident and
one-shot execution when the app advertises that capability. Apply the same
permissions and confirmations to Link requests as to local UI requests.

Consume peers from a cached registry refreshed by OS notifications. Never scan
disk or query peers when opening a menu. Respect the master switch, per-peer
switches, accepted content types, availability and platform support. Use a
verb and the owner's glyph for actions; put missing-app Get buttons in Connected
apps. They call Tools when available and otherwise open the releases page.

Use file references/handoff files for bulk content. The current wire limit is
1 MiB per JSON line and the inline text limit is 256 KiB. Respect ownership:
consume or copy input handoffs before their creator cleans them up, and arrange
output lifetimes so callers can consume them. Do not assume a peer result is
permanent or becomes your app's property without copying it.

Implement progress, cancellation, deadlines and clear peer-crash errors. Bound
concurrency and memory. The local token authenticates the same OS user, not a
genuine Arcade application; do not grant elevated trust based on an app ID.

## 6. Registering a new app with the family

A valid manifest alone does not add a new app to every existing UI or installer.
Shared metadata and several consumers use explicit lists. Inspect and update
these where the user's repository scope permits:

| Repository | Required audit and integration work |
|---|---|
| Link | Rust and Qt IDs, `ids::APPS`, display names/pitches/release URLs, glyph and accent tokens, fixtures, action/shortcut catalog, conformance tests, e2e and benchmark app lists |
| Tools | The pinned Link app list/allowlist, per-platform install/executable/data/autostart mappings and release-source behavior; test installation, update and removal of the new app |
| Arcade catalog | Add the app, supported assets/channels, optional engines, release URL, docs and shortcut metadata in a reviewable catalog PR. Run the catalog validator and CI before treating the listing as onboarded. |
| Existing consumers | Explicit peer lists in Rust/C++/frontends, policy defaults, meaningful new actions and tests; bump pinned shared code when needed |
| New app | Exposed and consumed actions, Connected apps, isolated e2e group, packaging metadata, `docs/user/`, generated shortcut sheet and documentation |

Do not silently change protocol v1 or move an existing tag. Prepare shared
changes on branches and validate consumers against the exact candidate commit.
Add a Tools `tools-hub` e2e case for the app's catalog entry, install, update
and removal paths, including supported package mappings and receipt cleanup.
Publish a new tag, bump consumers, merge or release only when the user has
already authorized those operations. If shared repositories or credentials
are unavailable, complete the standalone app and reviewable integration changes,
then report the precise remaining dependency. Do not claim ecosystem onboarding
is finished while those changes remain unshipped. In particular, the catalog
PR, Tools mapping/e2e and required Link change must ship before onboarding is
reported complete.

## 7. Data and system safety

Keep content local unless the selected feature sends it. Store only the history
needed by the idea; document retention and deletion. Use scoped file access,
private temporary directories and atomic writes. Avoid overwriting source files;
explicit replacement needs a deliberate user choice. Use OS credential stores
where available and disclose any plaintext fallback. Never log secrets or content.

Keep temporary environments process-scoped. Never write `/tmp`, `/run/user`,
temporary toolchain homes, build sandboxes or generated environment paths into
shell profiles, compositor startup files, persistent environment files or user
services. Before an authorized change to a login-critical/persistent environment
file: read it, back it up, verify every path survives reboot, make a minimal
change, inspect the diff and validate that it loads. Prefer avoiding these edits.

Do not alter unrelated apps, remove user data or install system packages merely
to get a test green. Preserve existing user changes. Stop only processes the
current test started; never kill applications by a broad name pattern.

## 8. Verification and performance

Use meaningful tests for the behavior and risks introduced, including malformed
input, migration, limits and failure recovery. Do not substitute tests that only
repeat implementation details for exercising the real user flow.

- Run relevant formatting, linting, unit and integration checks.
- Exercise standalone operation and each exposed action, including one-shot
  where supported. Test missing/late peers, toggles, unsupported input,
  progress, cancellation, timeout, peer crashes and outbound confirmation.
- Extend [the isolated ecosystem runner](https://github.com/qa-p1/Arcade-Link/blob/main/tools/e2e.py)
  when working on the family. It uses private D-Bus, Xvfb and temporary profile
  roots. Keep automation away from the owner's live desktop. A deliberate
  interactive platform test is separate and explicitly scoped.
- Run the full affected cross-app suite after integration changes. Record
  commands, results, commit IDs and which flows used real peers versus mocks.
- Add Linux, Windows and macOS CI builds and tests for supported targets, plus
  package creation and relevant package smoke checks. CI compilation, CI tests,
  installer creation and interactive validation are distinct evidence.
- Run the isolated Link e2e groups that apply: `install` (receipt, launcher,
  icons, CLI shim, update, repair, uninstall and development-build prompt
  suppression), `trayhost` (start order, restart, crash hand-back, opt-out and
  disabled Link), `shortcuts` (schema, manifest, conflicts and live rebind),
  and `tools-hub` (catalog install, update and removal). Keep HOME, XDG,
  `ARCADE_HOME` and per-app data isolated; never install a test build into the
  owner's profile.
- Measure startup/readiness, warm invocation, idle RSS, idle CPU and repeated
  load. Use a comparable baseline, state process/child accounting and startup
  timing boundaries, and inspect outliers or negative counters. A zero CPU
  median does not prove zero wakeups. Whole-app RSS does not isolate Link cost.
- Aim for no idle polling, responsive startup and memory that plateaus under
  stress. Treat SPEC's Link overhead budgets as targets requiring dedicated
  measurement, not guarantees inherited from using the crate. Optimize measured
  bottlenecks; do not copy allocator tuning from Lens without evidence.
- Verify Linux AppImage install behavior on a supported real Linux desktop;
  verify Windows and macOS install and interactive behavior on their supported
  systems. Label Windows/macOS as build-only when only CI builds or package
  creation have run. Report the OS, desktop/session, architecture, package and
  tested flow for every real run. Never describe a build as a desktop test.

Do not mark unsupported or untested platform behavior as passed. When hardware,
credentials or a service prevent validation, finish independent work and record
the exact unverified scope and reproducible next step.

## 9. Packaging and delivery

Use a repository name consistent with `Arcade-<name>`. Respect an existing
license; for a new app select a compatible license and include its full text
and third-party notices. Do not infer that every Arcade app is MIT licensed.
Check redistribution requirements before bundling dependencies.

Provide reproducible per-user packages: Linux AppImage, Windows per-user
installer and macOS app/DMG. Keep runtime assets complete and optional engines
separate. Verify versions, icons, uninstall behavior and stable executable
paths.

### Install receipts and self-install

Write one atomic, owner-readable install receipt at
`<data>/arcade/installs/<arcade-id>.json` under the same Arcade data root as the
Link registry (and honor `ARCADE_HOME`). Use schema 1 and mode `0600` where the
platform supports POSIX modes. Record app ID, version, channel, install method,
update manager (`self` or `tools`), installed path, integration paths, previous
version/path when present, and install/update timestamps. The method is one of
`appimage`, `windows-installer`, `macos-bundle`, `tarball`, `manual` or `dev`.
Dev builds never write receipts.

The schema-1 record uses `schema`, `id`, `version`, `channel`, `method`,
`managedBy`, `path`, `integration`, optional `previous`, `installedAt` and
`updatedAt`. `integration` records paths the installer actually created, such
as `desktopEntry`, `icons`, `cli`, `autostart` and `uninstaller`; absent entries
are `null` or omitted. Restrict access to the current user (0600 on POSIX,
equivalent per-user ACLs on Windows). For example:

```json
{
  "schema": 1,
  "id": "arcade.example",
  "version": "1.0.0",
  "channel": "stable",
  "method": "appimage",
  "managedBy": "self",
  "path": "/home/user/Applications/Arcade/Arcade-Example.AppImage",
  "integration": {
    "desktopEntry": "/home/user/.local/share/applications/arcade-example.desktop",
    "icons": ["/home/user/.local/share/icons/hicolor/256x256/apps/arcade-example.png"],
    "cli": "/home/user/.local/bin/arcade-example",
    "autostart": null,
    "uninstaller": null
  },
  "previous": null,
  "installedAt": "2026-10-11T10:00:00Z",
  "updatedAt": "2026-10-11T10:00:00Z"
}
```

The app's installer writes Linux self-install receipts; the Windows installer
writes them during install; macOS first-run move writes them; Tools may write or
update a receipt when it manages the installation. Do not write a receipt for
a development build detected under a source `.git`, `target/` or `build/`
directory, or with `ARCADE_DEV_BUILD=1`. A dev build must never prompt for or
register a real desktop install.

- **Linux AppImage:** show the small, cancellable install dialog only when
  Link's `Runtime.is_appimage` contract succeeds: `APPIMAGE` names the real
  image file and the current executable is inside that image's `APPDIR` mount.
  Inherited `APPIMAGE`/`APPDIR` variables alone are not evidence of an AppImage
  run. Suppress the dialog for
  dev builds, `--background`, `--arcade-invoke`, `--arcade-manifest`,
  `--version` and `ARCADE_NO_INSTALL_PROMPT=1`. Offer Install, run once and do
  not ask again for this file. Detect NotInstalled, InstalledHere and
  InstalledElsewhere (including its version); updates managed by Tools go
  through the Tools handoff. `--install --silent` performs the same integration
  without the dialog after an explicit CLI request or confirmed Tools action.
  Install to
  `~/Applications/Arcade/<Name>.AppImage`; add the existing stable desktop ID,
  hicolor icons, a `~/.local/bin` CLI symlink and the configured autostart
  entry. Preserve the app's existing start-at-login default and make it visible
  in the dialog. If `~/.local/bin` is not on PATH, show the exact shell command
  as a hint; never edit a shell profile. Install must not delete the downloaded
  file unless the user selected that option.
- **Windows:** use the app's existing Inno or NSIS installer technology in
  per-user mode under `%LOCALAPPDATA%\Programs\<Name>`. Create a Start menu
  entry, offer the desktop shortcut and start-at-login choices, write/remove
  the receipt, and provide the shared per-user CLI shim in
  `%LOCALAPPDATA%\Arcade\bin`. Add that directory to the user's `Path` once;
  remove it only when the last Arcade app no longer needs it. The silent path
  must match the UI path and uninstall through the receipt. Register publisher
  `qa-p1` and the catalog page as installer metadata.
- **macOS:** provide a dmg with the normal drag-to-Applications layout. On
  first run from a translocated/read-only location, offer to copy to
  `~/Applications` (or `/Applications` when writable), relaunch, write the
  receipt and offer a CLI symlink. Never remove quarantine or bypass Gatekeeper.

On every OS, install/update leaves app data untouched. Uninstall removes only
paths recorded in the receipt and removes data only after an explicit
`--remove-data` choice. Repair rewrites launcher, CLI and autostart entries
from receipt paths; it must not guess or delete unrelated entries. Keep each
app's existing desktop-entry/bundle identity to preserve pins and avoid
duplicates. Start-at-login defaults are visible and reversible.

For Tools compatibility, generate `arcade-release.json` and `SHA256SUMS.txt`
with the versioned [release manifest tool](https://github.com/qa-p1/Arcade-Link/blob/main/tools/arcade-release.py)
and validate the schema and platform install mappings. SHA-256 checks integrity;
it is not publisher authentication. State signing/notarization status accurately.

Keep the release manifest schema compatible and include per-asset `installArgs`
where a package supports self-install (Linux/macOS `--install --silent`) and
`kind: "portable"` for downloads Tools must not install. HTTPS downloads must
come from allowed GitHub hosts and catalog entries must be restricted to the
`qa-p1` owner. Verify the declared asset size and SHA-256 before running or
installing it. A checksum detects corruption; it does not authenticate the
publisher. Never bypass macOS quarantine or Windows SmartScreen. Preserve each
app's existing permission, confirmation and file-safety rules.

Inspect workflow triggers before pushing. Several existing apps publish on
`main`; do not merge there as a routine verification step. Build artifacts and
prepare release notes first. Follow the user's existing authorization for
commits/pushes; publishing releases or tags requires authorization for that action.
Do not add automatic publishing merely because another repository uses it.

## 10. Documentation and completion report

Write concise docs that match the final implementation:

| File | Required content |
|---|---|
| `README.md` | Purpose, installation, primary flow, shortcuts, platform support, build commands and limits |
| `docs/ARCHITECTURE.md` | Modules, process/thread model, data flow, platform boundaries and dependency choices |
| `docs/ARCADE_LINK.md` | Actions, content/effects, consumed peers, settings, lifecycle and verification |
| `docs/STATUS.md` | Dated implementation/evidence table, tested commits, real limitations and document index |
| `docs/user/` | Task-focused install, first-run, primary-flow, settings, shortcuts, data/privacy, troubleshooting and platform-limit pages |
| `docs/user/shortcuts.json` and generated `shortcuts.md` | Versioned shortcut contract and readable sheet; schema, vectors, generated output and links validated in CI |
| `CHANGELOG.md` | User-visible changes |
| License and provenance files | License text, third-party notices and pins for vendored code |

Check local documentation links and command examples. Label historical plans
as historical. Keep the repository clean, review diffs and verify local/remote
commit equality after an authorized push.

The completion report states what was built, where the code and packages are,
which checks passed, what remains incomplete and why. Separate implementation
completion from release readiness and unverified platforms. Do not call the work
finished while an authorized, feasible part of the agreed scope remains undone.

### Tools hub delivery checklist

Track implementation and verified delivery separately. Before claiming a new
app is onboarded, check each item against the exact app and Link commits:

- [ ] A self-installing Linux AppImage, per-user Windows installer and macOS
  bundle, with isolated silent install/uninstall parity for supported targets.
- [ ] An atomic install receipt, stable launcher/icon/CLI entries, update,
  moved-file repair and receipt-owned uninstall; development builds skip prompts.
- [ ] `docs/user/` and versioned `shortcuts.json`, generated `shortcuts.md`,
  screenshots and CI validation of sheets and documentation links.
- [ ] The tray-host watcher, `status.tray`, both startup orders, restart grace,
  crash hand-back within one second, disabled Link and per-app opt-out.
- [ ] Advertised `describe.methods`: settings/restart, menu/invoke, live global
  shortcut rebinding, portable settings export/import with explicit exclusions.
- [ ] A catalog PR with the app's assets, docs, channels and supported platforms.
- [ ] Tools mappings and a real-app `tools-hub` e2e entry, plus the `install`,
  `trayhost` and `shortcuts` groups and existing cross-app checks.
- [ ] Recorded performance against Tools plan §18: critical-path watcher plus
  `detect()` ≤5 ms; hand-back ≤1 s (target ≤200 ms); closed-window Tools ≤40 MiB
  and zero CPU ticks over 30 s; overlay ≤80 ms p95; index rebuild ≤150 ms.
- [ ] Dated Linux desktop evidence and separate Windows/macOS CI/build evidence;
  unmeasured hardware-dependent behavior stays labeled as unverified.

## 11. Suggested implementation order

1. Confirm scope from the idea, inspect references and choose the stack.
2. Build the standalone core and real primary flow.
3. Finish UI, settings, persistence and platform lifecycle.
4. Add Link actions and methods, the tray-host watcher, peer consumption, the
   app's catalog PR and Tools app mapping/e2e entry.
5. Verify install/receipt, trayhost, shortcut and tools-hub e2e flows, failures,
   cross-app behavior, performance and platform builds; record platform
   evidence separately for CI and real desktop runs.
6. Finish self-install packages, licenses, CI, `docs/user/`, generated shortcut
   data, STATUS and other user documentation.
7. Review, commit/synchronize where authorized, and deliver the completion report.

Proceed through these phases autonomously. Ask early only when a consequential
choice cannot be inferred, and continue independent work while awaiting it.

## 12. Optional idea template

```markdown
# Idea: Arcade <Name>

One line: <what the app does>
Problem and audience: <what is annoying today, and for whom>
Main flow: <how the user opens it, acts and gets a result>
Inputs and outputs: <text, files, screen selection, audio, etc.>
Must have: <the essential capabilities>
Nice to have: <optional extras>
Out of scope: <what it must not become>
Platforms: <defaults are Linux/X11/Wayland, Windows and macOS>
UI preference: <overlay, small native window, document window, or infer>
Arcade connections: <which existing apps would help, or infer>
Data and network: <what it retains or sends, or local/minimal by default>
Constraints: <performance, engine, license or distribution requirements>
Repository scope: <new app path and any existing repos allowed to change>
Delivery authorization: <local work, commit/push, PR, release as applicable>
```
