param(
    [Parameter(Mandatory = $true)]
    [string]$ArchiveRoot,

    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot,

    [ValidateRange(1, 1000)]
    [int]$Cycles = 100,

    [string]$Siegfried,
    [string]$Signature,

    [ValidateRange(1, 20)]
    [int]$RetainSuccessfulSnapshots = 2
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
$CliTargetDir = Join-Path $Repo 'target-alpha6-cli-soak'
$CliExecutable = Join-Path $CliTargetDir 'release\mailvault-profiler.exe'
$PreviousCargoTargetDir = $env:CARGO_TARGET_DIR

$SourceDb = Join-Path $ArchiveRoot 'database\mailvault.sqlite3'

if (-not (Test-Path -LiteralPath $SourceDb -PathType Leaf)) {
    throw "MailVault database was not found: $SourceDb"
}

if (Test-Path -LiteralPath $WorkspaceRoot) {
    throw "Use a new soak workspace. Existing path: $WorkspaceRoot"
}

$EvidenceRoot = Join-Path $WorkspaceRoot 'acceptance-evidence'
New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null

$RunNonce = [Guid]::NewGuid().ToString('N')

function Restore-CargoTargetDirectory {
    if ($null -eq $PreviousCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $PreviousCargoTargetDir
    }
}

function Get-OptionalFileState {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    $Item = Get-Item -LiteralPath $Path
    return [ordered]@{
        Path             = $Item.FullName
        Bytes            = $Item.Length
        LastWriteTimeUtc = $Item.LastWriteTimeUtc.ToString('o')
        SHA256           = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    }
}

function Get-SourceState {
    return [ordered]@{
        Database = Get-OptionalFileState -Path $SourceDb
        Wal      = Get-OptionalFileState -Path ($SourceDb + '-wal')
    }
}

function Assert-SourceStateUnchanged {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Before,

        [Parameter(Mandatory = $true)]
        [object]$After
    )

    $BeforeJson = $Before | ConvertTo-Json -Depth 6 -Compress
    $AfterJson = $After | ConvertTo-Json -Depth 6 -Compress

    if ($BeforeJson -ne $AfterJson) {
        throw 'MailVault database/WAL state changed during the soak acceptance.'
    }
}

function Invoke-SoakCycle {
    param(
        [Parameter(Mandatory = $true)]
        [int]$Cycle
    )

    $CycleText = $Cycle.ToString('D3')
    $StdoutLog = Join-Path $EvidenceRoot "alpha6-soak-$RunNonce-cycle-$CycleText.stdout.log"
    $StderrLog = Join-Path $EvidenceRoot "alpha6-soak-$RunNonce-cycle-$CycleText.stderr.log"

    $Arguments = @(
        'profile',
        '--archive', $ArchiveRoot,
        '--workspace', $WorkspaceRoot,
        '--retain-successful-snapshots', $RetainSuccessfulSnapshots.ToString()
    )

    if ($Siegfried) {
        $Arguments += @('--siegfried', $Siegfried)
    }

    if ($Signature) {
        $Arguments += @('--signature', $Signature)
    }

    Write-Host "Alpha 6 soak cycle $Cycle / $Cycles" -ForegroundColor Cyan

    $Process = Start-Process `
        -FilePath $CliExecutable `
        -ArgumentList $Arguments `
        -RedirectStandardOutput $StdoutLog `
        -RedirectStandardError $StderrLog `
        -PassThru `
        -Wait

    if ($Process.ExitCode -ne 0) {
        $StderrTail = ''
        if (Test-Path -LiteralPath $StderrLog -PathType Leaf) {
            $StderrTail = (
                Get-Content -LiteralPath $StderrLog -Tail 100 -ErrorAction SilentlyContinue
            ) -join "`r`n"
        }

        throw @"
Soak cycle $Cycle failed with exit code $($Process.ExitCode).
STDOUT: $StdoutLog
STDERR: $StderrLog
STDERR tail:
$StderrTail
"@
    }
}

Push-Location $Repo
try {
    # The desktop and CLI packages both produce mailvault-profiler.exe in the
    # default Cargo target directory. Build and run the CLI in an isolated target.
    $env:CARGO_TARGET_DIR = $CliTargetDir

    & $Cargo build --locked --release -p mailvault-profiler-cli --bin mailvault-profiler
    if ($LASTEXITCODE -ne 0) {
        throw "Building the isolated Alpha 6 soak CLI failed with exit code $LASTEXITCODE."
    }

    if (-not (Test-Path -LiteralPath $CliExecutable -PathType Leaf)) {
        throw "Isolated CLI executable was not produced: $CliExecutable"
    }

    $SourceStateBefore = Get-SourceState

    for ($Cycle = 1; $Cycle -le $Cycles; $Cycle++) {
        Invoke-SoakCycle -Cycle $Cycle
    }

    $SourceStateAfter = Get-SourceState
    Assert-SourceStateUnchanged -Before $SourceStateBefore -After $SourceStateAfter
} finally {
    Restore-CargoTargetDirectory
    Pop-Location
}

$ProfilerDb = Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
if (-not (Test-Path -LiteralPath $ProfilerDb -PathType Leaf)) {
    throw "Profiler database was not produced: $ProfilerDb"
}

$Audit = Join-Path $env:TEMP "mailvault-alpha6-soak-$RunNonce.py"
$Python = @'
import json
import os
import sqlite3
import sys

database = sys.argv[1]
expected_cycles = int(sys.argv[2])
snapshot_limit = int(sys.argv[3])

connection = sqlite3.connect(database)
assert connection.execute("PRAGMA quick_check").fetchone()[0] == "ok"
assert not connection.execute("PRAGMA foreign_key_check").fetchall()

runs = connection.execute(
    """
    SELECT id, run_kind, generation, state, resume_count
    FROM profiler_runs
    ORDER BY generation
    """
).fetchall()

assert len(runs) == expected_cycles, (
    f"expected {expected_cycles} runs, found {len(runs)}"
)
assert all(row[3] == "succeeded" for row in runs), "not all runs succeeded"
assert runs[0][1] == "initial", f"first run was {runs[0][1]}"
assert all(row[1] == "incremental" for row in runs[1:]), (
    "non-initial soak runs must be incremental"
)
assert [row[2] for row in runs] == list(range(1, expected_cycles + 1)), (
    "run generations are not contiguous"
)

snapshots = connection.execute(
    "SELECT COUNT(*) FROM source_snapshots"
).fetchone()[0]
assert snapshots <= snapshot_limit, (
    f"snapshot retention exceeded: {snapshots} > {snapshot_limit}"
)

live_runs = connection.execute(
    """
    SELECT COUNT(*)
    FROM profiler_runs
    WHERE state NOT IN ('succeeded', 'failed', 'cancelled')
    """
).fetchone()[0]
assert live_runs == 0, f"live runs remain: {live_runs}"

orphan_file_stats = connection.execute(
    """
    SELECT COUNT(*)
    FROM file_stat_observations observation
    LEFT JOIN profiler_runs run ON run.id = observation.run_id
    WHERE run.id IS NULL
    """
).fetchone()[0]
assert orphan_file_stats == 0, (
    f"orphan file-stat observations remain: {orphan_file_stats}"
)

successful_file_stat_reprocessed = connection.execute(
    """
    SELECT COUNT(*)
    FROM file_stat_observations observation
    JOIN profiler_runs run ON run.id = observation.run_id
    WHERE run.generation > 1
      AND observation.availability_state = 'available'
      AND observation.size_state = 'match'
    """
).fetchone()[0]
assert successful_file_stat_reprocessed == 0, (
    "unchanged successful file-stat objects were reprocessed: "
    f"{successful_file_stat_reprocessed}"
)

format_rows = connection.execute(
    """
    SELECT
        run.generation,
        format_run.state,
        format_run.identified,
        format_run.unknown,
        format_run.ambiguous,
        format_run.empty_objects,
        format_run.tool_errors
    FROM format_identification_runs format_run
    JOIN profiler_runs run ON run.id = format_run.baseline_run_id
    WHERE run.generation > 1
    ORDER BY run.generation
    """
).fetchall()

assert len(format_rows) == max(expected_cycles - 1, 0), (
    f"expected {expected_cycles - 1} incremental format runs, "
    f"found {len(format_rows)}"
)
assert all(row[1] == "succeeded" for row in format_rows), (
    "an incremental format run did not succeed"
)

reidentified = sum(sum(row[2:6]) for row in format_rows)
tool_errors = sum(row[6] for row in format_rows)

assert reidentified == 0, (
    f"unchanged successful format objects were reidentified: {reidentified}"
)
assert tool_errors == 0, (
    f"incremental format tool errors were recorded: {tool_errors}"
)

result = {
    "runs": len(runs),
    "firstGeneration": runs[0][2],
    "lastGeneration": runs[-1][2],
    "snapshots": snapshots,
    "snapshotLimit": snapshot_limit,
    "liveRuns": live_runs,
    "orphanFileStatObservations": orphan_file_stats,
    "successfulFileStatReprocessed": successful_file_stat_reprocessed,
    "incrementalFormatRuns": len(format_rows),
    "successfulFormatsReidentified": reidentified,
    "formatToolErrors": tool_errors,
    "databaseBytes": os.path.getsize(database),
}

print(json.dumps(result, indent=2))
'@

[System.IO.File]::WriteAllText(
    $Audit,
    $Python.TrimEnd("`r", "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)

$AuditOutput = @(
    & python $Audit $ProfilerDb $Cycles $RetainSuccessfulSnapshots
)

if ($LASTEXITCODE -ne 0) {
    throw 'Soak integrity assertions failed.'
}

$AuditText = $AuditOutput -join "`r`n"
$AuditText | Write-Host
$AuditResult = $AuditText | ConvertFrom-Json

$CandidateHead = ''
Push-Location $Repo
try {
    $CandidateHead = (git rev-parse HEAD).Trim()
} finally {
    Pop-Location
}

$Summary = [ordered]@{
    Status            = 'SOAK_GREEN'
    CandidateHead     = $CandidateHead
    Cycles            = $Cycles
    Workspace         = $WorkspaceRoot
    SourceStateBefore = $SourceStateBefore
    SourceStateAfter  = $SourceStateAfter
    Audit             = $AuditResult
    RecordedAtUtc     = [DateTime]::UtcNow.ToString('o')
}

$SummaryPath = Join-Path $EvidenceRoot "alpha6-soak-$RunNonce-summary.json"
[System.IO.File]::WriteAllText(
    $SummaryPath,
    ($Summary | ConvertTo-Json -Depth 12) + "`n",
    [System.Text.UTF8Encoding]::new($false)
)

$Summary | ConvertTo-Json -Depth 12
Get-FileHash -LiteralPath $SummaryPath -Algorithm SHA256
