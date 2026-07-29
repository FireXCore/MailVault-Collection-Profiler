param([switch]$BuildInstallers)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$Root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $Root
try {
    & (Join-Path $PSScriptRoot 'quality.ps1')
    node scripts/check-tag-version.cjs v0.1.0-alpha.6
    if($LASTEXITCODE -ne 0){throw 'Alpha 6 tag/version gate failed.'}
    if($BuildInstallers){
        $Npm=if($env:OS -eq 'Windows_NT'){'npm.cmd'}else{'npm'}
        & $Npm run tauri:desktop:bundle
        if($LASTEXITCODE -ne 0){throw 'Tauri installer build failed.'}
    }
} finally { Pop-Location }
