#!/usr/bin/env bash
# Starts a private D-Bus, a StatusNotifierWatcher, the global-shortcuts portal
# stand-in and headless Hyprland, then runs "$@" inside that session.
# Exports HYPRLAND_INSTANCE_SIGNATURE, WAYLAND_DISPLAY, DBUS_SESSION_BUS_ADDRESS
# and ARCADE_DESKTOP_ARTIFACTS (screenshots and logs) to the command.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export ARCADE_DESKTOP_ARTIFACTS=${ARCADE_DESKTOP_ARTIFACTS:-$PWD/desktop-artifacts}
mkdir -p "$ARCADE_DESKTOP_ARTIFACTS"
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR_OVERRIDE:-$(mktemp -d)}
chmod 700 "$XDG_RUNTIME_DIR"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=Hyprland
exec dbus-run-session -- bash -c '
  set -euo pipefail
  here="$1"; shift
  python3 "$here/sni_watcher_service.py" > "$ARCADE_DESKTOP_ARTIFACTS/sni-watcher.log" 2>&1 &
  python3 "$here/portal_shortcuts.py" > "$ARCADE_DESKTOP_ARTIFACTS/portal.log" 2>&1 &
  ready="$XDG_RUNTIME_DIR/hypr-ready"
  ARCADE_DESKTOP_READY_CMD="touch $ready" HYPRLAND_HEADLESS_ONLY=1 AQ_NO_KMS_REQUIREMENT=1 \
    Hyprland --config "$here/hyprland.lua" > "$ARCADE_DESKTOP_ARTIFACTS/hyprland.log" 2>&1 &
  hypr=$!
  for _ in $(seq 1 300); do
    [ -e "$ready" ] && break
    kill -0 "$hypr" 2>/dev/null || { echo "Hyprland exited during start"; tail -50 "$ARCADE_DESKTOP_ARTIFACTS/hyprland.log"; exit 1; }
    sleep 0.1
  done
  [ -e "$ready" ] || { echo "Hyprland did not start in 30 s"; tail -50 "$ARCADE_DESKTOP_ARTIFACTS/hyprland.log"; exit 1; }
  export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$XDG_RUNTIME_DIR/hypr" | head -1)
  export WAYLAND_DISPLAY=$(cd "$XDG_RUNTIME_DIR" && ls wayland-* | grep -v lock | head -1)
  echo "Hyprland $HYPRLAND_INSTANCE_SIGNATURE on $WAYLAND_DISPLAY"
  status=0
  "$@" || status=$?
  hyprctl dispatch "hl.dsp.exit()" >/dev/null 2>&1 || true
  sleep 1; kill "$hypr" 2>/dev/null || true
  if ls /var/lib/systemd/coredump/* /tmp/core* 2>/dev/null | grep -q .; then echo "coredumps were written"; status=1; fi
  exit $status
' bash "$here" "$@"
