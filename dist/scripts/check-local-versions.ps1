$root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
$appsDir = Join-Path $root "dist\apps"
$versionJsonPath = Join-Path $root "version.json"

$vjsonApps = @{}
if (Test-Path $versionJsonPath) {
    try {
        $vj = Get-Content $versionJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
        foreach ($p in $vj.apps.PSObject.Properties) {
            $vjsonApps[$p.Name] = $p.Value
        }
    } catch {}
}

$results = @()
if (Test-Path $appsDir) {
    Get-ChildItem $appsDir -Directory | ForEach-Object {
        $id = $_.Name
        $dir = $_.FullName

        $verFile = Join-Path $dir ".version"
        $dotVersion = if (Test-Path $verFile) { (Get-Content $verFile -Raw).Trim() } else { "" }

        $vjsonInfo = if ($vjsonApps.ContainsKey($id)) { $vjsonApps[$id] } else { $null }
        $vjsonVer = if ($vjsonInfo) { $vjsonInfo.version } else { "" }

        $exe = Get-ChildItem $dir -Filter "*.exe" | Where-Object { $_.Name -notmatch "cli|uninst|setup|crash" } | Select-Object -First 1
        $exeName = if ($exe) { $exe.Name } else { "none" }
        $exeDate = if ($exe) { $exe.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss") } else { "none" }
        $exeSize = if ($exe) { [math]::Round($exe.Length / 1MB, 2).ToString() + " MB" } else { "0" }

        $cli = Get-ChildItem $dir -Filter "*cli*.exe" | Select-Object -First 1
        $cliDate = if ($cli) { $cli.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss") } else { "" }

        $results += [PSCustomObject]@{
            Id         = $id
            Version    = if ($dotVersion) { $dotVersion } else { $vjsonVer }
            ExeName    = $exeName
            ExeDate    = $exeDate
            ExeSize    = $exeSize
            CliDate    = $cliDate
        }
    }
}

$results | Format-Table -AutoSize
