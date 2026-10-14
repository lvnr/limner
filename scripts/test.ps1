# Runs every test: the Rust workspace, then the interface tests in Godot.
#   scripts\test.ps1              everything
#   scripts\test.ps1 timeline     only the interface test files whose name contains "timeline"
# The interface tests that need a renderer (captures) are skipped here; to include
# them run without --headless:  scripts\godot.ps1 --path godot --script res://tests/run.gd -- capture
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
if ($args.Count -eq 0) {
    cargo test --workspace
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
cargo build -p limner-godot
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# Registers new scripts and assets; the first run on a fresh checkout may print a crash trace at exit.
& "$Root\scripts\godot.ps1" --headless --path godot --import *> $null
# --xr-mode off keeps the headset's software from starting for a test run.
& "$Root\scripts\godot.ps1" --headless --xr-mode off --path godot --script res://tests/run.gd -- @args
exit $LASTEXITCODE
