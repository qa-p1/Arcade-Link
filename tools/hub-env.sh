#!/usr/bin/env bash
# Source only in a task shell; never in profiles/startup configuration.
arcade_e2e_link=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
arcade_e2e_coding=$(cd "$arcade_e2e_link/../.." && pwd)
export ARCADE_FIND_REPO="$arcade_e2e_coding/Rust/Arcade-Find-wt-hub"
export ARCADE_SHELF_REPO="$arcade_e2e_coding/C++/Arcade-Shelf-wt-hub"
export ARCADE_LENS_REPO="$arcade_e2e_coding/Rust/Arcade-lens-wt-hub"
export ARCADE_LOOK_REPO="$arcade_e2e_coding/Rust/arcade-look-wt-hub"
export ARCADE_BOX_REPO="$arcade_e2e_coding/Rust/Arcade Box-wt-hub"
export ARCADE_WHEEL_REPO="$arcade_e2e_coding/C++/Arcade wheel-wt-hub"
export ARCADE_CLIPBOARD_REPO="$arcade_e2e_coding/App dev/Arcade-clipboard-wt-hub"
export ARCADE_TOOLS_REPO="$arcade_e2e_coding/Rust/Arcade-tools-wt-hub"
export ARCADE_BOX_BIN="${ARCADE_BOX_BIN:-target/debug/arcade-desktop}"
export ARCADE_BOX_CLI_BIN="${ARCADE_BOX_CLI_BIN:-target/debug/arcadebox}"
export ARCADE_LENS_BIN="${ARCADE_LENS_BIN:-target/debug/arcade-lens}"
export ARCADE_LOOK_BIN="${ARCADE_LOOK_BIN:-src-tauri/target/debug/arcade-look}"
export CARGO_BUILD_JOBS=3
unset arcade_e2e_link arcade_e2e_coding
