[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
try {
    [System.Net.ServicePointManager]::ServerCertificateValidationCallback = { $true }
} catch {}

$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
$registryPath = Join-Path $root "registry.json"
$reg = Get-Content $registryPath -Raw -Encoding UTF8 | ConvertFrom-Json

$headers = @{ "User-Agent" = "ArtCraftStudio" }

foreach ($app in $reg.apps) {
    $id = $app.id
    $repoPath = ($app.repo -replace '^https://github.com/', '')
    try {
        $rel = Invoke-RestMethod "https://api.github.com/repos/$repoPath/releases/latest" -Headers $headers -TimeoutSec 6
        Write-Host "[$id] ($repoPath) Latest Release: $($rel.tag_name) | Published: $($rel.published_at)"
        foreach ($a in $rel.assets) {
            Write-Host "   - $($a.name)"
        }
    } catch {
        # Fallback to curl
        $tmp = Join-Path $env:TEMP ("gh_rel_" + [System.Guid]::NewGuid().ToString("N") + ".json")
        & curl.exe -s -k -L -H "User-Agent: ArtCraftStudio" "https://api.github.com/repos/$repoPath/releases/latest" -o $tmp
        if (Test-Path $tmp) {
            $raw = Get-Content $tmp -Raw
            $rel = $raw | ConvertFrom-Json
            Remove-Item $tmp -Force -ErrorAction SilentlyContinue
            if ($rel.tag_name) {
                Write-Host "[$id] ($repoPath) Latest Release (curl): $($rel.tag_name) | Published: $($rel.published_at)"
                foreach ($a in $rel.assets) {
                    Write-Host "   - $($a.name)"
                }
            } else {
                Write-Host "[$id] ($repoPath) No release found: $($rel.message)"
            }
        }
    }
}
