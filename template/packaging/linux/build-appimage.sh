#!/usr/bin/env bash
# Builds dist/Arcade-Template-<version>-x86_64.AppImage from target/release.
set -euo pipefail
cd "$(dirname "$0")/../.."
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
app=$(mktemp -d)/AppDir
install -Dm755 target/release/arcade-template "$app/usr/bin/arcade-template"
install -Dm644 packaging/linux/dev.arcade.Template.desktop "$app/dev.arcade.Template.desktop"
# The root icon and .DirIcon are PNG: file managers and thumbnailers cannot be expected to render SVG.
install -Dm644 packaging/linux/dev.arcade.Template.png "$app/dev.arcade.Template.png"
install -Dm644 packaging/linux/dev.arcade.Template.png "$app/usr/share/icons/hicolor/256x256/apps/dev.arcade.Template.png"
install -Dm644 packaging/linux/dev.arcade.Template.svg "$app/usr/share/icons/hicolor/scalable/apps/dev.arcade.Template.svg"
install -Dm644 packaging/linux/arcade-template-symbolic.svg "$app/usr/share/icons/hicolor/symbolic/apps/arcade-template-symbolic.svg"
install -Dm644 LICENSE "$app/usr/share/doc/arcade-template/LICENSE"
install -Dm644 THIRD_PARTY_NOTICES.md "$app/usr/share/doc/arcade-template/THIRD_PARTY_NOTICES.md"
ln -s usr/bin/arcade-template "$app/AppRun"
ln -s dev.arcade.Template.png "$app/.DirIcon"
tool=${APPIMAGETOOL:-appimagetool}
if ! command -v "$tool" >/dev/null; then
  tool=$(mktemp -d)/appimagetool
  curl -fsSL --retry 3 --max-time 300 -o "$tool" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$tool"
fi
mkdir -p dist
# APPIMAGE_RUNTIME: a downloaded type2 runtime, so appimagetool does not fetch it from GitHub on every build.
runtime=()
if [ -n "${APPIMAGE_RUNTIME:-}" ]; then runtime=(--runtime-file "$APPIMAGE_RUNTIME"); fi
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "${runtime[@]}" "$app" "dist/Arcade-Template-$version-x86_64.AppImage"
echo "dist/Arcade-Template-$version-x86_64.AppImage"
