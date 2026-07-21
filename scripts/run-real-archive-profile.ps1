[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ArchiveRoot,

    [Parameter(Mandatory)]
    [string] $WorkspaceRoot,

    [Parameter()]
    [string] $EvidenceRoot = ".\runtime-evidence",

    [Parameter()]
    [ValidateRange(0, 64)]
    [int] $FileStatWorkers = 0,

    [Parameter()]
    [ValidateRange(1, 100000)]
    [int] $FileStatBatchSize = 512,

    [Parameter()]
    [ValidateRange(1, 100000)]
    [int] $InventoryBatchSize = 1000,

    [switch] $SkipBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Get-NormalizedDirectoryPath {
    param([Parameter(Mandatory)][string] $Path)

    $full = [System.IO.Path]::GetFullPath($Path)
    return $full.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
}

function Test-PathOverlap {
    param(
        [Parameter(Mandatory)][string] $Left,
        [Parameter(Mandatory)][string] $Right
    )

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $leftWithSeparator = $Left + $separator
    $rightWithSeparator = $Right + $separator
    return $Left.Equals($Right, [System.StringComparison]::OrdinalIgnoreCase) `
        -or $leftWithSeparator.StartsWith($rightWithSeparator, [System.StringComparison]::OrdinalIgnoreCase) `
        -or $rightWithSeparator.StartsWith($leftWithSeparator, [System.StringComparison]::OrdinalIgnoreCase)
}

$repositoryRoot = Get-NormalizedDirectoryPath (Join-Path $PSScriptRoot "..")
$archive = Get-NormalizedDirectoryPath $ArchiveRoot
$workspace = Get-NormalizedDirectoryPath $WorkspaceRoot
$evidenceBase = Get-NormalizedDirectoryPath $EvidenceRoot
$sourceDatabase = Join-Path $archive "database\mailvault.sqlite3"

if (-not (Test-Path -LiteralPath $sourceDatabase -PathType Leaf)) {
    throw "MailVault database was not found at: $sourceDatabase"
}

if (Test-PathOverlap -Left $archive -Right $workspace) {
    throw "Profiler workspace must not overlap the canonical MailVault archive."
}

if (Test-PathOverlap -Left $archive -Right $evidenceBase) {
    throw "Runtime evidence directory must not overlap the canonical MailVault archive."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found. Install the Rust toolchain pinned by rust-toolchain.toml."
}

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ")
$evidenceDirectory = Join-Path $evidenceBase "profile-$timestamp"
New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
New-Item -ItemType Directory -Path $workspace -Force | Out-Null

Push-Location $repositoryRoot
try {
    if (-not $SkipBuild) {
        Write-Host "Building the release CLI..."
        & cargo build --release -p mailvault-profiler-cli
        if ($LASTEXITCODE -ne 0) {
            throw "Release CLI build failed with exit code $LASTEXITCODE."
        }
    }

    $binary = Join-Path $repositoryRoot "target\release\mailvault-profiler.exe"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Release CLI was not found at: $binary"
    }

    $preflightJson = Join-Path $evidenceDirectory "preflight.json"
    $preflightLog = Join-Path $evidenceDirectory "preflight.stderr.log"
    Write-Host "Running read-only preflight..."
    & $binary preflight --archive $archive --json 1> $preflightJson 2> $preflightLog
    if ($LASTEXITCODE -ne 0) {
        throw "Preflight failed. Review $preflightJson and $preflightLog."
    }

    $resultJson = Join-Path $evidenceDirectory "profile-result.json"
    $progressLog = Join-Path $evidenceDirectory "profile-progress.jsonl"
    $startedAt = (Get-Date).ToUniversalTime()
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

    Write-Host "Profiling the canonical collection in read-only mode..."
    & $binary profile `
        --archive $archive `
        --workspace $workspace `
        --batch-size $InventoryBatchSize `
        --file-stat-workers $FileStatWorkers `
        --file-stat-batch-size $FileStatBatchSize `
        1> $resultJson 2> $progressLog
    $profileExitCode = $LASTEXITCODE

    $stopwatch.Stop()
    $finishedAt = (Get-Date).ToUniversalTime()

    $manifest = [ordered]@{
        schemaVersion = 1
        profilerVersion = "0.1.0-alpha.3"
        startedAt = $startedAt.ToString("o")
        finishedAt = $finishedAt.ToString("o")
        elapsedMilliseconds = [long] $stopwatch.ElapsedMilliseconds
        exitCode = $profileExitCode
        archiveLeaf = Split-Path -Leaf $archive
        workspaceLeaf = Split-Path -Leaf $workspace
        inventoryBatchSize = $InventoryBatchSize
        fileStatWorkers = $FileStatWorkers
        fileStatBatchSize = $FileStatBatchSize
        operatingSystem = [System.Environment]::OSVersion.VersionString
        processorCount = [System.Environment]::ProcessorCount
        files = @()
    }

    foreach ($path in @($preflightJson, $preflightLog, $resultJson, $progressLog)) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $item = Get-Item -LiteralPath $path
            $digest = Get-FileHash -LiteralPath $path -Algorithm SHA256
            $manifest.files += [ordered]@{
                name = $item.Name
                bytes = [long] $item.Length
                sha256 = $digest.Hash.ToLowerInvariant()
            }
        }
    }

    $manifestPath = Join-Path $evidenceDirectory "run-manifest.json"
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

    Write-Host "Runtime evidence: $evidenceDirectory"
    if ($profileExitCode -ne 0) {
        throw "Collection profile failed with exit code $profileExitCode. Review the runtime evidence directory."
    }

    Write-Host "Collection profile completed successfully."
}
finally {
    Pop-Location
}
