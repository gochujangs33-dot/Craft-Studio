$root = Split-Path $PSScriptRoot -Parent
$uploadDir = Join-Path $root "release\upload_assets"
New-Item -ItemType Directory -Force $uploadDir | Out-Null

$temp = Join-Path $env:TEMP "photocraft_pkg_091"
if (Test-Path $temp) { Remove-Item $temp -Recurse -Force }
Copy-Item (Join-Path $root "dist\apps\photocraft") $temp -Recurse

# Set version to 0.9.1
Set-Content (Join-Path $temp ".version") "0.9.1" -NoNewline

$destZip = Join-Path $uploadDir "photocraft-0.9.1-windows-x64-portable.zip"
if (Test-Path $destZip) { Remove-Item $destZip -Force }

Write-Host "Compressing PhotoCraft v0.9.1 update package..."
Compress-Archive -Path "$temp\*" -DestinationPath $destZip -Force
Remove-Item $temp -Recurse -Force

Write-Host "Created: $destZip"
Get-Item $destZip | Select-Object Name, Length
