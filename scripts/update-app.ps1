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

Write-Output "PROGRESS:5:업데이트 정보 확인 중..."

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
            $suiteAsset = $suiteRel.assets | Where-Object { 
                $_.name -like "$AppId-*.zip" -or $_.name -like "$AppId.zip" -or $_.name -like "$AppId-windows*.zip"
            } | Select-Object -First 1

            if ($suiteAsset) {
                Write-Output "PROGRESS:15:릴리스 패키지 발견: $($suiteAsset.name)"
                $asset = $suiteAsset
                if ($suiteAsset.name -match "$AppId-([0-9\.]+)") {
                    $latestTag = $matches[1]
                } else {
                    $latestTag = ($suiteRel.tag_name -replace '^v', '')
                }
            }
        }
    } catch {}
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

$tempDir = Join-Path $env:TEMP "ArtCraft_Update_$AppId"
if (Test-Path $tempDir) { Remove-Item $tempDir -Recurse -Force }
New-Item -ItemType Directory -Force $tempDir | Out-Null

$zipName = if ($asset) { $asset.name } else { "$AppId-$latestTag-windows-x64-portable.zip" }
$zipPath = Join-Path $tempDir $zipName

# Check local staged release assets first for instant testing
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$localAssets = @(
    (Join-Path $scriptDir "..\release\upload_assets\$zipName"),
    (Join-Path $scriptDir "upload_assets\$zipName"),
    (Join-Path (Get-Location) "release\upload_assets\$zipName")
)
$foundLocal = $null
foreach ($la in $localAssets) {
    if (Test-Path $la) { $foundLocal = $la; break }
}

if ($foundLocal) {
    Write-Output "PROGRESS:25:패키지 복사 중..."
    Copy-Item $foundLocal -Destination $zipPath -Force
    Write-Output "PROGRESS:70:다운로드 완료"
} else {
    if (-not $asset) {
        Write-Error "No Windows portable asset found for $AppId"
        exit 2
    }
    Write-Output "PROGRESS:20:다운로드 서버 연결 중..."
    try {
        $req = [System.Net.HttpWebRequest]::Create($asset.browser_download_url)
        $req.UserAgent = "ArtCraftStudio-Updater"
        $req.Timeout = 120000
        $res = $req.GetResponse()
        $totalBytes = $res.ContentLength
        $inStream = $res.GetResponseStream()
        $outStream = [System.IO.File]::Create($zipPath)
        $buffer = New-Object byte[] 65536
        $totalRead = 0
        $lastReport = 20

        while (($read = $inStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $outStream.Write($buffer, 0, $read)
            $totalRead += $read
            if ($totalBytes -gt 0) {
                $pct = [math]::Floor(20 + ($totalRead / $totalBytes) * 55)
                if ($pct -gt $lastReport + 2 -or $pct -eq 75) {
                    $lastReport = $pct
                    $mbGot = [math]::Round($totalRead / 1MB, 1)
                    $mbTot = [math]::Round($totalBytes / 1MB, 1)
                    Write-Output "PROGRESS:$pct:다운로드 중 ($mbGot MB / $mbTot MB)"
                }
            }
        }
        $outStream.Close()
        $inStream.Close()
        $res.Close()
        Write-Output "PROGRESS:75:다운로드 완료"
    } catch {
        # Fallback to Invoke-WebRequest
        Write-Output "PROGRESS:40:다운로드 진행 중..."
        Invoke-WebRequest $asset.browser_download_url -OutFile $zipPath -UseBasicParsing -TimeoutSec 120
        Write-Output "PROGRESS:75:다운로드 완료"
    }
}

Write-Output "PROGRESS:80:압축 파일 해제 중..."
$extractDir = Join-Path $tempDir "extracted"
Expand-Archive $zipPath -DestinationPath $extractDir -Force

# Flatten if single subfolder exists inside zip
$items = Get-ChildItem $extractDir
$sourceDir = $extractDir
if ($items.Count -eq 1 -and $items[0].PSIsContainer) {
    $sourceDir = $items[0].FullName
}

Write-Output "PROGRESS:90:프로그램 파일 교체 중..."
Get-ChildItem $sourceDir | ForEach-Object {
    Copy-Item $_.FullName -Destination $DestDir -Recurse -Force
}

# Update version stamp
Set-Content (Join-Path $DestDir ".version") $latestTag -NoNewline

# Cleanup temp
Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Output "PROGRESS:100:v$latestTag 업데이트 완료"
exit 0
