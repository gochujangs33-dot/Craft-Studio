# In-place Replacement Updater for ArtCraft apps
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

# Enable TLS 1.2 and trust certificate for environments behind corporate / Zscaler SSL inspection
[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
try {
    [System.Net.ServicePointManager]::ServerCertificateValidationCallback = { $true }
} catch {}

function Get-RemoteJson([string]$Url) {
    # 1. Try curl.exe with -k (bypasses untrusted root cert / corporate SSL inspection) and -A
    $tmpJson = Join-Path $env:TEMP ("artcraft_net_" + [System.Guid]::NewGuid().ToString("N") + ".json")
    try {
        & curl.exe -k -L -s --max-time 15 -A "ArtCraftStudio-Updater" "$Url" -o "$tmpJson"
        if ($LASTEXITCODE -eq 0 -and (Test-Path $tmpJson)) {
            $raw = Get-Content $tmpJson -Raw -Encoding UTF8 -ErrorAction SilentlyContinue
            if ($raw -and ($raw.Trim().StartsWith("{") -or $raw.Trim().StartsWith("["))) {
                $obj = $raw | ConvertFrom-Json -ErrorAction SilentlyContinue
                if ($obj) {
                    Remove-Item $tmpJson -Force -ErrorAction SilentlyContinue
                    return $obj
                }
            }
        }
    } catch {}
    finally {
        if (Test-Path $tmpJson) { Remove-Item $tmpJson -Force -ErrorAction SilentlyContinue }
    }

    # 2. Fallback to Invoke-RestMethod with TLS 1.2
    try {
        return (Invoke-RestMethod $Url -Headers @{ "User-Agent" = "ArtCraftStudio-Updater" } -TimeoutSec 15)
    } catch {
        return $null
    }
}

Write-Output "PROGRESS:5:업데이트 정보 확인 중..."

# Make sure destination exists
if (-not (Test-Path $DestDir)) {
    New-Item -ItemType Directory -Force $DestDir | Out-Null
}

$asset = $null
$downloadUrl = ""
$latestTag = "latest"
$zipName = ""
$expectedBytes = 0

$repoPath = ($RepoUrl -replace '^https://github.com/', '')

# Tier 0: Direct version.json manifest lookup from raw.githubusercontent.com (No API rate limits!)
$vjson = Get-RemoteJson "https://raw.githubusercontent.com/$SuiteRepo/main/version.json"
if ($vjson -and $vjson.apps -and $vjson.apps.$AppId) {
    $vinfo = $vjson.apps.$AppId
    $tagCandidate = $vinfo.version
    $assetCandidate = $vinfo.asset
    
    $candUrls = @(
        "https://github.com/$repoPath/releases/download/v$tagCandidate/$assetCandidate",
        "https://github.com/$repoPath/releases/download/$tagCandidate/$assetCandidate",
        "https://github.com/$repoPath/releases/latest/download/$assetCandidate"
    )
    foreach ($u in $candUrls) {
        $code = & curl.exe -s -o NUL -w "%{http_code}" -L --max-time 6 "$u"
        if ($code -eq "200") {
            $downloadUrl = $u
            $latestTag = $tagCandidate
            $zipName = $assetCandidate
            Write-Output "PROGRESS:15:매니페스트 기반 직접 다운로드 채널 연결: $zipName (v$latestTag)"
            break
        }
    }
}

# Tier 1: Suite repository releases (if available)
if (-not $downloadUrl -and $SuiteRepo) {
    $suiteRel = Get-RemoteJson "https://api.github.com/repos/$SuiteRepo/releases/latest"
    if ($suiteRel -and $suiteRel.assets) {
        $suiteAsset = $suiteRel.assets | Where-Object { 
            $_.name -like "$AppId-*.zip" -or $_.name -like "$AppId.zip" -or $_.name -like "$AppId-windows*.zip" -or $_.name -like "$AppId*.exe"
        } | Select-Object -First 1

        if ($suiteAsset) {
            Write-Output "PROGRESS:15:릴리스 패키지 발견: $($suiteAsset.name)"
            $downloadUrl = $suiteAsset.browser_download_url
            $zipName = $suiteAsset.name
            $expectedBytes = if ($suiteAsset.size) { [int64]$suiteAsset.size } else { 0 }
            if ($suiteAsset.name -match "$AppId-([0-9\.]+)") {
                $latestTag = $matches[1]
            } else {
                $latestTag = ($suiteRel.tag_name -replace '^v', '')
            }
        }
    }
}

# Tier 2: Upstream official repo (storytold/<id>)
if (-not $downloadUrl) {
    $rel = Get-RemoteJson "https://api.github.com/repos/$repoPath/releases/latest"
    if ($rel -and $rel.assets) {
        $latestTag = ($rel.tag_name -replace '^v', '')
        $asset = $rel.assets | Where-Object { $_.name -like "*-windows-$Arch-portable.zip" } | Select-Object -First 1
        if (-not $asset) {
            $asset = $rel.assets | Where-Object { $_.name -like "*windows*.zip" } | Select-Object -First 1
        }
        if (-not $asset) {
            $asset = $rel.assets | Where-Object { $_.name -like "*windows*$Arch*.exe" -or $_.name -like "*windows*.exe" -or $_.name -like "*.exe" } | Select-Object -First 1
        }
        if ($asset) {
            Write-Output "PROGRESS:15:공식 릴리스 패키지 발견: $($asset.name)"
            $downloadUrl = $asset.browser_download_url
            $zipName = $asset.name
            $expectedBytes = if ($asset.size) { [int64]$asset.size } else { 0 }
        }
    }
}

# Tier 3: Direct standard asset probe fallbacks (bypasses rate limit if API returned 403)
if (-not $downloadUrl) {
    $fallbackAssets = @(
        "$AppId-windows-$Arch-portable.zip",
        "$AppId-windows-x64-portable.zip",
        "$AppId-windows.zip",
        "$AppId-windows-x64.exe",
        "HOP-windows-x64.exe"
    )
    foreach ($fa in $fallbackAssets) {
        $testUrl = "https://github.com/$repoPath/releases/latest/download/$fa"
        $code = & curl.exe -s -o NUL -w "%{http_code}" -L --max-time 6 "$testUrl"
        if ($code -eq "200") {
            $downloadUrl = $testUrl
            $zipName = $fa
            Write-Output "PROGRESS:15:직접 자산 프로브 연결 성공: $zipName"
            break
        }
    }
}

if (-not $downloadUrl) {
    Write-Error "GitHub에서 $AppId 릴리스 패키지 다운로드 URL을 찾을 수 없습니다."
    exit 2
}

$tempDir = Join-Path $env:TEMP "ArtCraft_Update_$AppId"
if (Test-Path $tempDir) { Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force $tempDir | Out-Null

if (-not $zipName) {
    $zipName = "$AppId-$latestTag-windows-x64-portable.zip"
}
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
    Write-Output "PROGRESS:25:로컬 패키지 복사 중..."
    Copy-Item $foundLocal -Destination $zipPath -Force
    Write-Output "PROGRESS:75:다운로드 완료"
} else {
    Write-Output "PROGRESS:20:다운로드 서버 연결 중..."
    $downloadOk = $false

    # Method 1: curl.exe in background with live progress tracking
    try {
        $pinfo = New-Object System.Diagnostics.ProcessStartInfo
        $pinfo.FileName = "curl.exe"
        $pinfo.Arguments = "-k -L --max-time 300 -A ArtCraftStudio-Updater `"$downloadUrl`" -o `"$zipPath`""
        $pinfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
        $pinfo.CreateNoWindow = $true
        $pinfo.UseShellExecute = $false
        $proc = [System.Diagnostics.Process]::Start($pinfo)

        $lastReport = 20
        while (-not $proc.HasExited) {
            Start-Sleep -Milliseconds 350
            if ((Test-Path $zipPath) -and $expectedBytes -gt 0) {
                $curLen = (Get-Item $zipPath).Length
                $pct = [math]::Floor(20 + ($curLen / $expectedBytes) * 55)
                if ($pct -gt $lastReport + 1) {
                    $lastReport = $pct
                    $mbGot = [math]::Round($curLen / 1MB, 1)
                    $mbTot = [math]::Round($expectedBytes / 1MB, 1)
                    Write-Output "PROGRESS:$pct:다운로드 중 ($mbGot MB / $mbTot MB)"
                }
            }
        }
        $proc.WaitForExit()
        if ($proc.ExitCode -eq 0 -and (Test-Path $zipPath) -and (Get-Item $zipPath).Length -gt 1000) {
            $downloadOk = $true
            Write-Output "PROGRESS:75:다운로드 완료"
        }
    } catch {}

    # Method 2: Fallback to .NET WebClient with Certificate bypass
    if (-not $downloadOk) {
        Write-Output "PROGRESS:35:대체 다운로드 채널 연결 중..."
        try {
            $wc = New-Object System.Net.WebClient
            $wc.Headers.Add("User-Agent", "ArtCraftStudio-Updater")
            $wc.DownloadFile($downloadUrl, $zipPath)
            if ((Test-Path $zipPath) -and (Get-Item $zipPath).Length -gt 1000) {
                $downloadOk = $true
                Write-Output "PROGRESS:75:다운로드 완료"
            }
        } catch {
            Write-Error "다운로드 실패: $_"
            exit 3
        }
    }
}

Write-Output "PROGRESS:80:프로그램 패키지 준비 중..."
$extractDir = Join-Path $tempDir "extracted"
New-Item -ItemType Directory -Force $extractDir | Out-Null
if ($zipPath -match '\.exe$') {
    if ($AppId -eq "hop" -or $zipName -match 'setup|install') {
        Write-Output "PROGRESS:82:포터블 무설치 패키지 자동 압축 해제 중..."
        $installProc = Start-Process -FilePath $zipPath -ArgumentList "/S /D=$extractDir" -Wait -PassThru -NoNewWindow
        Start-Sleep -Seconds 1
        if (Test-Path (Join-Path $extractDir "uninstall.exe")) {
            Remove-Item (Join-Path $extractDir "uninstall.exe") -Force -ErrorAction SilentlyContinue
        }
        $realExe = Get-ChildItem -Path $extractDir -Filter "*.exe" | Where-Object { $_.Name -notmatch 'uninst|setup|crash|update' } | Select-Object -First 1
        if ($realExe) {
            Copy-Item $realExe.FullName (Join-Path $extractDir "$AppId.exe") -Force
        }
    } else {
        Copy-Item $zipPath (Join-Path $extractDir "$AppId.exe") -Force
    }
} else {
    Expand-Archive $zipPath -DestinationPath $extractDir -Force
}

# Flatten if single subfolder exists inside zip
$items = Get-ChildItem $extractDir
$sourceDir = $extractDir
if ($items.Count -eq 1 -and $items[0].PSIsContainer) {
    $sourceDir = $items[0].FullName
}

Write-Output "PROGRESS:88:실행 중인 프로세스 확인 및 안전 종료..."
# Ensure target process is closed before replacing files
Get-Process -Name $AppId, "${AppId}-desktop", "hop-desktop", "HOP" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

Write-Output "PROGRESS:92:기존 바이너리 임시 안전 백업 중..."
$bakDir = Join-Path $env:TEMP "ArtCraft_Backup_$AppId"
if (Test-Path $bakDir) { Remove-Item $bakDir -Recurse -Force -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force $bakDir | Out-Null
if (Test-Path $DestDir) {
    Get-ChildItem $DestDir | ForEach-Object {
        Copy-Item $_.FullName -Destination $bakDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Output "PROGRESS:95:최신 프로그램 동일 위치 안전 교체 (In-place)..."
try {
    Get-ChildItem $sourceDir | ForEach-Object {
        Copy-Item $_.FullName -Destination $DestDir -Recurse -Force
    }
    # Update version stamp
    Set-Content (Join-Path $DestDir ".version") $latestTag -NoNewline
    # Clean up backup on success
    Remove-Item $bakDir -Recurse -Force -ErrorAction SilentlyContinue
} catch {
    Write-Output "PROGRESS:95:교체 오류 발생! 이전 버전으로 안전 롤백 중..."
    if (Test-Path $bakDir) {
        Get-ChildItem $bakDir | ForEach-Object {
            Copy-Item $_.FullName -Destination $DestDir -Recurse -Force
        }
    }
    Write-Error "In-place replacement failed, successfully rolled back: $_"
    exit 4
}

# Cleanup temp
Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Output "PROGRESS:100:v$latestTag 교체 완료"
exit 0
