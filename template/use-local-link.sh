#!/usr/bin/env bash
# Link CI only: build the template against this checkout of Arcade Link
# instead of the pinned tag. A repo made from the template never runs this.
set -euo pipefail
cd "$(dirname "$0")"
sed -i 's#arcade-link = { git = "https://github.com/qa-p1/Arcade-Link", tag = "v[0-9.]*",#arcade-link = { path = "../crates/arcade-link",#' Cargo.toml
grep -q 'path = "../crates/arcade-link"' Cargo.toml
