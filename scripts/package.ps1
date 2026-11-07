$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskManifest = Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json
$taskDist = Join-Path $taskRoot 'dist'
New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
$taskVersion = $taskManifest.version
$taskInstaller = Join-Path $taskRoot "target\release\bundle\nsis\RustifiedYoutube_${taskVersion}_x64-setup.exe"
$taskExecutable = Join-Path $taskRoot 'target\release\rustified-youtube.exe'
if (!(Test-Path -LiteralPath $taskInstaller)) { throw "Missing installer: $taskInstaller" }
if (!(Test-Path -LiteralPath $taskExecutable)) { throw "Missing executable: $taskExecutable" }
Copy-Item -LiteralPath $taskInstaller -Destination $taskDist
$taskArchive = Join-Path $taskDist "RustifiedYoutube_${taskVersion}_windows-x64-portable.zip"
Compress-Archive -LiteralPath $taskExecutable -DestinationPath $taskArchive -Force
$taskArtifacts = @((Join-Path $taskDist (Split-Path -Leaf $taskInstaller)), $taskArchive)
$taskChecksums = foreach ($taskArtifact in $taskArtifacts) {
    $taskHash = (Get-FileHash -LiteralPath $taskArtifact -Algorithm SHA256).Hash.ToLowerInvariant()
    "$taskHash  $(Split-Path -Leaf $taskArtifact)"
}
[System.IO.File]::WriteAllLines((Join-Path $taskDist 'SHA256SUMS.txt'), $taskChecksums)
Get-ChildItem -LiteralPath $taskDist | Select-Object Name,Length
