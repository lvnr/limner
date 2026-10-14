#!/usr/bin/env bash
# Makes a packaged build of the app for this system: builds the Rust extension,
# fetches Godot's export templates when they are not installed yet, and exports the
# project with the matching preset from godot/export_presets.cfg.
#   scripts/package.sh            release build -> local/out/builds/<system>/
#   scripts/package.sh --debug    debug build: quicker to make, and says more when it fails
# The templates are a 1.4 GB download, of which only this system's part is kept
# (in Godot's own folder for them, where the editor would put them too).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cat "$ROOT/scripts/godot-version.txt")"
# Godot names the folder 4.8.dev7 where the download is named 4.8-dev7.
TEMPLATE_VERSION="${VERSION/-/.}"
MODE=release
if [ "${1:-}" = "--debug" ]; then MODE=debug; fi

case "$(uname -s)" in
  Darwin)
    PRESET="macOS"
    OUT="$ROOT/local/out/builds/macos/Limner.zip"
    TEMPLATES="$HOME/Library/Application Support/Godot/export_templates/$TEMPLATE_VERSION"
    NEEDED=(macos.zip) ;;
  Linux)
    PRESET="Linux"
    OUT="$ROOT/local/out/builds/linux/limner.x86_64"
    TEMPLATES="${XDG_DATA_HOME:-$HOME/.local/share}/godot/export_templates/$TEMPLATE_VERSION"
    NEEDED=(linux_debug.x86_64 linux_release.x86_64) ;;
  *) echo "Unsupported platform; on Windows use scripts/package.ps1" >&2; exit 1 ;;
esac

missing=0
for file in "${NEEDED[@]}"; do
  [ -f "$TEMPLATES/$file" ] || missing=1
done
if [ "$missing" = 1 ]; then
  BASE="https://github.com/godotengine/godot-builds/releases/download/$VERSION"
  ASSET="Godot_v${VERSION}_export_templates.tpz"
  DL="$ROOT/tools/godot/dl"
  mkdir -p "$DL" "$TEMPLATES"
  echo "Fetching Godot's export templates ($ASSET, about 1.4 GB)..."
  curl -sSL --fail -o "$DL/SHA512-SUMS.txt" "$BASE/SHA512-SUMS.txt"
  curl -sSL --fail -o "$DL/$ASSET" "$BASE/$ASSET"
  (cd "$DL" && grep " $ASSET\$" SHA512-SUMS.txt | shasum -a 512 -c -)
  unzip -q -o -j "$DL/$ASSET" templates/version.txt "${NEEDED[@]/#/templates/}" -d "$TEMPLATES"
  rm "$DL/$ASSET"
fi

cd "$ROOT"
if [ "$MODE" = release ]; then
  cargo build -p limner-godot --release
else
  cargo build -p limner-godot
fi
# Registers scripts and assets; the first run on a fresh checkout may print a crash trace at exit.
"$ROOT/scripts/godot.sh" --headless --path godot --import >/dev/null 2>&1 || true
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
"$ROOT/scripts/godot.sh" --headless --path godot "--export-$MODE" "$PRESET" "$OUT"
if [ ! -s "$OUT" ]; then
  echo "The export wrote nothing to $OUT; see the messages above." >&2
  exit 1
fi
echo "Packaged ($MODE): $OUT"
if [ "$PRESET" = "macOS" ]; then
  echo "The extension inside is built for this Mac's processor only ($(uname -m)); see docs/xr.md for a universal build."
fi
