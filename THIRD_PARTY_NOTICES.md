# Third-party notices

Arcade Link is MIT OR Apache-2.0. It bundles or links:

- **Inter** font (`design/fonts/`), © The Inter Project Authors, SIL Open Font License 1.1 (`design/fonts/OFL.txt`).
- Rust crates listed in `Cargo.lock`, under MIT, Apache-2.0, BSD, ISC, Zlib, 0BSD, Unicode-3.0 or MPL-2.0 licences
  (checked by `cargo deny check licenses`); `egui` bundles fonts under OFL-1.1 and the Ubuntu Font Licence 1.0 when
  the `design-egui` feature is used.
- The Qt module (`qt/`) links Qt 6 (LGPL-3.0) dynamically; apps using it ship Qt's notices and relinking information.

Apps generate their own `THIRD_PARTY_NOTICES` for everything they bundle (ARCADE_APP.md REPO-1).
