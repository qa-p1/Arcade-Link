#!/usr/bin/env bash
# Builds tools/desktop/vpointer next to this script. Needs wayland-scanner,
# libwayland-client and the wlr-protocols XML (WLR_PROTOCOLS points at its root).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
xml=$(find "${WLR_PROTOCOLS:?set WLR_PROTOCOLS}" -name wlr-virtual-pointer-unstable-v1.xml | head -1)
out=$(mktemp -d)
wayland-scanner client-header "$xml" "$out/wlr-virtual-pointer-unstable-v1-client-protocol.h"
wayland-scanner private-code "$xml" "$out/protocol.c"
cc -O2 -Wall -Werror -I"$out" -o "$here/vpointer" "$here/vpointer.c" "$out/protocol.c" $(pkg-config --cflags --libs wayland-client)
echo "built $here/vpointer"
