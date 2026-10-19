# Downloads the official Windows portable builds of every Craft app listed in
# registry.json from GitHub Releases (storytold/*) and extracts them into dist/apps/<id>/.
param(
    [string]$Arch = "x64",
    [switch]$Force
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

$headers = @{ "User-Agent" = "ArtCraftStudio" }
$updatedVersions = @{}

foreach ($app in $registry.apps) {
    $id   = $app.id
    $dest = Join-Path $appsDir $id

    $repo = ($app.repo -replace '^https://github.com/', '')
    
    # Fetch release info with curl fallback
    $rel = $null
    try {
        $rel = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers -TimeoutSec 8
    } catch {
        $tmp = Join-Path $env:TEMP ("gh_fetch_" + [System.Guid]::NewGuid().ToString("N") + ".json")
        & curl.exe -s -k -L -H "User-Agent: ArtCraftStudio" "https://api.github.com/repos/$repo/releases/latest" -o $tmp
        if (Test-Path $tmp) {
            $raw = Get-Content $tmp -Raw -Encoding UTF8
            $rel = $raw | ConvertFrom-Json
            Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        }
    }

    if (-not $rel -or -not $rel.tag_name) {
        Write-Warning "[skip] $id could not fetch release from $repo"
        continue
    }

    $latestTag = ($rel.tag_name -replace '^v', '')
    $verFile = Join-Path $dest ".version"
    $currentVer = if (Test-Path $verFile) { (Get-Content $verFile -Raw).Trim() } else { "" }

    if ((Test-Path $dest) -and ($currentVer -eq $latestTag) -and -not $Force) {
        Write-Host "[skip] $id is already at latest v$latestTag (use -Force to re-download)"
        $updatedVersions[$id] = $latestTag
        continue
    }

    Write-Host "[info] ${id}: current v$currentVer -> latest v$latestTag" -ForegroundColor Cyan

    $asset = $rel.assets | Where-Object { $_.name -like "*-windows-$Arch-portable.zip" } | Select-Object -First 1
    if (-not $asset) {
        $asset = $rel.assets | Where-Object { $_.name -like "*windows*.zip" } | Select-Object -First 1
    }
    if (-not $asset) {
        $asset = $rel.assets | Where-Object { $_.name -like "*windows*$Arch*.exe" -or $_.name -like "*windows*.exe" -or $_.name -like "*.exe" } | Select-Object -First 1
    }
    if (-not $asset) { Write-Warning "[miss] $id has no windows-$Arch asset"; continue }

    $zip = Join-Path $cacheDir $asset.name
    if (-not (Test-Path $zip) -or $Force) {
        Write-Host "[get ] $id $($rel.tag_name) <- $($asset.name)" -ForegroundColor Yellow
        & curl.exe -k -L -A "ArtCraftStudio" $asset.browser_download_url -o $zip
        if (-not (Test-Path $zip) -or (Get-Item $zip).Length -lt 1000) {
            Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
        }
    }

    if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
    if ($zip -match '\.exe$') {
        New-Item -ItemType Directory -Force $dest | Out-Null
        if ($id -eq "hop" -or $asset.name -match 'setup|install') {
            $tmpExtract = Join-Path $cacheDir "_x_$id"
            if (Test-Path $tmpExtract) { Remove-Item $tmpExtract -Recurse -Force }
            New-Item -ItemType Directory -Force $tmpExtract | Out-Null
            Start-Process -FilePath $zip -ArgumentList "/S /D=$tmpExtract" -Wait -NoNewWindow
            Start-Sleep -Seconds 1
            if (Test-Path (Join-Path $tmpExtract "uninstall.exe")) {
                Remove-Item (Join-Path $tmpExtract "uninstall.exe") -Force -ErrorAction SilentlyContinue
            }
            Get-ChildItem $tmpExtract | Copy-Item -Destination $dest -Recurse -Force
            $realExe = Get-ChildItem -Path $dest -Filter "*.exe" | Where-Object { $_.Name -notmatch 'uninst|setup|crash|update' } | Select-Object -First 1
            if ($realExe) {
                Copy-Item $realExe.FullName (Join-Path $dest "$id.exe") -Force
            }
            Remove-Item $tmpExtract -Recurse -Force -ErrorAction SilentlyContinue
        } else {
            Copy-Item $zip (Join-Path $dest "$id.exe") -Force
        }
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
            }
        }
        $vjson | ConvertTo-Json -Depth 5 | Set-Content $vjsonPath -Encoding UTF8
        Write-Host "[ ok ] Updated version.json" -ForegroundColor Green
    } catch {}
}
