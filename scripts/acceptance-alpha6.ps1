param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$WorkspaceRoot,
    [string]$Siegfried,
    [string]$Signature,
    [switch]$AllowKnownMissing,
    [switch]$BuildInstallers
)
$ErrorActionPreference='Stop'
$EvidenceRoot=Join-Path $WorkspaceRoot 'acceptance-evidence'
New-Item -ItemType Directory -Force -Path $EvidenceRoot|Out-Null
& (Join-Path $PSScriptRoot 'quality-alpha6.ps1') -BuildInstallers:$BuildInstallers 2>&1 |
    Tee-Object -FilePath (Join-Path $EvidenceRoot '01-quality.log')
& (Join-Path $PSScriptRoot 'validate-alpha6-real.ps1') -ArchiveRoot $ArchiveRoot `
    -WorkspaceRoot $WorkspaceRoot -Siegfried $Siegfried -Signature $Signature -SkipQuality -AllowKnownMissing:$AllowKnownMissing 2>&1 |
    Tee-Object -FilePath (Join-Path $EvidenceRoot '02-initial-no-change.log')
$Manifest=[ordered]@{
    schema='mailvault.profiler.alpha6.acceptance.v1'
    generatedAt=(Get-Date).ToUniversalTime().ToString('o')
    archiveRootToken=(Get-FileHash (Join-Path $ArchiveRoot 'database\mailvault.sqlite3') -Algorithm SHA256).Hash
    workspace=$WorkspaceRoot
    completed=@('quality','initial','no_change','source_immutability')
    pending=@('real_delta','force_kill_each_stage','soak','rebuild_equivalence')
}
$Manifest|ConvertTo-Json -Depth 6|Set-Content (Join-Path $EvidenceRoot 'acceptance-manifest.json') -Encoding UTF8
$Manifest|ConvertTo-Json -Depth 6
