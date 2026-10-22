# Runs the app windowed with the given options and saves a screenshot.
#   scripts\shot.ps1 <out.png> [app options...]
# Example: scripts\shot.ps1 local\out\view.png --open path\to\project --frame-all
param([Parameter(Mandatory = $true)][string]$Out, [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest)
$Root = Split-Path -Parent $PSScriptRoot
if (-not [System.IO.Path]::IsPathRooted($Out)) { $Out = Join-Path $Root $Out }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$Size = if ($env:SHOT_SIZE) { $env:SHOT_SIZE } else { "1440x900" }
# --xr-mode off and --desktop: a screenshot run never starts the headset.
& "$Root\scripts\godot.ps1" --xr-mode off --path "$Root\godot" --resolution $Size --position "40,40" '--' --desktop @Rest --shot $Out
exit $LASTEXITCODE
