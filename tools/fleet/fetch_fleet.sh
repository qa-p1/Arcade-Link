#!/usr/bin/env bash
# fetch_fleet.sh DIR [TAG_KIND] — downloads each app's newest release AppImage
# into DIR as Arcade-<App>.AppImage (the names fleet_budget.py expects).
# TAG_KIND: stable (default: releases/latest) or test (newest prerelease rc).
set -euo pipefail
dir=$1; kind=${2:-stable}; mkdir -p "$dir"
declare -A repos=([Tools]=Arcade-tools [Box]=Arcade-box [Look]=Arcade-look [Find]=Arcade-Find [Lens]=Arcade-lens
                  [Clipboard]=Arcade-clipboard [Shelf]=Arcade-Shelf [Wheel]=Arcade-wheel)
for app in "${!repos[@]}"; do
  api="https://api.github.com/repos/qa-p1/${repos[$app]}/releases"
  if [ "$kind" = stable ]; then json=$(curl -fsSL --retry 3 --max-time 60 "$api/latest")
  else json=$(curl -fsSL --retry 3 --max-time 60 "$api?per_page=20" | jq '[.[] | select(.prerelease and (.tag_name|test("-rc\\.")))][0]'); fi
  url=$(jq -r '[.assets[] | select(.name|test("(x86_64|amd64).*AppImage$|AppImage$"))][0].browser_download_url' <<<"$json")
  [ -n "$url" ] && [ "$url" != null ] || { echo "no AppImage for $app ($kind)"; exit 1; }
  curl -fsSL --retry 3 --max-time 900 -o "$dir/Arcade-$app.AppImage" "$url"
  chmod +x "$dir/Arcade-$app.AppImage"
  echo "$app: $(jq -r .tag_name <<<"$json")"
done
