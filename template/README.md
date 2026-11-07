# Arcade Template

The smallest Arcade app. It already builds, tests, packages an AppImage, passes `arcade-link conformance`, releases
an rc on the hidden test channel and promotes it to stable. Start a new app with the `/new-arcade-app` skill in
Arcade Link, or click "Use this template" and follow [`ARCADE_APP.md`](https://github.com/qa-p1/Arcade-Link/blob/main/ARCADE_APP.md).

| Workflow | What it does |
|---|---|
| `ci.yml` | fmt, clippy, tests, `cargo deny`, the AppImage, conformance (Link's `app-checks.yml`) |
| `release.yml` | builds once and publishes `vX.Y.Z-rc.N` on the hidden test channel |
| `promote.yml` | republishes a verified rc as `vX.Y.Z` with a signed manifest (needs `ARCADE_UPDATE_SIGNING_KEY`) |
| `nightly.yml` | promotes main CI's files to a signed nightly when main changed |

Rename: `arcade.template`, `dev.arcade.Template`, `arcade-template` and "Arcade Template" everywhere, then add the
app's budgets to `ARCADE_APP.md` PERF-1.
