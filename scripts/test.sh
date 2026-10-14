#!/usr/bin/env bash
# Runs every test: the Rust workspace, then the interface tests in Godot.
#   scripts/test.sh              everything
#   scripts/test.sh timeline     only the interface test files whose name contains "timeline"
# The interface tests that need a renderer (captures) are skipped here; to include
# them run without --headless:  scripts/godot.sh --path godot --script res://tests/run.gd -- capture
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [ "$#" -eq 0 ]; then
  cargo test --workspace
fi
cargo build -p limner-godot
# Registers new scripts and assets; the first run on a fresh checkout may print a crash trace at exit.
"$ROOT/scripts/godot.sh" --headless --path godot --import >/dev/null 2>&1 || true
"$ROOT/scripts/godot.sh" --headless --path godot --script res://tests/run.gd -- "$@"
