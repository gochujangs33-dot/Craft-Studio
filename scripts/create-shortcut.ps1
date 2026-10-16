# Creates a Desktop and/or Start Menu shortcut to ArtCraftStudio.exe
param(
    [switch]$Desktop = $true,
    [switch]$StartMenu
)

$root = Split-Path $PSScriptRoot -Parent
$targetExe = Join-Path $root "dist\ArtCraftStudio.exe"
$iconPath  = Join-Path $root "launcher\assets\app.ico"

if (-not (Test-Path $targetExe)) {
    throw "Target executable not found at: $targetExe. Please run build-all.ps1 first."
}

$WshShell = New-Object -ComObject WScript.Shell

if ($Desktop) {
    $desktopPath = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::Desktop)
    $shortcutPath = Join-Path $desktopPath "ArtCraft Studio.lnk"
    $shortcut = $WshShell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $targetExe
    $shortcut.WorkingDirectory = (Split-Path $targetExe -Parent)
    $shortcut.Description = "ArtCraft Studio - 13 Open Source Creative Apps Suite"
    if (Test-Path $iconPath) {
        $shortcut.IconLocation = "$iconPath,0"
    }
    $shortcut.Save()
    Write-Host "[OK] Desktop shortcut created: $shortcutPath"
}

if ($StartMenu) {
    $startPrograms = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::Programs)
    $folder = Join-Path $startPrograms "ArtCraft Studio"
    New-Item -ItemType Directory -Force $folder | Out-Null
    $shortcutPath = Join-Path $folder "ArtCraft Studio.lnk"
    $shortcut = $WshShell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $targetExe
    $shortcut.WorkingDirectory = (Split-Path $targetExe -Parent)
    $shortcut.Description = "ArtCraft Studio - 13 Open Source Creative Apps Suite"
    if (Test-Path $iconPath) {
        $shortcut.IconLocation = "$iconPath,0"
    }
    $shortcut.Save()
    Write-Host "[OK] Start Menu shortcut created: $shortcutPath"
}
