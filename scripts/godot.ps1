# Runs the pinned Godot build with the given arguments.
$Root = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content "$Root\scripts\godot-version.txt").Trim()
$Bin = "$Root\tools\godot\$Version\Godot_v${Version}_win64.exe"
if (-not (Test-Path $Bin)) { throw "Godot $Version not found. Run scripts\fetch-godot.ps1 first." }
& $Bin @args
exit $LASTEXITCODE
