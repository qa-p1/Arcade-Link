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

Do not block UI events on disk access, IPC, networking, helper probes or process
launches. Use workers and dispatch UI work back to the UI thread. Debounce active
work when useful; avoid periodic idle polling. Measure actual idle behavior.

## 5. Arcade Link integration

Use canonical ID `arcade.<lowercase-name>` and stable platform bundle IDs. New
actions normally use `<name>.<verb>`; keep the frozen v1 wire contract compatible.
Depend on a published immutable Link tag or exact commit and lock it. At the time
of writing the existing apps use `v0.1.0`:

```toml
arcade-link = { git = "https://github.com/qa-p1/Arcade-Link", tag = "v0.1.0", features = ["watch"] }
```

Add `engines` only if needed. There is no `tokio` feature in this version.
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
| Existing consumers | Explicit peer lists in Rust/C++/frontends, policy defaults, meaningful new actions and tests; bump pinned shared code when needed |
| New app | Exposed and consumed actions, Connected apps, isolated e2e group, packaging metadata and documentation |

Do not silently change protocol v1 or move an existing tag. Prepare shared
changes on branches and validate consumers against the exact candidate commit.
Publish a new tag, bump consumers, merge or release only when the user has
already authorized those operations. If shared repositories or credentials
are unavailable, complete the standalone app and reviewable integration changes,
then report the precise remaining dependency. Do not claim ecosystem onboarding
is finished while those changes remain unshipped.

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
- Measure startup/readiness, warm invocation, idle RSS, idle CPU and repeated
  load. Use a comparable baseline, state process/child accounting and startup
  timing boundaries, and inspect outliers or negative counters. A zero CPU
  median does not prove zero wakeups. Whole-app RSS does not isolate Link cost.
- Aim for no idle polling, responsive startup and memory that plateaus under
  stress. Treat SPEC's Link overhead budgets as targets requiring dedicated
  measurement, not guarantees inherited from using the crate. Optimize measured
  bottlenecks; do not copy allocator tuning from Lens without evidence.

Do not mark unsupported or untested platform behavior as passed. When hardware,
credentials or a service prevent validation, finish independent work and record
the exact unverified scope and reproducible next step.

## 9. Packaging and delivery

Use a repository name consistent with `Arcade-<name>`. Respect an existing
license; for a new app select a compatible license and include its full text
and third-party notices. Do not infer that every Arcade app is MIT licensed.
Check redistribution requirements before bundling dependencies.

Provide reproducible per-user packages appropriate to the stack: AppImage or a
self-contained Linux bundle, Windows per-user installer, macOS app/DMG. Keep
runtime assets complete and optional engines separate. Verify versions, icons,
uninstall behavior and stable executable paths.

For Tools compatibility, generate `arcade-release.json` and `SHA256SUMS.txt`
with the versioned [release manifest tool](https://github.com/qa-p1/Arcade-Link/blob/main/tools/arcade-release.py)
and validate the schema and platform install mappings. SHA-256 checks integrity;
it is not publisher authentication. State signing/notarization status accurately.

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
| `CHANGELOG.md` | User-visible changes |
| License and provenance files | License text, third-party notices and pins for vendored code |

Check local documentation links and command examples. Label historical plans
as historical. Keep the repository clean, review diffs and verify local/remote
commit equality after an authorized push.

The completion report states what was built, where the code and packages are,
which checks passed, what remains incomplete and why. Separate implementation
completion from release readiness and unverified platforms. Do not call the work
finished while an authorized, feasible part of the agreed scope remains undone.

## 11. Suggested implementation order

1. Confirm scope from the idea, inspect references and choose the stack.
2. Build the standalone core and real primary flow.
3. Finish UI, settings, persistence and platform lifecycle.
4. Add Link actions, peer consumption and family registration changes.
5. Verify failures, cross-app behavior, performance and platform builds.
6. Finish packages, licenses, CI and documentation.
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
