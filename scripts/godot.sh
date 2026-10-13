#!/usr/bin/env bash
# Runs the pinned Godot build with the given arguments.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cat "$ROOT/scripts/godot-version.txt")"
case "$(uname -s)" in
  Darwin) BIN="$ROOT/tools/godot/$VERSION/Godot.app/Contents/MacOS/Godot" ;;
  Linux)  BIN="$ROOT/tools/godot/$VERSION/Godot_v${VERSION}_linux.x86_64" ;;
  *) echo "Unsupported platform; on Windows use scripts/godot.ps1" >&2; exit 1 ;;
esac
if [ ! -x "$BIN" ]; then
  echo "Godot $VERSION not found at $BIN. Run scripts/fetch-godot.sh first." >&2
  exit 1
fi
exec "$BIN" "$@"
