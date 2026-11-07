$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskTools = Join-Path $taskRoot 'target\native-tools'
New-Item -ItemType Directory -Path $taskTools -Force | Out-Null
$taskArchive = Join-Path $taskTools 'nsis-3.13.zip'
$taskHash = 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8'
if (!(Test-Path -LiteralPath $taskArchive) -or (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash -ine $taskHash) {
    Invoke-WebRequest -Uri 'https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip' -OutFile $taskArchive
    if ((Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash -ine $taskHash) { throw 'NSIS checksum mismatch' }
}
if (!(Test-Path -LiteralPath (Join-Path $taskTools 'nsis\nsis-3.13\makensis.exe'))) {
    Expand-Archive -LiteralPath $taskArchive -DestinationPath (Join-Path $taskTools 'nsis') -Force
}
