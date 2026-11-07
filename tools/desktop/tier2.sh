#!/usr/bin/env bash
# Tier 2 session: tier2.sh sway|kwin|mutter|x11 COMMAND...
# Starts a private D-Bus and the StatusNotifierWatcher, then the compositor
# headless (or Xvfb + openbox for x11), and runs COMMAND with
# ARCADE_DESKTOP=<name> and WAYLAND_DISPLAY or DISPLAY set.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
desktop=$1; shift
export ARCADE_DESKTOP=$desktop
export ARCADE_DESKTOP_ARTIFACTS=${ARCADE_DESKTOP_ARTIFACTS:-$PWD/desktop-artifacts}
mkdir -p "$ARCADE_DESKTOP_ARTIFACTS"
export XDG_RUNTIME_DIR=$(mktemp -d); chmod 700 "$XDG_RUNTIME_DIR"
exec dbus-run-session -- bash -c '
  set -euo pipefail
  here=$1; shift
  log="$ARCADE_DESKTOP_ARTIFACTS/$ARCADE_DESKTOP.log"
  python3 "$here/sni_watcher_service.py" > "$ARCADE_DESKTOP_ARTIFACTS/sni-watcher.log" 2>&1 &
  case "$ARCADE_DESKTOP" in
    sway)   export XDG_CURRENT_DESKTOP=sway; WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c /dev/null > "$log" 2>&1 & ;;
    kwin)   export XDG_CURRENT_DESKTOP=KDE; kwin_wayland --virtual --no-lockscreen --width 1920 --height 1080 > "$log" 2>&1 & ;;
    mutter) export XDG_CURRENT_DESKTOP=GNOME; mutter --headless --wayland --no-x11 --virtual-monitor 1920x1080 > "$log" 2>&1 & ;;
    x11)    export XDG_CURRENT_DESKTOP=X11; Xvfb :77 -screen 0 1920x1080x24 > "$log" 2>&1 & export DISPLAY=:77 ;;
    *) echo "unknown desktop $ARCADE_DESKTOP"; exit 2 ;;
  esac
  comp=$!
  for _ in $(seq 1 200); do
    if [ "$ARCADE_DESKTOP" = x11 ]; then xdpyinfo -display :77 >/dev/null 2>&1 && break
    else ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | grep -v lock | grep -q . && break; fi
    kill -0 $comp 2>/dev/null || { echo "$ARCADE_DESKTOP exited during start"; tail -40 "$log"; exit 1; }
    sleep 0.1
  done
  if [ "$ARCADE_DESKTOP" = x11 ]; then openbox > "$ARCADE_DESKTOP_ARTIFACTS/openbox.log" 2>&1 & sleep 0.5
  else export WAYLAND_DISPLAY=$(cd "$XDG_RUNTIME_DIR" && ls wayland-* | grep -v lock | head -1); fi
  [ "$ARCADE_DESKTOP" = sway ] && export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.* | head -1)
  echo "$ARCADE_DESKTOP up (${WAYLAND_DISPLAY:-$DISPLAY})"
  status=0; "$@" || status=$?
  kill $comp 2>/dev/null || true
  exit $status
' bash "$here" "$@"
