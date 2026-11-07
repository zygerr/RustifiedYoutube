$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
Push-Location $taskRoot
try {
    & "$PSScriptRoot\fetch-native-tools.ps1"
    & cargo build --workspace --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
    & "$PSScriptRoot\package.ps1"
} finally { Pop-Location }
