#!/usr/bin/env bash
# Prints the runner's OS image (e.g. ubuntu-22.04, macOS-15, Windows-X64), for cache keys of built binaries:
# a binary built on one image may not run on another of the same OS (glibc).
set -euo pipefail
if [[ -r /etc/os-release ]]; then
  . /etc/os-release
  echo "${ID:-linux}-${VERSION_ID:-unknown}-$(uname -m)"
else
  echo "${RUNNER_OS:-$(uname -s)}-${ImageOS:-unknown}-$(uname -m)"
fi
