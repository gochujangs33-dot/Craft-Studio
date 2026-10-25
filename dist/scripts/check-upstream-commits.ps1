param(
    [switch]$Detailed
)

$ErrorActionPreference = "SilentlyContinue"
$ProgressPreference = "SilentlyContinue"

[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
try {
    [System.Net.ServicePointManager]::ServerCertificateValidationCallback = { $true }
} catch {}

$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
$registryPath = Join-Path $root "registry.json"
if (-not (Test-Path $registryPath)) {
    $registryPath = Join-Path (Get-Location) "registry.json"
}
$versionJsonPath = Join-Path $root "version.json"
$cacheFile = Join-Path $root ".upstream_status.json"
$appsDir = Join-Path $root "dist\apps"

# 1. Load local version.json
$localVersions = @{}
if (Test-Path $versionJsonPath) {
    try {
        $vj = Get-Content $versionJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($p in $vj.apps.PSObject.Properties) {
            $localVersions[$p.Name] = $p.Value.version
        }
    } catch {}
}

# 2. Load apps list from registry.json
$apps = @()
if (Test-Path $registryPath) {
    $reg = Get-Content $registryPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $apps = $reg.apps
}

$headers = @{
    "User-Agent" = "ArtCraftStudio-CommitChecker"
    "Accept"     = "application/vnd.github.v3+json"
}
if ($env:GITHUB_TOKEN) {
    $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
}

Write-Host "=================================================================================" -ForegroundColor Cyan
Write-Host "  ArtCraft Studio: Local Build vs Upstream Source Code Comparison" -ForegroundColor Cyan
Write-Host "=================================================================================" -ForegroundColor Cyan

$results = @()
$newHistory = @{}

foreach ($app in $apps) {
    $id = $app.id
    $repoUrl = $app.repo
    $repoPath = ($repoUrl -replace '^https://github.com/', '')

    # Get local app details
    $localAppDir = Join-Path $appsDir $id
    $verFile = Join-Path $localAppDir ".version"
    $localVer = if (Test-Path $verFile) { (Get-Content $verFile -Raw).Trim() } elseif ($localVersions.ContainsKey($id)) { $localVersions[$id] } else { "unknown" }
    
    $exe = if (Test-Path $localAppDir) {
        Get-ChildItem $localAppDir -Filter "*.exe" | Where-Object { $_.Name -notmatch "cli|uninst|setup|crash" } | Select-Object -First 1
    } else { $null }
    $localDate = if ($exe) { $exe.LastWriteTime.ToString("yyyy-MM-dd HH:mm") } else { "none" }

    # Fetch upstream commits
    $apiUrl = "https://api.github.com/repos/$repoPath/commits?per_page=3"
    $commits = $null
    try {
        $commits = Invoke-RestMethod -Uri $apiUrl -Headers $headers -TimeoutSec 10
    } catch {
        $tmp = Join-Path $env:TEMP ("gh_c_" + [System.Guid]::NewGuid().ToString("N") + ".json")
        & curl.exe -s -k -L -H "User-Agent: ArtCraftStudio" $apiUrl -o $tmp
        if (Test-Path $tmp) {
            $raw = Get-Content $tmp -Raw
            $commits = $raw | ConvertFrom-Json
            Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        }
    }

    if ($commits -and $commits.Count -gt 0) {
        $latest = $commits[0]
        $sha = $latest.sha.Substring(0, 7)
        $dateRaw = $latest.commit.committer.date
        $upDateObj = [DateTime]::Parse($dateRaw).ToLocalTime()
        $upDateStr = $upDateObj.ToString("yyyy-MM-dd HH:mm")
        $author = $latest.commit.author.name
        $msgLines = $latest.commit.message -split "`n"
        $title = $msgLines[0].Trim()

        # Compare URL: shows exact git diff from local version to current upstream HEAD
        $compareTag = if ($localVer.StartsWith("v")) { $localVer } else { "v$localVer" }
        $compareUrl = "https://github.com/$repoPath/compare/$compareTag...main"

        $newHistory[$id] = @{
            localVer   = $localVer
            localDate  = $localDate
            upSha      = $sha
            upDate     = $upDateStr
            title      = $title
            compareUrl = $compareUrl
        }

        $results += [PSCustomObject]@{
            Id         = $id
            Name       = $app.prefix + "Craft"
            LocalVer   = $localVer
            LocalDate  = $localDate
            UpDate     = $upDateStr
            UpSha      = $sha
            Author     = $author
            Title      = $title
            CompareUrl = $compareUrl
            Commits    = $commits
        }
    }
}

# Display Results
foreach ($item in $results) {
    Write-Host ""
    Write-Host "[$($item.Id)] $($item.Name)" -ForegroundColor White -NoNewline
    Write-Host "  (Local: v$($item.LocalVer) [$($item.LocalDate)] -> Upstream: [$($item.UpDate)])" -ForegroundColor Yellow
    Write-Host "  * Latest Upstream Commit : [$($item.UpSha)] $($item.Title) ($($item.Author))" -ForegroundColor Cyan
    Write-Host "  * Full Diff URL          : $($item.CompareUrl)" -ForegroundColor Green

    if ($Detailed) {
        Write-Host "  * Recent Commits on Upstream:" -ForegroundColor DarkGray
        foreach ($c in $item.Commits) {
            $cDate = [DateTime]::Parse($c.commit.committer.date).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
            $cMsg = ($c.commit.message -split "`n")[0].Trim()
            $cSha = $c.sha.Substring(0, 7)
            Write-Host "     - [$cDate] ($cSha) $cMsg" -ForegroundColor DarkGray
        }
    }
}

$newHistory | ConvertTo-Json -Depth 5 | Set-Content -Path $cacheFile

Write-Host ""
Write-Host "=================================================================================" -ForegroundColor Cyan
Write-Host "  Comparison complete for $($results.Count) apps. Saved snapshot to .upstream_status.json" -ForegroundColor Green
Write-Host "=================================================================================" -ForegroundColor Cyan
