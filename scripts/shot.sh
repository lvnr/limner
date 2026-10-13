#!/usr/bin/env bash
# Runs the app windowed with the given options and saves a screenshot.
#   scripts/shot.sh <out.png> [app options...]
# Example: scripts/shot.sh local/out/view.png --open path/to/project --frame-all
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$1"; shift
case "$OUT" in /*) ;; *) OUT="$ROOT/$OUT" ;; esac
mkdir -p "$(dirname "$OUT")"
"$ROOT/scripts/godot.sh" --path "$ROOT/godot" --resolution "${SHOT_SIZE:-1440x900}" --position 40,40 -- "$@" --shot "$OUT"
