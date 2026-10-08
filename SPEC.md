# Arcade Link specification

**Protocol version 1 (frozen) · manifest schema 1**

Arcade Link is how the Arcade apps (Box, Lens, Look, Wheel, Clipboard, and
the optional manager, Tools) recognize each other and work together. It is a
protocol and a small library, not a process: there is no broker. Apps find
each other through a file-based registry and talk directly over a local
socket. Nothing has to run except the two apps in a conversation.

Implementations: the Rust crate in `crates/arcade-link` (Box, Lens, Look,
Clipboard's core, Tools) and the Qt module in `qt/` (Wheel). Both pass the
conformance vectors in `spec/vectors/`.

## 1. Principles

1. **Standalone first.** Every app works exactly as it does alone. Every
   integration is additive and hidden when the peer is missing.
2. **Never show a broken entry.** An entry is shown only if the peer is
   installed, its Link is enabled, the action supports this input and this
   OS, and the manifest reports it `available`.
3. **No idle discovery polling.** Link targets no idle timers or polling and
   at most one thread blocked in `accept`. Whole-app idle cost includes the
   UI toolkit and other features; measure it separately from Link overhead.
4. **Never block a UI thread** on discovery, IPC or launching a peer.
5. **Each app keeps its safety model.** A request arriving over the Link goes
   through the same checks as the app's own UI (Lens's secret guard, Box's
   grants and never-overwrite rule, Clipboard's Private mode and "receiving
   never writes your clipboard", Look's no-execution sandbox, Wheel's "never
   run shell strings").
6. **Delegate to the owner.** If a peer owns a verb, use it when present and
   use your own fallback when not.
7. **Content moves by reference.** Files travel as paths; in-memory content is
   written once to a handoff file. Bulk data never goes inline.
8. **The user confirms persistence and outbound actions** in the owner's UI,
   or the caller marks them ↗ with a payload preview.

## 2. Locations

`ARCADE_HOME`, if set, replaces every root: `$ARCADE_HOME/apps`,
`$ARCADE_HOME/run`, `$ARCADE_HOME/handoff`.

| | Registry (manifests) | Runtime (endpoints, sockets) | Handoff |
|---|---|---|---|
| Linux | `${XDG_DATA_HOME:-~/.local/share}/arcade/apps/` | `${XDG_RUNTIME_DIR}/arcade/` (fallback `${XDG_CACHE_HOME:-~/.cache}/arcade/run/`) | `${XDG_CACHE_HOME:-~/.cache}/arcade/handoff/` |
| macOS | `~/Library/Application Support/Arcade/apps/` | `$TMPDIR/arcade/` | `~/Library/Caches/Arcade/handoff/` |
| Windows | `%LOCALAPPDATA%\Arcade\apps\` | `%LOCALAPPDATA%\Arcade\run\` (endpoint files only) | `%LOCALAPPDATA%\Arcade\handoff\` |

The runtime and handoff directories are created with mode 0700 and must be
owned by the current user; an implementation refuses a runtime directory that
is a symlink or belongs to someone else.

**Engines.** Helper programs the apps use but never ship (today only
Tesseract) live beside the registry, in `engines/` under the registry's parent
(`${XDG_DATA_HOME:-~/.local/share}/arcade/engines/`,
`~/Library/Application Support/Arcade/engines/`, `%LOCALAPPDATA%\Arcade\engines\`,
or `$ARCADE_HOME/engines`). Launchers go in `engines/bin/`, which every app
searches after `PATH` and the platform's usual install folders, so a copy one
app downloads serves all of them. A download is per user, pinned and
SHA-256-checked before it runs; the user's own install always wins. The Rust
crate implements this behind the `engines` feature.

## 3. Manifests

One file per app: `<registry>/<arcade-id>.json`. Canonical IDs:
`arcade.box`, `arcade.lens`, `arcade.look`, `arcade.wheel`,
`arcade.clipboard`, `arcade.tools`. They are separate from platform bundle
IDs, which never change.

```jsonc
{
  "schema": 1,
  "id": "arcade.look",
  "name": "Arcade Look",
  "version": "0.4.0",
  "link": { "protocol": [1] },
  "executable": "/home/u/Applications/Arcade-Look.AppImage", // $APPIMAGE, never the mount
  "launch": { "background": ["--background"], "invoke": ["--arcade-invoke"] }, // invoke: only with headless actions
  "icon": "/…/arcade.look.png",                              // optional
  "shortcuts": [{ "id": "preview-selection", "accelerator": "Ctrl+Alt+Space" }],
  "settings": { "linkEnabled": true },
  "actions": [ /* Action, below */ ],
  "writtenAt": "2026-10-05T10:00:00Z"
}
```

**Action**

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | `look.preview`, `box:arcade.image.convert#webp` (`#preset` for Box presets) |
| `version` | int | 1 | Bumped on incompatible change; stored by callers (Wheel slots, Box pipelines) |
| `title` | string | required | The verb as the user wants it: "Quick Look", "Compress for sharing" |
| `verb` | string | "" | `preview`, `convert`, `send`, … |
| `accepts` | [type] | [] | Accept patterns (§5.2). Empty: takes no input |
| `produces` | [type] | [] | Output types |
| `effects` | [effect] | [] | §4.3 |
| `interactive` | bool | false | Opens UI; never runs one-shot |
| `privacy` | string | "local" | `local`, `network`, `cloud` |
| `platforms` | [os] | [] | `linux`, `windows`, `macos`; empty means all |
| `available` | bool | true | From cached probes; readers never run probes |
| `reason` | string | — | Why unavailable ("FFmpeg isn't installed") |
| `preset` | string | — | Box: the preset this action runs |
| `featuredFor` | [type] | — | Box: types for which peers show it inline (3–5 per type) |
| `maxBytes` | int | — | Largest accepted input; callers disable the entry above it, with the reason |
| `group` | string | — | Grouping label in long lists |

Rules:

- Apps write their manifest on start (off the startup path) and when their
  capabilities change, atomically (temporary file + rename) and only if the
  content changed apart from `writtenAt`.
- With the app's "Connect with other Arcade apps" switch off, the manifest is
  still written (the app shows as installed) with `settings.linkEnabled:
  false` and no actions, and nothing listens.
- Readers ignore unknown fields, ignore files that don't parse, and ignore a
  manifest whose `executable` doesn't exist. Uninstallers and Arcade Tools
  remove the manifest.
- Readers cache manifests by modification time and refresh on a directory
  watch (inotify / FSEvents / ReadDirectoryChangesW, or
  `QFileSystemWatcher`) or on demand, off the UI thread. Opening a menu that
  shows peer entries does no disk or IPC work.

## 4. Transport and wire protocol

### 4.1 Endpoint

The resident instance listens on a local socket and writes
`<runtime>/<arcade-id>.endpoint` (mode 0600, atomic rename):

```json
{ "protocol": [1], "transport": "unix", "address": "/run/user/1000/arcade/arcade.look.sock",
  "pid": 4242, "startedAt": "2026-10-05T10:00:00Z", "token": "<64 hex chars>" }
```

- Unix: a domain socket `<runtime>/<arcade-id>.sock` (mode 0600). If that path
  exceeds 100 bytes (`sun_path` is 104 bytes on macOS), the socket goes in
  `/tmp/arcade-<uid>/` (0700, owned by the user) under a hashed name. Clients
  always read the address from the endpoint file.
- Windows: a named pipe `\\.\pipe\arcade-<hash>-<arcade-id>`, where the hash
  covers the user name and the runtime directory, with a DACL granting only
  the owner and SYSTEM (`D:P(A;;GA;;;SY)(A;;GA;;;OW)`).
- The token is 32 bytes from the OS random generator, sent in `hello`,
  compared in constant time. It is defense in depth on top of the directory
  and pipe permissions.
- An endpoint is dead if connecting fails or `hello` doesn't answer within
  150 ms. A starting app replaces a dead endpoint; it refuses to start a
  second server when a live one answers.
- Existing single-instance channels (Lens's TCP, Wheel's command socket,
  Look's single-instance plugin, Clipboard's D-Bus) stay; the Link endpoint
  is added alongside them.

### 4.2 Messages

One UTF-8 JSON object per line, at most 1 MiB including the newline. Every
message has `v` (the sender's protocol version, ≥ 1).

| Kind | Fields |
|---|---|
| Request | `v`, `id` (u64), `method`, `params` |
| Notification | `v`, `method`, `params` (no `id`) |
| Response | `v`, `id`, and exactly one of `result` / `error` |

`error` is `{ "code", "message", "reason"?, "limit"? }` (§6). Unknown fields
are ignored and never given authority.

```jsonc
→ {"v":1,"id":1,"method":"hello","params":{"token":"…","client":{"id":"arcade.lens","version":"0.2.0"},"protocol":[1]}}
← {"v":1,"id":1,"result":{"server":{"id":"arcade.box","version":"0.2.0"},"protocol":1}}
→ {"v":1,"id":2,"method":"invoke","params":{"action":"box:arcade.image.convert","preset":"webp",
     "inputs":[{"type":"file/image","path":"/…/handoff/7f3c…/region.png","owner":"arcade.lens"}],
     "options":{},"context":{"source":"arcade.lens","interactive":true,"reason":"user-click"}}}
← {"v":1,"id":2,"result":{"job":"j-41"}}
← {"v":1,"method":"job.progress","params":{"job":"j-41","fraction":0.4,"message":"Encoding"}}
← {"v":1,"method":"job.done","params":{"job":"j-41","status":"success",
     "outputs":[{"type":"file/image","path":"/home/u/Pictures/region.webp"}],"message":"Converted to WebP"}}
```

### 4.3 Methods

| Method | Params → result |
|---|---|
| `hello` | `{token, client:{id,version}, protocol:[…]}` → `{server:{id,version}, protocol}`. Must be the first request, within 2 s; anything else, or a wrong token, is answered `denied` and the connection closed. No common version → `version_mismatch` |
| `describe` | `{}` → `{actions:[Action]}`, the live list |
| `invoke` | `{action, version?, preset?, inputs:[Content], options, context:{source, interactive, reason}}` → an `InvokeResult` `{outputs:[Content], message?, data?}` or `{job}` |
| `job.cancel` | `{job}` → `{cancelled: bool}`. Partial outputs are removed by the owner |
| `subscribe` | `{topics:["app.changed", "job.*"]}` → `{topics}`. Job notifications always go to the connection that started the job |
| `app.status` | `{}` → `{id, version, pid, protocol, busy, jobs, status}`. `status` is an app-defined object; it should include `mode`: `"background"` (started with `--background`, no window) or `"foreground"`, so a manager can relaunch the app the same way after an update |
| `app.activate` | `{}` → `{activated: true}` (bring the main window forward) |
| `app.quit` | `{force?}` → `{quitting: true}`, or `busy` while jobs run unless `force` |

Notifications: `job.progress {job, fraction?, message}`, `job.done {job,
status: "success"|"error"|"cancelled", outputs, message?, data?, error?}`,
`app.changed {app}`.

A server sends `{job}` before any notification for that job. If the caller
disconnects, its running jobs are cancelled. A job that ends without a result
reports `internal`.

**Effects** (Lens's names, adopted by every app): `clipboard`, `writes-files`,
`overwrites-files`, `deletes-files`, `network`, `uploads-content`,
`sends-to-device`, `launch-apps`, `opens-ui`, `persists`,
`executes-commands`, `window-control`. **Privacy classes** (Box's):
`local`, `network`, `cloud`.

### 4.4 One-shot mode

`<exe> --arcade-invoke` reads one `invoke` request line from stdin (no
`hello`; the caller spawned it directly), writes `job.progress` lines and
then one response line with the same `id` to stdout, and exits (0 on
success). It must not start any UI, tray, shortcut, listener or manifest
write. Box (tools), Lens (`lens.recognize`) and Look (`look.inspect`)
implement it. Callers use it only for non-interactive actions of an app that
isn't running and whose manifest has `launch.invoke`.

## 5. Content

### 5.1 Types

| Type | Transport |
|---|---|
| `text/plain`, `text/url`, `text/rich` | `text` inline up to 256 KiB, else a handoff file in `path`. `text/rich` carries `html` plus `text` as the plain fallback |
| `file/<kind>`, `file/<kind>[]` | `path` (single) or `paths` (array) of existing files. Kinds: `image`, `video`, `audio`, `pdf`, `document`, `spreadsheet`, `presentation`, `archive`, `text`, `code`, `font`, `model`, `any` |
| `folder/reference` | `path` of a directory |
| `structured/<name>` | `data` (inline JSON): `color`, `barcode`, `findings`, `devices`, `table`, `file-info`, `pipelines`, `arcade-action` |
| `screen/region` | `data: {rect:{x,y,width,height}, monitor}` in physical pixels of the virtual desktop |

Other fields: `hints` (semantic detail the type loses, e.g. `["command"]`),
`owner` (the app that created a handoff file), `name` (display or suggested
file name), `size` (bytes).

The kind of a file comes from its extension (`spec/vectors/content.json`).

### 5.2 Matching

An action's `accepts` holds patterns; a value matches if any pattern does:

- `*` matches anything; `text/*`, `file/*`, `structured/*`, `screen/*` match
  their family; `file/any` matches any single file.
- `text/url` and `text/rich` also satisfy `text/plain`.
- An array pattern (`file/image[]`, `file/*[]`) also matches a single file
  (a batch of one). A single pattern never matches an array.
- `type;hint=name` additionally requires `name` in `hints`
  (`text/plain;hint=command`).

### 5.3 Handoff files

- The creator writes `<handoff>/<random hex>/<name>` (file 0600, directory
  0700) and sets `owner`.
- The receiver treats it as read-only, never moves or deletes it, and writes
  outputs to its own output location.
- The creator deletes the directory when the job finishes. Every app removes
  handoff directories older than 24 hours at startup (one directory listing,
  off the startup path).
- Existing user files are never copied; only their path travels.

### 5.4 Structured data

The `data` of each `structured/<name>` value. Readers ignore unknown fields;
writers never remove a field within protocol version 1.

| Name | `data` |
|---|---|
| `findings` | array of `{capability, text, summary, confidence, recognizer, details:[{label, value}]}`; `capability` is Lens's (`text`, `url`, `path`, `command`, `color`, `qr`, `email`, …) |
| `file-info` | `{path, name, type, kind, format, mime, size, modified, width?, height?, durationMs?, tags?, family?}` (`type` is the Link content type) |
| `devices` | array of `{name, platform, online}`; `platform` lower-case (`linux`, `windows`, `macos`, `android`, `ios`, `device`). Never keys, IDs or addresses |
| `pipelines` | array of `{id, name, version, accepts:[type], produces:[type], effects:[effect], interactive}`: `accepts` is the first node's input types (empty for a pipeline that takes no input), `interactive` is true when the first node is interactive |
| `arcade-action` | `{app, action, version, title, preset?, options?, input?}`: a runnable reference to another app's action, e.g. for "Add to Wheel". `options` are passed as the invoke's `options` (a pipeline: `{pipeline: id}`); `input` is a suggested input mode (`none`, `clipboard`, `lens-selection`, `file-selection`) |
| `region` | `{rect:{x,y,width,height}, monitor}` (the same as `screen/region`'s `data`) |
| `color` | `{hex, rgb:[r,g,b]}` |
| `barcode` | `{format, text}` |
| `table` | `{columns:[string], rows:[[string]]}` |

`box.pipeline.run` takes the pipeline as `options.pipeline` (its `id`) and the
first node's input in `inputs`; its effects are the union of its nodes'
effects. A pipeline's `{app:"arcade.box", action:"box.pipeline.run",
options.pipeline}` is what Wheel slots and Lens/Look entries store.

## 6. Errors

| Code | Meaning |
|---|---|
| `not_installed` | No manifest, or its executable is gone |
| `not_running` | No live endpoint; also "stopped while working" when a peer dies mid-job |
| `launch_failed` | Started the app but its endpoint didn't answer within 3 s |
| `timeout` | No answer in time |
| `unsupported_input` | The action doesn't take this input |
| `unavailable` | The action can't run here now; `reason` says why |
| `too_large` | `limit` gives the limit in bytes |
| `denied` | `reason`: `private_mode`, `secret`, `user_cancelled`, `disabled`, `token` |
| `busy` | Jobs are running |
| `cancelled` | The job was cancelled |
| `version_mismatch` | No common protocol or action version |
| `internal` | Anything else, including codes unknown to this version |

Standard messages, used verbatim (`{app}` is the failing app's name):

| Code | Message |
|---|---|
| `not_installed` | `{app} isn't installed.` |
| `not_running` | `{app} isn't running.` |
| `launch_failed` | `{app} didn't start.` |
| `timeout` | `{app} didn't respond in time.` |
| `unsupported_input` | `{app} can't open this kind of content.` |
| `unavailable` | `{app} can't do this yet: {reason}.` / `{app} can't do this right now.` |
| `too_large` | Clipboard: `Too large to send to your devices (limit {N MB}).` Others: `Too large for {app} (limit {N MB}).` |
| `denied` | `private_mode`: `{app} is in Private mode.` · `secret`: `Not sent: this looks like a password or key.` · `user_cancelled`: `Cancelled.` · `disabled`: `{app} has connections to other Arcade apps turned off.` · other: `{app} declined this request.` |
| `busy` | `{app} is busy. Try again when its current job finishes.` |
| `cancelled` | `Cancelled.` |
| `version_mismatch` | `{app} needs an update to work with this app.` |
| `internal` | `{app} ran into a problem: {reason}.` / `{app} ran into a problem.` |

Limits are written `16 MB`, `1.5 MB`, `32 KB`, `512 bytes` (binary units).

## 7. Lifecycle

```
caller wants action A from app P
 ├─ P's manifest missing, executable missing, Link off, A unavailable,
 │  A not for this OS, or input not accepted       → don't show the entry
 ├─ endpoint alive → connect + hello (≤ 20 ms)     → invoke
 ├─ not running, A headless, P has launch.invoke   → one-shot process
 └─ not running, A interactive                     → spawn `P launch.background…`,
                                                     wait for the endpoint
      (spinner in the caller after 150 ms; give up after 3 s with launch_failed)
```

Connections are cheap enough to open per call. A caller may keep one open
while subscribed (a settings page, an open preview window).

Link overhead targets (the whole-app regression runner in `benchmarks/`
checks only part of these; it does not isolate Link allocations or prove
zero wakeups):

| Measure | Budget |
|---|---|
| Startup time added to an app | ≤ 5 ms on the critical path (manifest and listener after the first frame) |
| Idle cost | 0 timers, 0 polling, ≤ 1 accept thread blocked, ≤ 1 MB RSS |
| Registry read (5 apps) | ≤ 2 ms warm, ≤ 10 ms cold, never on a UI thread |
| connect + hello | ≤ 20 ms p95 |
| Headless invoke overhead | ≤ 30 ms p95 resident, ≤ 300 ms one-shot |

## 8. Consistency

### 8.1 Standard CLI

| Flag | Meaning |
|---|---|
| `--version` | Print the version |
| `--background` | Start resident without a window |
| `--settings` | Open settings |
| `--quit` | Quit the running instance |
| `--arcade-manifest` | Print the manifest JSON; no side effects |
| `--arcade-invoke` | One-shot mode (§4.4), apps with headless actions |

Existing flags keep working.

### 8.2 Connected apps page

Every app has the same page:

- A master switch, **Connect with other Arcade apps**. Off: no listener, no
  manifest actions (§3).
- One row per Arcade app: glyph, name, state (*Running · v0.2.0* /
  *Installed* / *Not installed*) and a toggle **Use with <this app's full
  name>** ("Use with Arcade Lens"); off hides that peer's entries in this
  app only.
- For an app that isn't installed: one line on what it would add here, and
  **Get** (opens Arcade Tools if installed, otherwise the releases page).
  Promotion appears only on this page, never in palettes, menus or results.
- A diagnostics expander: registry path, endpoint state, last error.

### 8.3 Tray menu

Every app that stays resident has the same tray (menu bar) icon. A click opens
Settings (on macOS the click opens the menu). The menu is:

**Open <App>** · **Open Settings** · **Restart Arcade <App>** · separator ·
**Quit Arcade <App>**

"Open <App>" does the app's main thing (Lens: a capture; Wheel: the wheel;
Look, Box, Clipboard: their window). Restart starts a successor that waits for
the old instance to exit. Start at login is a Settings switch, not a tray
item. If no tray host exists (yet), the app must stay reachable and show the
icon when a host appears.

### 8.4 Naming

- Entries are named as the verb ("Quick Look", "Compress for sharing", "Send
  to my devices") with the owning app's monochrome glyph
  (`assets/glyphs/*.svg`). No "Powered by".
- Outbound actions carry ↗ and a payload preview.
- Integration surfaces use `assets/tokens.json` (per-app accent, neutrals,
  radius, motion). Existing UIs are not reskinned.

### 8.5 Shortcuts

Defaults change for new installs only; a saved shortcut is never changed.

| App | Linux | Windows | macOS |
|---|---|---|---|
| Box | Ctrl+Alt+Space | Ctrl+Alt+Space | Cmd+Shift+Space |
| Look | none (Space in GNOME Files) | Ctrl+Alt+Shift+Space | Ctrl+Option+Space |
| Lens | Ctrl+Alt+Shift+L | Ctrl+Alt+Shift+L | Ctrl+Alt+Shift+L |
| Clipboard | Ctrl+Shift+Space | Ctrl+Alt+V | Cmd+Shift+V |
| Wheel | F8 | F8 | F8 |

Apps publish their effective shortcuts in `shortcuts`. A shortcut recorder
warns "Used by <app>" when another app already uses the accelerator,
comparing normalized accelerators (case-insensitive, modifier order ignored,
Control = Ctrl, Option = Alt, Cmd = Super; `spec/vectors/accelerators.json`),
from the registry, without IPC.

## 9. Versioning

- The protocol version is negotiated in `hello`. Version 1 is frozen; a
  breaking change becomes version 2 and implementations then support N and
  N−1.
- Each action has its own `version`. Stored references (Wheel slots, Box
  pipelines) keep `{app, action, version}`; an incompatible change marks them
  "needs repair" instead of breaking them.
- Manifests carry `schema: 1`; readers accept later schemas and ignore
  unknown fields.

## 10. Security model

The Link authenticates "the same OS user", not "a genuine Arcade app": the
same trust level as each app's CLI. Directory permissions (0700), socket and
endpoint permissions (0600), the Windows pipe DACL, and the random token
keep other users out. Every app applies its own safety rules to Link
requests exactly as to its own UI (§1.5), and nothing exposes history or
clipboard contents without the user acting in the owner's UI.

## 11. Action catalog

| Action | App | Accepts | Produces | Effects | Interactive | One-shot |
|---|---|---|---|---|---|---|
| `look.preview` | Look | `file/*`, `file/*[]`, `folder/reference`, `text/url` | — | opens-ui | yes | no |
| `look.inspect` | Look | `file/*`, `folder/reference` | `structured/file-info` | — | no | yes |
| `look.preview_selection` | Look | — | `file/*[]` with `options.resolveOnly` (the selection, no UI) | opens-ui | yes | no |
| `lens.capture` | Lens | — | `file/image`, `screen/region` | opens-ui | yes | no |
| `lens.capture_and_act` | Lens | — | — | opens-ui | yes | no |
| `lens.analyze` | Lens | `file/image` | — | opens-ui | yes | no |
| `lens.recognize` | Lens | `file/image`, `text/plain` | `structured/findings`, `text/plain` | — | no | yes |
| `lens.pin` | Lens | `file/image` | — | opens-ui | yes | no |
| `box:<tool-id>` (`#preset`) | Box | per catalog | per catalog | per catalog | no | yes |
| `box.open` | Box | `*` | — | opens-ui | yes | no |
| `box.pipeline.run` | Box | per pipeline | per pipeline | union of nodes | first node may be | if the first node isn't |
| `box.pipelines` | Box | — | `structured/pipelines` | — | no | yes |
| `clipboard.add` | Clipboard | `text/*`, `file/image`, `file/any[]` | — | sends-to-device | no | no |
| `clipboard.pick` | Clipboard | — | `text/*`, `file/*` | opens-ui | yes | no |
| `clipboard.devices` | Clipboard | — | `structured/devices` | — | no | no |
| `wheel.add_action` | Wheel | `text/url`, `text/plain;hint=command`, `file/*`, `structured/arcade-action` | — | persists, opens-ui | yes | no |
| `wheel.show` | Wheel | — | — | opens-ui | yes | no |
| `tools.install` | Tools | `text/plain` (an app ID), or `options.app` | — | opens-ui (the user confirms in Tools) | yes | no |

Each app documents its exposed actions in its own repository.

## 12. Conformance

`spec/vectors/` holds wire messages and version negotiation (`wire.json`),
content matching and file kinds (`content.json`), standard error messages
(`errors.json`), manifests (`manifest.json`) and accelerator normalization
(`accelerators.json`). The Rust crate (`cargo test`) and the Qt module
(`qt/tests`, also vendored into Wheel's test suite) run all of them.
`arcade-link mock` is a scriptable fake app for testing a consumer without
building any other app.
