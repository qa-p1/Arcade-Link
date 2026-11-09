#!/usr/bin/env bash
# Starts a private D-Bus, a StatusNotifierWatcher, headless Hyprland and the
# global-shortcuts portal stand-in, then runs "$@" inside that session.
# Exports HYPRLAND_INSTANCE_SIGNATURE, WAYLAND_DISPLAY, DBUS_SESSION_BUS_ADDRESS
# and ARCADE_DESKTOP_ARTIFACTS (screenshots and logs) to the command.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export ARCADE_DESKTOP_ARTIFACTS=${ARCADE_DESKTOP_ARTIFACTS:-$PWD/desktop-artifacts}
mkdir -p "$ARCADE_DESKTOP_ARTIFACTS"
# Short: Hyprland refuses socket paths over ~107 bytes.
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR_OVERRIDE:-$(mktemp -d /tmp/xdg.XXXXXX)}
chmod 700 "$XDG_RUNTIME_DIR"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=Hyprland
exec dbus-run-session -- bash -c '
  set -euo pipefail
  here="$1"; shift
  python3 "$here/sni_watcher_service.py" > "$ARCADE_DESKTOP_ARTIFACTS/sni-watcher.log" 2>&1 &
  # A real desktop has a Secret Service; apps keep their keys there.
  if command -v gnome-keyring-daemon >/dev/null; then
    # A non-empty password creates and unlocks the "login" collection; an empty
    # one asks a GUI prompter instead (none here), leaving no collection.
    printf "arcade-ci\n" | gnome-keyring-daemon --unlock --components=secrets --daemonize > "$ARCADE_DESKTOP_ARTIFACTS/keyring.log" 2>&1 || true
  fi
  if [ "${ARCADE_SEATD:-}" = 1 ]; then
    # Ubuntu ships seatd with its own service, whose socket ours replaces: waiting for the socket to exist could
    # pass on that one, and Hyprland then got Permission denied (or got ours before seatd gave it our group).
    # Stop that service, and wait until this user can actually connect.
    sudo -n systemctl stop seatd.service 2>/dev/null || true
    sudo -n rm -f /run/seatd.sock
    sudo -n seatd -g "$(id -gn)" -l info > "$ARCADE_DESKTOP_ARTIFACTS/seatd.log" 2>&1 &
    seat_ready() {
      python3 -c "import socket; socket.socket(socket.AF_UNIX).connect(\"/run/seatd.sock\")" 2>/dev/null
    }
    for _ in $(seq 1 100); do seat_ready && break; sleep 0.1; done
    seat_ready || { echo "seatd did not start:"; ls -l /run/seatd.sock; cat "$ARCADE_DESKTOP_ARTIFACTS/seatd.log"; exit 1; }
  fi
  if [ -n "${AQ_DRM_DEVICES:-}" ] && ! [ -r "$AQ_DRM_DEVICES" -a -w "$AQ_DRM_DEVICES" ]; then
    echo "cannot open $AQ_DRM_DEVICES for reading and writing:"; ls -l "$AQ_DRM_DEVICES"; exit 1
  fi
  ready="$XDG_RUNTIME_DIR/hypr-ready"
  keep_logs() {
    cp "$XDG_RUNTIME_DIR"/hypr/*/hyprland.log "$ARCADE_DESKTOP_ARTIFACTS/hyprland-full.log" 2>/dev/null || true
    cp "$HOME"/.cache/hyprland/hyprlandCrashReport*.txt "$ARCADE_DESKTOP_ARTIFACTS/" 2>/dev/null || true
  }
  dump() {
    keep_logs
    echo "--- Hyprland log (tail)"; tail -60 "$ARCADE_DESKTOP_ARTIFACTS/hyprland-full.log" 2>/dev/null || tail -60 "$ARCADE_DESKTOP_ARTIFACTS/hyprland.log"
    for c in "$ARCADE_DESKTOP_ARTIFACTS"/hyprlandCrashReport*.txt; do [ -e "$c" ] && { echo "--- $c"; head -60 "$c"; }; done
  }
  ARCADE_DESKTOP_READY_CMD="touch $ready" HYPRLAND_HEADLESS_ONLY=1 AQ_NO_KMS_REQUIREMENT=1 \
    Hyprland --config "$here/hyprland.lua" > "$ARCADE_DESKTOP_ARTIFACTS/hyprland.log" 2>&1 &
  hypr=$!
  for _ in $(seq 1 300); do
    [ -e "$ready" ] && break
    kill -0 "$hypr" 2>/dev/null || { echo "Hyprland exited during start"; dump; exit 1; }
    sleep 0.1
  done
  [ -e "$ready" ] || { echo "Hyprland did not start in 30 s"; dump; exit 1; }
  export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$XDG_RUNTIME_DIR/hypr" | head -1)
  export WAYLAND_DISPLAY=$(cd "$XDG_RUNTIME_DIR" && ls wayland-* | grep -v lock | head -1)
  # Two 1920x1080 outputs (positions from hyprland.lua), whatever the DRM side offers.
  hyprctl output create headless >/dev/null
  hyprctl output create headless >/dev/null
  outputs() { hyprctl -j monitors | python3 -c "import json,sys; print(\" \".join(m[\"name\"] for m in json.load(sys.stdin)))"; }
  for _ in $(seq 1 50); do [ "$(outputs)" = "HEADLESS-1 HEADLESS-2" ] && break; sleep 0.1; done
  [ "$(outputs)" = "HEADLESS-1 HEADLESS-2" ] || { echo "outputs are [$(outputs)], expected HEADLESS-1 HEADLESS-2"; hyprctl monitors all; dump; exit 1; }
  # After Hyprland: like xdg-desktop-portal-hyprland, the stand-in registers bound shortcuts with the compositor.
  python3 "$here/portal_shortcuts.py" > "$ARCADE_DESKTOP_ARTIFACTS/portal.log" 2>&1 &
  portal=$!
  for _ in $(seq 1 100); do grep -q "stand-in ready" "$ARCADE_DESKTOP_ARTIFACTS/portal.log" && break; kill -0 "$portal" 2>/dev/null || break; sleep 0.1; done
  grep -q "stand-in ready" "$ARCADE_DESKTOP_ARTIFACTS/portal.log" || { echo "the portal stand-in did not start:"; cat "$ARCADE_DESKTOP_ARTIFACTS/portal.log"; exit 1; }
  echo "Hyprland $HYPRLAND_INSTANCE_SIGNATURE on $WAYLAND_DISPLAY"
  status=0
  "$@" || status=$?
  keep_logs
  hyprctl dispatch "hl.dsp.exit()" >/dev/null 2>&1 || true
  sleep 1; kill "$hypr" 2>/dev/null || true
  if ls /var/lib/systemd/coredump/* /tmp/core* 2>/dev/null | grep -q .; then echo "coredumps were written"; status=1; fi
  exit $status
' bash "$here" "$@"
