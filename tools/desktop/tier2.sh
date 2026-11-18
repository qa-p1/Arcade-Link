#!/usr/bin/env bash
# Tier 2 session: tier2.sh sway|kwin|mutter|x11 COMMAND...
# Starts a private D-Bus, the StatusNotifierWatcher and an unlocked keyring, then the compositor
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
  # A real desktop has a Secret Service (KWallet, GNOME Keyring); apps keep their keys there. As in session.sh: a
  # non-empty password creates and unlocks the "login" collection.
  if command -v gnome-keyring-daemon >/dev/null; then
    printf "arcade-ci\n" | gnome-keyring-daemon --unlock --components=secrets --daemonize > "$ARCADE_DESKTOP_ARTIFACTS/keyring.log" 2>&1 || true
  fi
  case "$ARCADE_DESKTOP" in
    sway)   export XDG_CURRENT_DESKTOP=sway; WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c /dev/null > "$log" 2>&1 & ;;
    kwin)   export XDG_CURRENT_DESKTOP=KDE; kwin_wayland --virtual --no-lockscreen --width 1920 --height 1080 > "$log" 2>&1 & ;;
    mutter) export XDG_CURRENT_DESKTOP=GNOME; mutter --headless --wayland --no-x11 --virtual-monitor 1920x1080 > "$log" 2>&1 & ;;
    x11)    export XDG_CURRENT_DESKTOP=X11; Xvfb :77 -screen 0 1920x1080x24 > "$log" 2>&1 & export DISPLAY=:77 ;;
    *) echo "unknown desktop $ARCADE_DESKTOP"; exit 2 ;;
  esac
  comp=$!
  # Up to 60 s: Mutter usually opens its socket in 3 to 6 s, but once took 19 s on a CI runner (2026-11-18), past
  # the 20 s this waited before. A compositor that never comes up fails here with its log, not later with no display.
  up=no
  for _ in $(seq 1 600); do
    if [ "$ARCADE_DESKTOP" = x11 ]; then xdpyinfo -display :77 >/dev/null 2>&1 && { up=yes; break; }
    else ls "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null | grep -v lock | grep -q . && { up=yes; break; }; fi
    kill -0 $comp 2>/dev/null || { echo "$ARCADE_DESKTOP exited during start"; tail -40 "$log"; exit 1; }
    sleep 0.1
  done
  [ $up = yes ] || { echo "$ARCADE_DESKTOP did not open its display in 60 s"; tail -40 "$log"; exit 1; }
  if [ "$ARCADE_DESKTOP" = x11 ]; then openbox > "$ARCADE_DESKTOP_ARTIFACTS/openbox.log" 2>&1 & sleep 0.5
  else export WAYLAND_DISPLAY=$(cd "$XDG_RUNTIME_DIR" && ls wayland-* | grep -v lock | head -1); fi
  [ "$ARCADE_DESKTOP" = sway ] && export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.* | head -1)
  echo "$ARCADE_DESKTOP up (${WAYLAND_DISPLAY:-$DISPLAY})"
  status=0; "$@" || status=$?
  kill $comp 2>/dev/null || true
  exit $status
' bash "$here" "$@"
