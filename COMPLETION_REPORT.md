# Arcade ecosystem completion report

Verified 2026-10-08. Phases 0–9 are implemented, including the owner's later
dependency cleanup. The remaining exceptions are listed below; performance
budgets are not claimed to pass universally.

## Delivered

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

## Verification

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

## Final performance pass

Fresh isolated measurements at 17:24 IST on 2026-10-08, five runs per app,
using the unchanged baseline method. Raw samples:
[final-2026-10-08.json](benchmarks/final-2026-10-08.json).
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

## Stress test

Run in the isolated session (`python3 tools/e2e.py run -- python3 tools/stress.py`,
then `tools/leak.py` for ten-round memory), on the shipped builds:

| Load | Result |
|---|---|
| Box: 900 text-tool calls over the Link, 16 concurrent | 0 failures, p50 12 ms, p95 17 ms |
| Box: built-in PDF convert (51-page ODT), searchable PDF, images to PDF, system speech; 32 calls, 4 concurrent | 0 failures, p50 0.4 s, max 1.0 s |
| Lens: 180 `lens.recognize` OCR calls, 6 concurrent | 0 failures, p50 0.19 s |
| Box OCR through Lens: 10 calls, 3 concurrent | 0 failures |
| Box SIGKILLed 0.5 s into a 1500-call burst | 1 in-flight call failed cleanly ("closed the connection"); the Link client relaunched Box and the other 1499 succeeded; no stray processes |
| Clean quit after the load | Box 0.03 s, Lens 0.2 s, exit 0 |
| Ten rounds of 300 Box calls + 60 Lens OCR calls | Box plateaus at about 433 MiB; Lens was creeping 115 → 143 MiB |

The Lens creep was glibc malloc-arena fragmentation (rayon workers plus a
thread per Link connection), not a leak: with two arenas it stayed flat. Lens
now caps arenas at two on Linux (`cd2eebd`) and holds at 122–126 MiB under the
same load. On a brand-new profile Box's first engine check takes about 6 s;
until it finishes, tools that need an engine report "still checking" to other
apps. Box's idle CPU is about one 10 ms scheduler tick every few seconds.

## Remaining limits and owner actions

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

## Documentation follow-up (2026-10-08)

All seven repositories now have a current status page and documentation links.
Historical plans are identified separately from current implementation evidence.
The reusable [new-app brief](NEW_APP_SPEC.md) can be attached to an app idea;
it covers standalone behavior, dependencies, Link onboarding, platform tests,
packaging and delivery without assuming release authorization.

The documentation audit also makes product boundaries explicit: Box pipelines
execute sequentially in topological order; generic condition/retry/variable
features are not implemented. Box reads Groq keys from the environment or a
plaintext `.env`, not an OS credential-store adapter. Its main license text and
signing/plugin-registry work remain release gates in
[Box's release notes](https://github.com/qa-p1/Arcade-box/blob/arcade/link/docs/release.md).
The ecosystem completion statement does not certify every feature envisioned
in the original app product plans or imply that application releases were published.
