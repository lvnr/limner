# Makes a packaged build of the app for Windows: builds the Rust extension, fetches
# Godot's export templates when they are not installed yet, and exports the project
# with the "Windows Desktop" preset from godot\export_presets.cfg.
#   scripts\package.ps1            release build -> local\out\builds\windows\
#   scripts\package.ps1 -DebugBuild   debug build: quicker to make, and says more when it fails
# The templates are a 1.4 GB download, of which only the Windows part is kept
# (in Godot's own folder for them, where the editor would put them too).
param([switch]$DebugBuild)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Version = (Get-Content "$Root\scripts\godot-version.txt").Trim()
# Godot names the folder 4.8.dev7 where the download is named 4.8-dev7.
$TemplateVersion = $Version -replace "-", "."
$Mode = if ($DebugBuild) { "debug" } else { "release" }
$Out = "$Root\local\out\builds\windows\Limner.exe"
$Templates = Join-Path $env:APPDATA "Godot\export_templates\$TemplateVersion"
$Needed = @(
    "windows_debug_x86_64.exe", "windows_debug_x86_64_console.exe",
    "windows_release_x86_64.exe", "windows_release_x86_64_console.exe"
)

$Missing = $Needed | Where-Object { -not (Test-Path (Join-Path $Templates $_)) }
if ($Missing) {
    $Base = "https://github.com/godotengine/godot-builds/releases/download/$Version"
    $Asset = "Godot_v${Version}_export_templates.tpz"
    $Dl = "$Root\tools\godot\dl"
    New-Item -ItemType Directory -Force -Path $Dl, $Templates | Out-Null
    Write-Host "Fetching Godot's export templates ($Asset, about 1.4 GB)..."
    Invoke-WebRequest "$Base/SHA512-SUMS.txt" -OutFile "$Dl\SHA512-SUMS.txt"
    Invoke-WebRequest "$Base/$Asset" -OutFile "$Dl\$Asset"
    $Expected = ((Get-Content "$Dl\SHA512-SUMS.txt" | Where-Object { $_ -match [regex]::Escape($Asset) + '$' }) -split '\s+')[0]
    $Actual = (Get-FileHash "$Dl\$Asset" -Algorithm SHA512).Hash.ToLower()
    if ($Expected -ne $Actual) { throw "Checksum mismatch for $Asset" }
    # The download is a zip file; take only what Windows needs out of it.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Zip = [System.IO.Compression.ZipFile]::OpenRead("$Dl\$Asset")
    try {
        foreach ($Name in @("version.txt") + $Needed) {
            $Entry = $Zip.GetEntry("templates/$Name")
            if ($null -eq $Entry) { throw "$Asset has no templates/$Name" }
            [System.IO.Compression.ZipFileExtensions]::ExtractToFile($Entry, (Join-Path $Templates $Name), $true)
        }
    } finally {
        $Zip.Dispose()
    }
    Remove-Item "$Dl\$Asset"
}

Set-Location $Root
# Link the C runtime into the extension, so the package does not need the Visual C++
# Redistributable (VCRUNTIME140.dll), which not every PC has. This differs from the
# flags of a plain cargo build, so the first package after one rebuilds everything.
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static"
if ($DebugBuild) { cargo build -p limner-godot } else { cargo build -p limner-godot --release }
$Built = $LASTEXITCODE
Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
if ($Built -ne 0) { exit $Built }
# Registers scripts and assets; the first run on a fresh checkout may print a crash trace at exit.
& "$Root\scripts\godot.ps1" --headless --xr-mode off --path godot --import *> $null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
if (Test-Path $Out) { Remove-Item $Out }
& "$Root\scripts\godot.ps1" --headless --xr-mode off --path godot "--export-$Mode" "Windows Desktop" $Out
if (-not (Test-Path $Out)) { throw "The export wrote nothing to $Out; see the messages above." }
Write-Host "Packaged ($Mode): $Out"
