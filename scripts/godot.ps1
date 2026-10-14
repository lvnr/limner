# Runs the pinned Godot build with the given arguments.
$Root = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content "$Root\scripts\godot-version.txt").Trim()
$Bin = "$Root\tools\godot\$Version\Godot_v${Version}_win64.exe"
if (-not (Test-Path $Bin)) { throw "Godot $Version not found. Run scripts\fetch-godot.ps1 first." }
# The plain .exe is a windowed program: PowerShell would not wait for it or show what
# it prints. The console wrapper beside it does both and passes the exit code on.
$Console = "$Root\tools\godot\$Version\Godot_v${Version}_win64_console.exe"
if (Test-Path $Console) { $Bin = $Console }
& $Bin @args
exit $LASTEXITCODE
