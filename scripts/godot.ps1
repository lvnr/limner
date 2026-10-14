# Runs the pinned Godot build with the given arguments.
# Callers set $ErrorActionPreference = "Stop", which this script inherits. Windows PowerShell 5.1
# turns each line a program writes to stderr into an error record when the output is redirected
# (as test.ps1 and package.ps1 do for the import), so a harmless Godot warning would end the
# caller. Godot's exit code is what reports failure here.
$ErrorActionPreference = "Continue"
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
