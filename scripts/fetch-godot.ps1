# Downloads the pinned Godot editor build into tools\godot\<version>\ and verifies its checksum.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content "$Root\scripts\godot-version.txt").Trim()
$Base = "https://github.com/godotengine/godot-builds/releases/download/$Version"
$Asset = "Godot_v${Version}_win64.exe.zip"
$Dest = "$Root\tools\godot\$Version"
$Dl = "$Root\tools\godot\dl"
New-Item -ItemType Directory -Force -Path $Dest, $Dl | Out-Null
Invoke-WebRequest "$Base/SHA512-SUMS.txt" -OutFile "$Dl\SHA512-SUMS.txt"
Invoke-WebRequest "$Base/$Asset" -OutFile "$Dl\$Asset"
$Expected = ((Get-Content "$Dl\SHA512-SUMS.txt" | Where-Object { $_ -match [regex]::Escape($Asset) + '$' }) -split '\s+')[0]
$Actual = (Get-FileHash "$Dl\$Asset" -Algorithm SHA512).Hash.ToLower()
if ($Expected -ne $Actual) { throw "Checksum mismatch for $Asset" }
Expand-Archive -Force "$Dl\$Asset" -DestinationPath $Dest
& "$Dest\Godot_v${Version}_win64.exe" --version
