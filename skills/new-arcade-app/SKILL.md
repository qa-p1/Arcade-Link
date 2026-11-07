---
name: new-arcade-app
description: Turn a vague idea into a released Arcade app — one-page spec, toolkit choice, a repo from the Arcade template, an implementation that passes `arcade-link conformance`, an rc on the hidden test channel, promotion to stable, and a catalog entry. Use when someone wants a new Arcade app.
argument-hint: "[the idea, in any words]"
---

The rules every Arcade app must meet are in Arcade Link's [`ARCADE_APP.md`](../../ARCADE_APP.md) (each with a check
ID); the how-to is [`docs/ARCADE_APP_GUIDE.md`](../../docs/ARCADE_APP_GUIDE.md). Read both first. Don't restate them:
point at them.

1. **Clarify the idea into a one-page spec.** Ask only what changes the design: what the user does, which existing
   Arcade apps it talks to over Link (and which actions it offers them), whether it lives in the background (tray,
   shortcut) or only opens on demand, and what data it keeps. Write `docs/SPEC.md`: purpose, actions, Link
   vocabulary, data and its schema version, budgets (memory PSS/RSS, cold start, show latency), and what is out of
   scope. Feature ideas beyond it go to `BACKLOG.md`.
2. **Pick the toolkit.** Default: Rust + egui (lightest; the template). Tauri only when the UI needs a web view;
   Qt/QML when it must match Shelf/Wheel's overlays; Flutter only for mobile. Record why in `docs/DECISIONS.md`.
3. **Generate from the template.** Create `qa-p1/Arcade-<Name>` from `qa-p1/Arcade-template` (or copy Link's
   `template/`), then rename `arcade.template`, `dev.arcade.Template`, `arcade-template` and "Arcade Template".
   Pin Arcade Link by its newest `v1.x.y` tag.
4. **Implement against `ARCADE_APP.md`.** Keep the template's runtime calls (`runtime::*`, `childenv`, Link server,
   `--status`, Escape steps back). Write the behaviour tests first; every popup and Escape path gets a desktop test
   through Link's `desktop-e2e.yml`.
5. **Make conformance green:** `arcade-link conformance . --binary dist/<App>.AppImage` locally and in CI (the
   template's `ci.yml` already runs it). Add the app's budgets to `ARCADE_APP.md` PERF-1 in a Link PR.
6. **Cut an rc and promote.** Run "Release" with `vX.Y.Z-rc.1`; verify the published files (install from them, the
   desktop suite, the budgets); fix and cut `rc.N+1` until everything passes; then run "Promote" with that rc. Never
   rebuild for stable.
7. **Add the app to the catalog** (`qa-p1/Arcade-catalog` `catalog.json`: id, name, repo, description, version,
   notes) so Tools offers it.
