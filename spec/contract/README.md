# Contract suite

Language-neutral checks of the shared runtime behaviour every Arcade app must have, run against the built binary,
whatever toolkit it uses. Rust apps get the behaviour from `arcade_link::runtime`; Shelf and Wheel (C++) and
Clipboard's Flutter shell implement it themselves and must pass the same checks.

| Suite | Rules | How |
|---|---|---|
| `arcade-link conformance <AppImage>` | ID-*, REPO-1, CLI-1…4, FS-1 | no display needed |
| `runtime_contract.py <AppImage>` | RT-1 single instance, RT-2 restart, RT-3 reaping, RT-4 crash reports | inside `tools/desktop/tier2.sh x11` (or the Hyprland session) |
| `tools/desktop/tests/popup_survives_pointer_leave.py` | UI-1 | Hyprland session |

Link CI runs the contract against the released Shelf and Wheel (`.github/workflows/contract.yml`); it is expected to
fail until their v1 specs land.
