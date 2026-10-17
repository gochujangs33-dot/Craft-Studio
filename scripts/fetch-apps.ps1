# Downloads the official Windows portable builds of every Craft app listed in
# registry.json from GitHub Releases (storytold/*) and extracts them into dist/apps/<id>/.
param(
    [string]$Arch = "x64",
    [switch]$Force
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$root     = Split-Path $PSScriptRoot -Parent
$registry = Get-Content (Join-Path $root "registry.json") -Raw | ConvertFrom-Json
$appsDir  = Join-Path $root "dist\apps"
$cacheDir = Join-Path $root ".cache\downloads"
New-Item -ItemType Directory -Force $appsDir, $cacheDir | Out-Null

foreach ($app in $registry.apps) {
    $id   = $app.id
    $dest = Join-Path $appsDir $id
    if ((Test-Path $dest) -and -not $Force) {
        Write-Host "[skip] $id (already present, use -Force to refresh)"
        continue
    }

    $repo = ($app.repo -replace '^https://github.com/', '')
    $rel  = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers @{ "User-Agent" = "ArtCraftStudio" }
    $asset = $rel.assets | Where-Object { $_.name -like "*-windows-$Arch-portable.zip" } | Select-Object -First 1
    if (-not $asset) {
        $asset = $rel.assets | Where-Object { $_.name -like "*windows*.zip" } | Select-Object -First 1
    }
    if (-not $asset) {
        $asset = $rel.assets | Where-Object { $_.name -like "*windows*$Arch*.exe" -or $_.name -like "*windows*.exe" -or $_.name -like "*.exe" } | Select-Object -First 1
    }
    if (-not $asset) { Write-Warning "[miss] $id has no windows-$Arch asset"; continue }

    $zip = Join-Path $cacheDir $asset.name
    if (-not (Test-Path $zip)) {
        Write-Host "[get ] $id $($rel.tag_name) <- $($asset.name)"
        Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
    }

    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    if ($zip -match '\.exe$') {
        New-Item -ItemType Directory -Force $dest | Out-Null
        Copy-Item $zip (Join-Path $dest "$id.exe") -Force
    } else {
        $tmp = Join-Path $cacheDir "_x_$id"
        if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
        Expand-Archive $zip -DestinationPath $tmp -Force

        # Flatten a single top-level folder if the zip contains one.
        $items = Get-ChildItem $tmp
        if ($items.Count -eq 1 -and $items[0].PSIsContainer) { Move-Item $items[0].FullName $dest }
        else { Move-Item $tmp $dest }
        if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
    }

    Set-Content (Join-Path $dest ".version") ($rel.tag_name -replace '^v', '') -NoNewline
    Write-Host "[ ok ] $id -> $dest"
}
