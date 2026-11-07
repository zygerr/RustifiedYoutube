$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskTools = Join-Path $taskRoot 'target\native-tools'
New-Item -ItemType Directory -Path $taskTools -Force | Out-Null
function Get-VerifiedTool([string]$Url, [string]$Destination, [string]$Sha256) {
    if (Test-Path -LiteralPath $Destination) {
        if ((Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -ieq $Sha256) { return }
    }
    Invoke-WebRequest -Uri $Url -OutFile $Destination
    if ((Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -ine $Sha256) { throw "Checksum mismatch: $Destination" }
}
Get-VerifiedTool 'https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp.exe' (Join-Path $taskTools 'yt-dlp.exe') 'f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d'
$taskNodeVersion = '24.18.0'
$taskNodeArchive = "node-v$taskNodeVersion-win-x64.zip"
$taskNodeHash = '0ae68406b42d7725661da979b1403ec9926da205c6770827f33aac9d8f26e821'
$taskNodeZip = Join-Path $taskTools $taskNodeArchive
Get-VerifiedTool "https://nodejs.org/dist/v$taskNodeVersion/$taskNodeArchive" $taskNodeZip $taskNodeHash
if (!(Test-Path -LiteralPath (Join-Path $taskTools 'node.exe'))) {
    Expand-Archive -LiteralPath $taskNodeZip -DestinationPath $taskTools -Force
    Copy-Item -LiteralPath (Join-Path $taskTools "node-v$taskNodeVersion-win-x64\node.exe") -Destination (Join-Path $taskTools 'node.exe')
    Copy-Item -LiteralPath (Join-Path $taskTools "node-v$taskNodeVersion-win-x64\LICENSE") -Destination (Join-Path $taskTools 'NODE-LICENSE.txt')
}
foreach ($taskLicense in @('LICENSE', 'THIRD_PARTY_LICENSES.txt')) {
    $taskLicensePath = Join-Path $taskTools "YT-DLP-$taskLicense.txt"
    if (!(Test-Path -LiteralPath $taskLicensePath)) { Invoke-WebRequest -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/2026.10.26/$taskLicense" -OutFile $taskLicensePath }
}
$taskFfmpegZip = Join-Path $taskTools 'ffmpeg-2026-11-06-win64.zip'
Get-VerifiedTool 'https://github.com/yt-dlp/FFmpeg-Builds/releases/download/autobuild-2026-11-06-20-26/ffmpeg-N-128544-ga6c7695959-win64-gpl.zip' $taskFfmpegZip 'ec968f08c5f771fe230b8926295f18cf083779daaa32979c36e26110027d20a2'
if (!(Test-Path -LiteralPath (Join-Path $taskTools 'ffmpeg.exe'))) {
    $taskZip = [System.IO.Compression.ZipFile]::OpenRead($taskFfmpegZip)
    try {
        foreach ($taskEntry in $taskZip.Entries) {
            if ($taskEntry.FullName.EndsWith('/bin/ffmpeg.exe')) {
                [System.IO.Compression.ZipFileExtensions]::ExtractToFile($taskEntry, (Join-Path $taskTools 'ffmpeg.exe'), $true)
            }
            if ($taskEntry.Name -match '^(COPYING|LICENSE)(\..*)?$') {
                [System.IO.Compression.ZipFileExtensions]::ExtractToFile($taskEntry, (Join-Path $taskTools "FFMPEG-$($taskEntry.Name).txt"), $true)
            }
        }
    } finally { $taskZip.Dispose() }
    if (!(Test-Path -LiteralPath (Join-Path $taskTools 'ffmpeg.exe'))) { throw 'FFmpeg executable missing from archive' }
}
$taskNsisZip = Join-Path $taskTools 'nsis-3.13.zip'
Get-VerifiedTool 'https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip' $taskNsisZip 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8'
if (!(Test-Path -LiteralPath (Join-Path $taskTools 'nsis\nsis-3.13\makensis.exe'))) { Expand-Archive -LiteralPath $taskNsisZip -DestinationPath (Join-Path $taskTools 'nsis') -Force }
Get-ChildItem -LiteralPath $taskTools -File | Select-Object Name,Length
