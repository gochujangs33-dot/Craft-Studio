# Packages dist/ into a single installer (Inno Setup, if installed) or a portable zip.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $root "dist"
$out  = Join-Path $root "release"
New-Item -ItemType Directory -Force $out | Out-Null
if (-not (Test-Path "$dist\ArtCraftStudio.exe")) { & (Join-Path $PSScriptRoot "build-all.ps1") }

$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($iscc) {
    & $iscc "/O$out" (Join-Path $root "installer\ArtCraftStudio.iss")
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
    Write-Host "Installer written to $out"
} else {
    $zip = Join-Path $out "ArtCraftStudio-portable-x64.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    $pkgPy = Join-Path $PSScriptRoot "package.py"
    if (Test-Path $pkgPy) {
        python $pkgPy
    } else {
        tar -a -c -f $zip --exclude="*.log" --exclude="*.old" --exclude="ArtCraftStudio.app" -C $dist .
    }
    Write-Host "Portable zip written to $zip"
}
