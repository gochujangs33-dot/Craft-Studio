# Downloads the official Windows portable builds of every Craft app listed in
# registry.json from GitHub Releases (storytold/*) and extracts them into dist/apps/<id>/.
param(
    [string]$Arch = "x64",
    [switch]$Force,
    [string]$App = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Enable TLS 1.2
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
try {
    [System.Net.ServicePointManager]::ServerCertificateValidationCallback = { $true }
} catch {}

$root     = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
$regPath  = Join-Path $root "registry.json"
$registry = Get-Content $regPath -Raw -Encoding UTF8 | ConvertFrom-Json
$appsDir  = Join-Path $root "dist\apps"
$cacheDir = Join-Path $root ".cache\downloads"
$vjsonPath = Join-Path $root "version.json"
New-Item -ItemType Directory -Force $appsDir, $cacheDir | Out-Null

$updatedVersions = @{}

foreach ($app in $registry.apps) {
    $id   = $app.id
    if ($App -and $id -ne $App) { continue }
    $dest = Join-Path $appsDir $id

    $repo = ($app.repo -replace '^https://github.com/', '')
    
    # 1. Direct redirect inspection (Bypasses GitHub API rate limits completely)
    $latestTag = ""
    $effUrl = & curl.exe -s -k -I -L -o NUL -w "%{url_effective}" "https://github.com/$repo/releases/latest"
    if ($effUrl -match '/tag/(v?[0-9\.]+)') {
        $latestTag = ($Matches[1] -replace '^v', '')
    }

    # Fallback to API if redirect inspection failed
    if (-not $latestTag) {
        $tmp = Join-Path $env:TEMP ("gh_fetch_" + [System.Guid]::NewGuid().ToString("N") + ".json")
        & curl.exe -s -k -L -H "User-Agent: ArtCraftStudio" "https://api.github.com/repos/$repo/releases/latest" -o $tmp
        if (Test-Path $tmp) {
            $raw = Get-Content $tmp -Raw -Encoding UTF8
            $rel = $raw | ConvertFrom-Json
            if ($rel -and $rel.tag_name) {
                $latestTag = ($rel.tag_name -replace '^v', '')
            }
            Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        }
    }

    if (-not $latestTag) {
        Write-Warning "[skip] $id could not resolve latest version from $repo"
        continue
    }

    $verFile = Join-Path $dest ".version"
    $currentVer = if (Test-Path $verFile) { (Get-Content $verFile -Raw).Trim() } else { "" }

    if ((Test-Path $dest) -and ($currentVer -eq $latestTag) -and -not $Force) {
        Write-Host "[skip] $id is already at latest v$latestTag" -ForegroundColor DarkGray
        $updatedVersions[$id] = $latestTag
        continue
    }

    Write-Host "[info] ${id}: current v$currentVer -> latest v$latestTag" -ForegroundColor Cyan

    # Determine asset name and download URL
    $assetName = "$id-$latestTag-windows-$Arch-portable.zip"
    if ($id -eq "hop") {
        $assetName = "HOP-windows-$Arch.exe"
    }

    $downloadUrl = "https://github.com/$repo/releases/download/v$latestTag/$assetName"
    
    # Check if download URL is accessible
    $httpCode = & curl.exe -s -k -I -o NUL -w "%{http_code}" -L $downloadUrl
    if ($httpCode -ne "200") {
        # Try without 'v' prefix in download url
        $downloadUrlNoV = "https://github.com/$repo/releases/download/$latestTag/$assetName"
        $httpCode2 = & curl.exe -s -k -I -o NUL -w "%{http_code}" -L $downloadUrlNoV
        if ($httpCode2 -eq "200") {
            $downloadUrl = $downloadUrlNoV
        } else {
            Write-Warning "[miss] $id asset $assetName not directly reachable (HTTP $httpCode)"
            continue
        }
    }

    $zip = Join-Path $cacheDir $assetName
    if (-not (Test-Path $zip) -or (Get-Item $zip).Length -lt 1000 -or $Force) {
        Write-Host "[get ] $id v$latestTag <- $assetName" -ForegroundColor Yellow
        & curl.exe -k -L -A "ArtCraftStudio" "$downloadUrl" -o "$zip"
        if (-not (Test-Path $zip) -or (Get-Item $zip).Length -lt 1000) {
            Write-Warning "[fail] Failed to download $assetName"
            continue
        }
    }

    # Extract or copy to destination
    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }

    if ($zip -match '\.exe$') {
        New-Item -ItemType Directory -Force $dest | Out-Null
        Copy-Item $zip (Join-Path $dest "$id.exe") -Force
    } else {
        $tmp = Join-Path $cacheDir "_x_$id"
        if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
        New-Item -ItemType Directory -Force $tmp | Out-Null

        # Fast extraction using tar.exe if available, fallback to Expand-Archive
        $hasTar = Get-Command tar.exe -ErrorAction SilentlyContinue
        if ($hasTar) {
            & tar.exe -xf "$zip" -C "$tmp"
        } else {
            Expand-Archive "$zip" -DestinationPath "$tmp" -Force
        }

        # Flatten a single top-level folder if the zip contains one.
        $items = Get-ChildItem $tmp
        if ($items.Count -eq 1 -and $items[0].PSIsContainer) { 
            Move-Item $items[0].FullName $dest 
        } else { 
            Move-Item $tmp $dest 
        }
        if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
    }

    Set-Content (Join-Path $dest ".version") $latestTag -NoNewline
    $updatedVersions[$id] = $latestTag
    Write-Host "[ ok ] $id -> $dest (v$latestTag)" -ForegroundColor Green
}

# Update version.json with latest versions
if (Test-Path $vjsonPath) {
    try {
        $vjson = Get-Content $vjsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $vjson.updated_at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        foreach ($prop in $vjson.apps.PSObject.Properties) {
            if ($updatedVersions.ContainsKey($prop.Name)) {
                $prop.Value.version = $updatedVersions[$prop.Name]
                $prop.Value.asset = "$($prop.Name)-$($updatedVersions[$prop.Name])-windows-x64-portable.zip"
            }
        }
        $vjson | ConvertTo-Json -Depth 5 | Set-Content $vjsonPath -Encoding UTF8
        Write-Host "[ ok ] Updated version.json" -ForegroundColor Green
    } catch {}
}
