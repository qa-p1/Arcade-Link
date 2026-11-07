# Security policy

Report a vulnerability privately through GitHub's "Report a vulnerability" (Security tab), not in a public issue.
The app talks only over Arcade Link's per-user socket (0600, peer UID checked, token required) and writes only under
`arcade/template`. Crash reports stay local. `cargo deny` runs in CI and Dependabot watches dependencies.
