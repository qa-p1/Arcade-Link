#!/usr/bin/env bash
# Builds tools/desktop/vpointer and tools/desktop/gshortcuts next to this script.
# Needs wayland-scanner, libwayland-client, the wlr-protocols XML (WLR_PROTOCOLS
# points at its root) and the hyprland-protocols XML (HYPRLAND_PROTOCOLS likewise).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
build() { # name protocol-xml header-name
  local out
  out=$(mktemp -d)
  wayland-scanner client-header "$2" "$out/$3"
  wayland-scanner private-code "$2" "$out/protocol.c"
  cc -O2 -Wall -Werror -I"$out" -o "$here/$1" "$here/$1.c" "$out/protocol.c" $(pkg-config --cflags --libs wayland-client)
  echo "built $here/$1"
}
xml=$(find "${WLR_PROTOCOLS:?set WLR_PROTOCOLS}" -name wlr-virtual-pointer-unstable-v1.xml | head -1)
build vpointer "$xml" wlr-virtual-pointer-unstable-v1-client-protocol.h
xml=$(find "${HYPRLAND_PROTOCOLS:?set HYPRLAND_PROTOCOLS}" -name hyprland-global-shortcuts-v1.xml | head -1)
[ -n "$xml" ] || { echo "no hyprland-global-shortcuts-v1.xml under $HYPRLAND_PROTOCOLS"; exit 1; }
build gshortcuts "$xml" hyprland-global-shortcuts-v1-client-protocol.h
