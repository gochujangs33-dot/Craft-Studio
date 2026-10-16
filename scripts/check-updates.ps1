# Checks for latest updates for all apps from GitHub Releases and outputs JSON
param(
    [string]$RegistryPath
)

$ErrorActionPreference = "SilentlyContinue"
$ProgressPreference = "SilentlyContinue"

$root = Split-Path $PSScriptRoot -Parent
if (-not $RegistryPath) {
    $RegistryPath = Join-Path $root "registry.json"
}

$registry = Get-Content $RegistryPath -Raw | ConvertFrom-Json
$headers = @{ "User-Agent" = "ArtCraftStudio-Updater" }

$results = @{}

foreach ($app in $registry.apps) {
    $id = $app.id
    $repoPath = ($app.repo -replace '^https://github.com/', '')
    try {
        $rel = Invoke-RestMethod "https://api.github.com/repos/$repoPath/releases/latest" -Headers $headers -TimeoutSec 5
        if ($rel.tag_name) {
            $tag = $rel.tag_name -replace '^v', ''
            $results[$id] = @{
                "latest_version" = $tag
                "release_url"    = $rel.html_url
                "published_at"   = $rel.published_at
            }
        }
    } catch {
        # ignore timeout / rate limit
    }
}

$results | ConvertTo-Json -Compress
