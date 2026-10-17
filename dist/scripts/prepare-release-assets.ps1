# Prepares all release assets for uploading to https://github.com/gochujangs33-dot/Craft-Studio/releases
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$releaseDir = Join-Path $root "release"
$uploadDir  = Join-Path $releaseDir "upload_assets"

New-Item -ItemType Directory -Force $uploadDir | Out-Null

Write-Host "Preparing release assets for GitHub Releases..."

# 1. Launcher Executable (standalone launcher update)
$launcherExe = Join-Path $root "dist\ArtCraftStudio.exe"
if (Test-Path $launcherExe) {
    Copy-Item $launcherExe (Join-Path $uploadDir "ArtCraftStudio.exe") -Force
    Write-Host "[OK] Copied ArtCraftStudio.exe"
}

# 2. Full Portable Bundle (all 13 apps + launcher)
$fullZip = Join-Path $releaseDir "ArtCraftStudio-portable-x64.zip"
if (Test-Path $fullZip) {
    Copy-Item $fullZip (Join-Path $uploadDir "ArtCraftStudio-portable-x64.zip") -Force
    Write-Host "[OK] Copied ArtCraftStudio-portable-x64.zip"
} else {
    Write-Warning "ArtCraftStudio-portable-x64.zip not found. Run scripts/package.ps1 first."
}

# 3. Generate version.json manifest
$regContent = Get-Content (Join-Path $root "registry.json") -Encoding UTF8 -Raw
$registry = $regContent | ConvertFrom-Json
$versionInfo = @{
    "suite" = "ArtCraft Studio"
    "launcher_version" = "1.0.0"
    "repo" = "https://github.com/gochujangs33-dot/Craft-Studio"
    "updated_at" = (Get-Date).ToString("yyyy-MM-ddTHH:mm:ssZ")
    "apps" = @{}
}

foreach ($app in $registry.apps) {
    $verFile = Join-Path $root "dist\apps\$($app.id)\.version"
    $ver = "0.9.0"
    if (Test-Path $verFile) {
        $ver = (Get-Content $verFile -Raw).Trim()
    }
    $versionInfo.apps[$app.id] = @{
        "name" = $app.prefix + "Craft"
        "version" = $ver
    }
}

$versionJsonPath = Join-Path $uploadDir "version.json"
$versionInfo | ConvertTo-Json -Depth 4 | Set-Content $versionJsonPath -Encoding UTF8
Write-Host "[OK] Generated version.json"

Write-Host ""
Write-Host "=========================================================="
Write-Host " All release assets are ready in: $uploadDir"
Write-Host " Upload these files to: https://github.com/gochujangs33-dot/Craft-Studio/releases"
Write-Host "=========================================================="
explorer.exe $uploadDir
