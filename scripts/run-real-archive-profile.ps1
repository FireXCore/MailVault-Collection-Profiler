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
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)

    return $fullPath.TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
}

function Test-PathOverlap {
    param(
        [Parameter(Mandatory)]
        [string] $Left,

        [Parameter(Mandatory)]
        [string] $Right
    )

    $separator = [System.IO.Path]::DirectorySeparatorChar

    $leftWithSeparator = $Left + $separator
    $rightWithSeparator = $Right + $separator

    return (
        $Left.Equals(
            $Right,
            [System.StringComparison]::OrdinalIgnoreCase
        ) -or
        $leftWithSeparator.StartsWith(
            $rightWithSeparator,
            [System.StringComparison]::OrdinalIgnoreCase
        ) -or
        $rightWithSeparator.StartsWith(
            $leftWithSeparator,
            [System.StringComparison]::OrdinalIgnoreCase
        )
    )
}

function ConvertTo-NativeCommandLineArgument {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Value
    )

    if ($Value.Length -eq 0) {
        return '""'
    }

    if ($Value -notmatch '[\s"]') {
        return $Value
    }

    return '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-NativeRedirected {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [Parameter(Mandatory)]
        [string[]] $Arguments,

        [Parameter(Mandatory)]
        [string] $StandardOutputPath,

        [Parameter(Mandatory)]
        [string] $StandardErrorPath
    )

    $outputDirectory = Split-Path -Parent $StandardOutputPath
    $errorDirectory = Split-Path -Parent $StandardErrorPath

    if ($outputDirectory) {
        New-Item `
            -ItemType Directory `
            -Path $outputDirectory `
            -Force |
            Out-Null
    }

    if ($errorDirectory -and $errorDirectory -ne $outputDirectory) {
        New-Item `
            -ItemType Directory `
            -Path $errorDirectory `
            -Force |
            Out-Null
    }

    Remove-Item `
        -LiteralPath $StandardOutputPath `
        -Force `
        -ErrorAction SilentlyContinue

    Remove-Item `
        -LiteralPath $StandardErrorPath `
        -Force `
        -ErrorAction SilentlyContinue

    $commandLine = (
        $Arguments |
        ForEach-Object {
            ConvertTo-NativeCommandLineArgument -Value $_
        }
    ) -join " "

    $process = Start-Process `
        -FilePath $FilePath `
        -ArgumentList $commandLine `
        -WorkingDirectory $repositoryRoot `
        -RedirectStandardOutput $StandardOutputPath `
        -RedirectStandardError $StandardErrorPath `
        -NoNewWindow `
        -Wait `
        -PassThru

    return [int] $process.ExitCode
}

function Write-Utf8JsonFile {
    param(
        [Parameter(Mandatory)]
        [object] $Value,

        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter()]
        [ValidateRange(1, 100)]
        [int] $Depth = 8
    )

    $json = $Value | ConvertTo-Json -Depth $Depth
    $utf8WithoutBom = [System.Text.UTF8Encoding]::new($false)

    [System.IO.File]::WriteAllText(
        $Path,
        $json + [System.Environment]::NewLine,
        $utf8WithoutBom
    )
}

function Add-EvidenceFileToManifest {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Manifest,

        [Parameter(Mandatory)]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }

    $item = Get-Item -LiteralPath $Path
    $digest = Get-FileHash -LiteralPath $Path -Algorithm SHA256

    $Manifest.files += [ordered]@{
        name   = $item.Name
        bytes  = [long] $item.Length
        sha256 = $digest.Hash.ToLowerInvariant()
    }
}

$repositoryRoot = Get-NormalizedDirectoryPath (
    Join-Path $PSScriptRoot ".."
)

$archive = Get-NormalizedDirectoryPath $ArchiveRoot
$workspace = Get-NormalizedDirectoryPath $WorkspaceRoot
$evidenceBase = Get-NormalizedDirectoryPath $EvidenceRoot

$sourceDatabase = Join-Path $archive "database\mailvault.sqlite3"

if (-not (Test-Path -LiteralPath $archive -PathType Container)) {
    throw "MailVault archive directory was not found at: $archive"
}

if (-not (Test-Path -LiteralPath $sourceDatabase -PathType Leaf)) {
    throw "MailVault database was not found at: $sourceDatabase"
}

if (Test-PathOverlap -Left $archive -Right $workspace) {
    throw "Profiler workspace must not overlap the canonical MailVault archive."
}

if (Test-PathOverlap -Left $archive -Right $evidenceBase) {
    throw "Runtime evidence directory must not overlap the canonical MailVault archive."
}

if (Test-PathOverlap -Left $workspace -Right $evidenceBase) {
    throw "Profiler workspace and runtime evidence directory must not overlap."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo was not found. Install the Rust toolchain pinned by rust-toolchain.toml."
}

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ")
$evidenceDirectory = Join-Path $evidenceBase "profile-$timestamp"

New-Item `
    -ItemType Directory `
    -Path $evidenceDirectory `
    -Force |
    Out-Null

New-Item `
    -ItemType Directory `
    -Path $workspace `
    -Force |
    Out-Null

$preflightJson = Join-Path $evidenceDirectory "preflight.json"
$preflightLog = Join-Path $evidenceDirectory "preflight.stderr.log"
$resultJson = Join-Path $evidenceDirectory "profile-result.json"
$progressLog = Join-Path $evidenceDirectory "profile-progress.jsonl"
$manifestPath = Join-Path $evidenceDirectory "run-manifest.json"

$profileStarted = $false
$profileExitCode = $null
$startedAt = $null
$finishedAt = $null
$elapsedMilliseconds = $null

Push-Location $repositoryRoot

try {
    if (-not $SkipBuild) {
        Write-Host "Building the release CLI..."

        & cargo build `
            --release `
            --locked `
            -p mailvault-profiler-cli

        $buildExitCode = $LASTEXITCODE

        if ($buildExitCode -ne 0) {
            throw "Release CLI build failed with exit code $buildExitCode."
        }
    }

    $binary = Join-Path `
        $repositoryRoot `
        "target\release\mailvault-profiler.exe"

    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Release CLI was not found at: $binary"
    }

    Write-Host "Running read-only preflight..."

    $preflightExitCode = Invoke-NativeRedirected `
        -FilePath $binary `
        -Arguments @(
            "preflight",
            "--archive",
            $archive,
            "--json"
        ) `
        -StandardOutputPath $preflightJson `
        -StandardErrorPath $preflightLog

    if ($preflightExitCode -ne 0) {
        throw (
            "Preflight failed with exit code $preflightExitCode. " +
            "Review: $preflightJson and $preflightLog"
        )
    }

    if (
        -not (Test-Path -LiteralPath $preflightJson -PathType Leaf) -or
        (Get-Item -LiteralPath $preflightJson).Length -eq 0
    ) {
        throw "Preflight completed without producing a result: $preflightJson"
    }

    try {
        Get-Content `
            -LiteralPath $preflightJson `
            -Raw |
            ConvertFrom-Json |
            Out-Null
    }
    catch {
        throw "Preflight produced invalid JSON at: $preflightJson. $($_.Exception.Message)"
    }

    Write-Host "Profiling the canonical collection in read-only mode..."

    $startedAt = (Get-Date).ToUniversalTime()
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $profileStarted = $true

    $profileExitCode = Invoke-NativeRedirected `
        -FilePath $binary `
        -Arguments @(
            "profile",
            "--archive",
            $archive,
            "--workspace",
            $workspace,
            "--batch-size",
            $InventoryBatchSize.ToString(
                [System.Globalization.CultureInfo]::InvariantCulture
            ),
            "--file-stat-workers",
            $FileStatWorkers.ToString(
                [System.Globalization.CultureInfo]::InvariantCulture
            ),
            "--file-stat-batch-size",
            $FileStatBatchSize.ToString(
                [System.Globalization.CultureInfo]::InvariantCulture
            )
        ) `
        -StandardOutputPath $resultJson `
        -StandardErrorPath $progressLog

    $stopwatch.Stop()
    $finishedAt = (Get-Date).ToUniversalTime()
    $elapsedMilliseconds = [long] $stopwatch.ElapsedMilliseconds

    if ($profileExitCode -eq 0) {
        if (
            -not (Test-Path -LiteralPath $resultJson -PathType Leaf) -or
            (Get-Item -LiteralPath $resultJson).Length -eq 0
        ) {
            $profileExitCode = 1

            throw (
                "Profiler returned success but did not produce a result file: " +
                $resultJson
            )
        }

        try {
            Get-Content `
                -LiteralPath $resultJson `
                -Raw |
                ConvertFrom-Json |
                Out-Null
        }
        catch {
            $profileExitCode = 1

            throw "Profiler produced invalid JSON at: $resultJson. $($_.Exception.Message)"
        }
    }
}
catch {
    if ($profileStarted -and $null -eq $finishedAt) {
        if ($null -ne $stopwatch -and $stopwatch.IsRunning) {
            $stopwatch.Stop()
            $elapsedMilliseconds = [long] $stopwatch.ElapsedMilliseconds
        }

        $finishedAt = (Get-Date).ToUniversalTime()
    }

    throw
}
finally {
    try {
        if ($null -eq $startedAt) {
            $startedAt = (Get-Date).ToUniversalTime()
        }

        if ($null -eq $finishedAt) {
            $finishedAt = (Get-Date).ToUniversalTime()
        }

        if ($null -eq $elapsedMilliseconds) {
            $elapsedMilliseconds = 0
        }

        $manifestExitCode = if ($null -eq $profileExitCode) {
            -1
        }
        else {
            [int] $profileExitCode
        }

        $manifest = [ordered]@{
            schemaVersion       = 1
            profilerVersion     = "0.1.0-alpha.4"
            startedAt           = $startedAt.ToString("o")
            finishedAt          = $finishedAt.ToString("o")
            elapsedMilliseconds = [long] $elapsedMilliseconds
            exitCode            = $manifestExitCode
            archiveLeaf         = Split-Path -Leaf $archive
            workspaceLeaf       = Split-Path -Leaf $workspace
            inventoryBatchSize  = $InventoryBatchSize
            fileStatWorkers     = $FileStatWorkers
            fileStatBatchSize   = $FileStatBatchSize
            operatingSystem     = [System.Environment]::OSVersion.VersionString
            processorCount      = [System.Environment]::ProcessorCount
            files               = @()
        }

        foreach ($path in @(
            $preflightJson,
            $preflightLog,
            $resultJson,
            $progressLog
        )) {
            Add-EvidenceFileToManifest `
                -Manifest $manifest `
                -Path $path
        }

        Write-Utf8JsonFile `
            -Value $manifest `
            -Path $manifestPath `
            -Depth 8

        Write-Host "Runtime evidence: $evidenceDirectory"
    }
    finally {
        Pop-Location
    }
}

if ($profileExitCode -ne 0) {
    throw (
        "Collection profile failed with exit code $profileExitCode. " +
        "Review the runtime evidence directory: $evidenceDirectory"
    )
}

Write-Host "Collection profile completed successfully."
Write-Host "Result: $resultJson"
Write-Host "Progress: $progressLog"
Write-Host "Manifest: $manifestPath"
