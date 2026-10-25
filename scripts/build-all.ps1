# Builds the launcher in release mode and assembles the runnable bundle in dist/:
#   dist/ArtCraftStudio.exe
#   dist/apps/<id>/<id>.exe   (fetched by fetch-apps.ps1)
#   dist/LICENSES/*
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

if (-not (Test-Path (Join-Path $root "dist\apps"))) {
    & (Join-Path $PSScriptRoot "fetch-apps.ps1")
}

Push-Location (Join-Path $root "launcher")
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }

$dist = Join-Path $root "dist"
$targetExe = Join-Path $dist "ArtCraftStudio.exe"
$srcExe = Join-Path $root "launcher\target\release\ArtCraftStudio.exe"
try {
    Copy-Item $srcExe $dist -Force
} catch {
    $oldExe = Join-Path $dist "ArtCraftStudio.exe.old"
    if (Test-Path $oldExe) { Remove-Item $oldExe -Force -ErrorAction SilentlyContinue }
    Move-Item $targetExe $oldExe -Force
    Copy-Item $srcExe $dist -Force
}

# License notices (Apache-2.0 requires shipping the license + NOTICE files)
$lic = Join-Path $dist "LICENSES"
New-Item -ItemType Directory -Force $lic | Out-Null
Get-ChildItem (Join-Path $dist "apps") -Directory | ForEach-Object {
    $files = Get-ChildItem $_.FullName -File | Where-Object { $_.Name -match '^(LICENSE|NOTICE|COPYING)' }
    foreach ($f in $files) { Copy-Item $f.FullName (Join-Path $lic "$($_.Name)-$($f.Name)") -Force }
}
$distScripts = Join-Path $dist "scripts"
New-Item -ItemType Directory -Force $distScripts | Out-Null
Get-ChildItem (Join-Path $root "scripts") -File | ForEach-Object {
    Copy-Item $_.FullName $distScripts -Force
}
Copy-Item (Join-Path $root "registry.json") $dist -Force
Copy-Item (Join-Path $root "version.json") $dist -Force

# Ensure SavedFiles folders exist for all registered apps
$reg = Get-Content (Join-Path $root "registry.json") -Raw -Encoding UTF8 | ConvertFrom-Json
$distSaves = Join-Path $dist "SavedFiles"
New-Item -ItemType Directory -Force $distSaves | Out-Null
foreach ($app in $reg.apps) {
    $folderName = $app.prefix + "Craft"
    if ($app.id -eq "hop") { $folderName = "HopCraft" }
    New-Item -ItemType Directory -Force (Join-Path $distSaves $folderName) | Out-Null
}

$oldExe = Join-Path $dist "ArtCraftStudio.exe.old"
if (Test-Path $oldExe) { Remove-Item $oldExe -Force -ErrorAction SilentlyContinue }

Write-Host "Bundle ready: $dist\ArtCraftStudio.exe"
