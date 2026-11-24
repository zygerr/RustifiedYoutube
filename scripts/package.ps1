$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskMetadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $taskRoot 'Cargo.toml') | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read Cargo metadata' }
$taskVersion = ($taskMetadata.packages | Where-Object name -eq 'rustified-youtube-native').version
if (!$taskVersion) { throw 'Application version unavailable' }
$taskStage = Join-Path $taskRoot "target\package\$taskVersion"
$taskDist = Join-Path $taskRoot "dist\$taskVersion"
$taskTools = Join-Path $taskRoot 'target\native-tools'
New-Item -ItemType Directory -Path $taskStage,$taskDist -Force | Out-Null
foreach ($taskBinary in @('RustifiedYoutube.exe', 'rustified-auth.exe')) {
    Copy-Item -LiteralPath (Join-Path $taskRoot "target\release\$taskBinary") -Destination $taskStage
}
Copy-Item -LiteralPath (Join-Path $taskRoot 'README.md'),(Join-Path $taskRoot 'LICENSE'),(Join-Path $taskRoot 'THIRD-PARTY-NOTICES.md') -Destination $taskStage
$taskAllMetadata = & cargo metadata --locked --format-version 1 --manifest-path (Join-Path $taskRoot 'Cargo.toml') | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read dependency licenses' }
$taskLicenses = $taskAllMetadata.packages | Where-Object source | Sort-Object name,version | ForEach-Object { "$($_.name) $($_.version) | $($_.license) | $($_.repository)" }
[System.IO.File]::WriteAllLines((Join-Path $taskStage 'RUST-DEPENDENCIES.txt'), $taskLicenses)
$taskInstaller = Join-Path $taskDist "RustifiedYoutube_${taskVersion}_x64-setup.exe"
$taskNsis = Join-Path $taskTools 'nsis\nsis-3.13\makensis.exe'
& $taskNsis /V2 "/DAPP_VERSION=$taskVersion" "/DSTAGE_DIR=$taskStage" "/DOUTPUT_FILE=$taskInstaller" (Join-Path $taskRoot 'installer\windows.nsi')
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed' }
$taskArchive = Join-Path $taskDist "RustifiedYoutube_${taskVersion}_windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $taskStage '*') -DestinationPath $taskArchive -Force
$taskChecksums = foreach ($taskArtifact in @($taskInstaller, $taskArchive)) {
    $taskHash = (Get-FileHash -LiteralPath $taskArtifact -Algorithm SHA256).Hash.ToLowerInvariant()
    "$taskHash  $(Split-Path -Leaf $taskArtifact)"
}
[System.IO.File]::WriteAllLines((Join-Path $taskDist 'SHA256SUMS.txt'), $taskChecksums)
Get-ChildItem -LiteralPath $taskDist | Select-Object Name,Length
