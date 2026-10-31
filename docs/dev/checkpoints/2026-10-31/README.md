# Link checkpoint — paused 2026-10-31

Owner requested stop and GitHub preservation. **No release work should continue
from this checkpoint without a new instruction.**

## Released shared work

Current main is `047b686d9b02de6dcf7bdfa77c85bf7eefbf7264`, tagged **v0.4.11**.
This supersedes the owner's initial v0.4.6 baseline. Find adopted these tags.

- PRs #22–#26 / v0.4.7–v0.4.11 contain the shared layout, disabled-ink,
  welcome/disclosure and contrast fixes exposed by Find verification.
- PR #25, v0.4.10: unchecked checkbox boundary contrast. Merge
  `a1c7f01f2364c3deb513ecd17d40ec8232d99f50`.
- PR #26, v0.4.11: neutral welcome hero glyph tile. Merge
  `047b686d9b02de6dcf7bdfa77c85bf7eefbf7264`.
- PR and merge CI logs were read before these tag workflows were dispatched.
  Link v0.4.11's annotated tag object is
  `09078b8deb711a31cf5cbb2fae357960b1ff3b10`; tag workflow
  [41051289485](https://github.com/qa-p1/Arcade-Link/actions/runs/41051289485)
  succeeded. Link is a library: no GitHub releases were created.
- One-use dispatch bridge branches were retained for the owner, never deleted.

## Unpushed exact-timestamp work

Find's last formatter audit found `find_core::fmt::datetime` still used for the
exact file-detail timestamp. The proposed fix was to move that behavior to
`design::format::format_datetime`, not replace it with a relative date and lose
precision. Local branch `fix/shared-exact-timestamp-v0.4.12` was based on the main
SHA above. Five files were edited: Cargo.toml and Cargo.lock (both workspace
packages 0.4.12), design/format.rs, CHANGELOG.md, design/README.md.

**Those unpushed files were lost in the workspace reset at the owner's stop.**
There was no PR #27, no v0.4.12 tag and no merge at that point. This documentation
branch stores the recoverable helper below as an archival snippet, not a build
change. Its exact final formatted file/lockfile bytes cannot be recovered from
the remaining workspace. Do not claim a new version has been published.

```rust
pub fn format_datetime(time_seconds: i64, offset_seconds: i64) -> String {
    let local = i128::from(time_seconds) + i128::from(offset_seconds);
    let day = local.div_euclid(86_400) as i64;
    let (year, month, date) = civil_from_days(day);
    let minutes = local.rem_euclid(86_400) / 60;
    format!("{year:04}-{month:02}-{date:02} {:02}:{:02}", minutes / 60, minutes % 60)
}
```

Planned/locally written test name:
`exact_timestamps_preserve_dates_time_and_explicit_offsets`.
Cases recorded: epoch UTC; -1 second UTC; positive/negative 19,800-second
explicit offsets; 1,770,249,600 → 2026-02-05 00:00; 1,709,251,199 →
2024-02-29 23:59 and +60 seconds → March 1 00:00; both arguments at i64 MAX
or MIN without overflow. Intermediate i128 addition preserves negative epochs,
historical sub-minute offsets and extreme values.

The local format unit subset was observed passing before Cargo attempted an
unrelated integration executable named `link-…`, whose execution was denied by
the environment. The overall command exited nonzero. It had no `--lib` filter;
Clippy and generated design checks placed after it under `set -e` did not run.
No full CI was run for this helper, and no full-suite pass is claimed.

## When work is authorized again

1. Check the next available Link version; do not assume v0.4.12 remains free.
2. Restore/review the helper and focused tests; version bump in the same PR.
3. Run focused `cargo test --locked -p arcade-link --features design,design-egui
   --lib design::format`, appropriate Clippy/design checks, and all CI jobs.
4. Non-draft implementation PR, green CI, merge commit with expected full head,
   dispatch tag.yml; never overwrite or push tags manually.
5. Find pins that tag in all dependencies/runner references and syncs design,
   notices and hashes. Its file detail calls the shared helper; public old core
   function can be a re-export for compatibility.
6. Lens still needs native minimum/125% review. A ten-item sidebar at 125% may
   need shared scrolling; this was only a source-level concern, not a proven
   layout bug. No corresponding Link fix was started. Add missing shared
   controls here only when demonstrated and tested.
7. Shelf's QML component set is still not built. Shelf and Wheel have not started.

Cross-repo handoff:
https://github.com/qa-p1/Arcade-catalog/blob/docs/overhaul-checkpoint-2026-10-31/dev/overhaul/HANDOFF-2026-10-31.md
