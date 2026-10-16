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
Copy-Item (Join-Path $root "launcher\target\release\ArtCraftStudio.exe") $dist -Force

# License notices (Apache-2.0 requires shipping the license + NOTICE files)
$lic = Join-Path $dist "LICENSES"
New-Item -ItemType Directory -Force $lic | Out-Null
Get-ChildItem (Join-Path $dist "apps") -Directory | ForEach-Object {
    $files = Get-ChildItem $_.FullName -File | Where-Object { $_.Name -match '^(LICENSE|NOTICE|COPYING)' }
    foreach ($f in $files) { Copy-Item $f.FullName (Join-Path $lic "$($_.Name)-$($f.Name)") -Force }
}
$distScripts = Join-Path $dist "scripts"
New-Item -ItemType Directory -Force $distScripts | Out-Null
Copy-Item (Join-Path $root "scripts\update-app.ps1") $distScripts -Force
Copy-Item (Join-Path $root "registry.json") $dist -Force

Write-Host "Bundle ready: $dist\ArtCraftStudio.exe"
