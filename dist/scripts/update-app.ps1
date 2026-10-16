# In-place Replacement Updater for ArtCraft apps (3M-style inplace replace)
param(
    [Parameter(Mandatory=$true)]
    [string]$AppId,

    [Parameter(Mandatory=$true)]
    [string]$RepoUrl,

    [Parameter(Mandatory=$true)]
    [string]$DestDir,

    [string]$Arch = "x64",

    [string]$SuiteRepo = "gochujangs33-dot/Craft-Studio"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

Write-Host "Updating $AppId in-place at $DestDir..."

# Make sure destination exists
if (-not (Test-Path $DestDir)) {
    New-Item -ItemType Directory -Force $DestDir | Out-Null
}

$headers = @{ "User-Agent" = "ArtCraftStudio-Updater" }
$asset = $null
$latestTag = "latest"

# 1. First check user's suite repo (https://github.com/gochujangs33-dot/Craft-Studio/releases)
if ($SuiteRepo) {
    try {
        $suiteRel = Invoke-RestMethod "https://api.github.com/repos/$SuiteRepo/releases/latest" -Headers $headers -TimeoutSec 10
        if ($suiteRel -and $suiteRel.assets) {
            # Look for app-specific asset like photocraft-*.zip or photocraft.zip
            $suiteAsset = $suiteRel.assets | Where-Object { 
                $_.name -like "$AppId-*.zip" -or $_.name -like "$AppId.zip" -or $_.name -like "$AppId-windows*.zip"
            } | Select-Object -First 1

            if ($suiteAsset) {
                Write-Host "Found custom $AppId update asset in $SuiteRepo: $($suiteAsset.name)"
                $asset = $suiteAsset
                # Extract version from asset name or release tag
                if ($suiteAsset.name -match "$AppId-([0-9\.]+)") {
                    $latestTag = $matches[1]
                } else {
                    $latestTag = ($suiteRel.tag_name -replace '^v', '')
                }
            }
        }
    } catch {
        # ignore and fallback to upstream repo
    }
}

# 2. Fallback to upstream official repo (storytold/<id>)
if (-not $asset) {
    $repoPath = ($RepoUrl -replace '^https://github.com/', '')
    try {
        $rel = Invoke-RestMethod "https://api.github.com/repos/$repoPath/releases/latest" -Headers $headers -TimeoutSec 15
        $latestTag = $rel.tag_name -replace '^v', ''
        $asset = $rel.assets | Where-Object { $_.name -like "*-windows-$Arch-portable.zip" } | Select-Object -First 1
        if (-not $asset) {
            $asset = $rel.assets | Where-Object { $_.name -like "*windows*.zip" } | Select-Object -First 1
        }
    } catch {
        Write-Error "Failed to fetch release info from GitHub: $_"
        exit 1
    }
}

if (-not $asset) {
    Write-Error "No Windows portable asset found for $AppId"
    exit 2
}

$tempDir = Join-Path $env:TEMP "ArtCraft_Update_$AppId"
if (Test-Path $tempDir) { Remove-Item $tempDir -Recurse -Force }
New-Item -ItemType Directory -Force $tempDir | Out-Null

$zipPath = Join-Path $tempDir $asset.name
Write-Host "Downloading $($asset.name) (version $latestTag)..."
try {
    Invoke-WebRequest $asset.browser_download_url -OutFile $zipPath -UseBasicParsing -TimeoutSec 120
} catch {
    Write-Error "Download failed: $_"
    exit 3
}

$extractDir = Join-Path $tempDir "extracted"
Expand-Archive $zipPath -DestinationPath $extractDir -Force

# Flatten if single subfolder exists inside zip
$items = Get-ChildItem $extractDir
$sourceDir = $extractDir
if ($items.Count -eq 1 -and $items[0].PSIsContainer) {
    $sourceDir = $items[0].FullName
}

Write-Host "Replacing files in $DestDir..."
# Copy-Item with overwrite to replace existing files in-place
Get-ChildItem $sourceDir | ForEach-Object {
    Copy-Item $_.FullName -Destination $DestDir -Recurse -Force
}

# Update version stamp
Set-Content (Join-Path $DestDir ".version") $latestTag -NoNewline

# Cleanup temp
Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "SUCCESS: $AppId updated in-place to v$latestTag"
exit 0
