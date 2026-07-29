param(
    [Parameter(Mandatory = $true)]
    [string]$ArchiveRoot,

    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot,

    [string]$Siegfried,
    [string]$Signature,

    [ValidateRange(1, 20)]
    [int]$RetainSuccessfulSnapshots = 2
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
$CliTargetDir = Join-Path $Repo 'target-alpha6-cli-delta'
$CliExecutable = Join-Path $CliTargetDir 'release\mailvault-profiler.exe'
$PreviousCargoTargetDir = $env:CARGO_TARGET_DIR

$ProfilerDb = Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
$SourceDb = Join-Path $ArchiveRoot 'database\mailvault.sqlite3'
$Evidence = Join-Path $WorkspaceRoot 'acceptance-evidence'

foreach ($Required in @($ProfilerDb, $SourceDb)) {
    if (-not (Test-Path -LiteralPath $Required -PathType Leaf)) {
        throw "Required file is missing: $Required"
    }
}

New-Item -ItemType Directory -Force -Path $Evidence | Out-Null

$Nonce = [Guid]::NewGuid().ToString('N')
$Marker = Join-Path $Evidence "alpha6-duplicate-sha-marker-$Nonce.bin"
$MarkerText = @"
MailVault Alpha 6 duplicate SHA acceptance marker
contract=v1
nonce=$Nonce
"@
$MarkerBytes = [System.Text.Encoding]::UTF8.GetBytes(
    $MarkerText.TrimEnd("`r", "`n") + "`n"
)
[System.IO.File]::WriteAllBytes($Marker, $MarkerBytes)

$Sha = (
    Get-FileHash -LiteralPath $Marker -Algorithm SHA256
).Hash.ToLowerInvariant()

$Audit = Join-Path $env:TEMP "mailvault-alpha6-delta-$Nonce.py"
$Python = @'
import json
import sqlite3
import sys

database = sys.argv[1]
sha256 = sys.argv[2]

connection = sqlite3.connect(database)
assert connection.execute("PRAGMA quick_check").fetchone()[0] == "ok"
assert not connection.execute("PRAGMA foreign_key_check").fetchall()

run = connection.execute(
    """
    SELECT id, generation, source_snapshot_id
    FROM profiler_runs
    WHERE state = 'succeeded'
    ORDER BY generation DESC
    LIMIT 1
    """
).fetchone()
assert run is not None, "no succeeded profiler run exists"

snapshot = connection.execute(
    "SELECT source_metrics_json FROM source_snapshots WHERE id = ?",
    (run[2],),
).fetchone()
assert snapshot is not None, "latest run has no source snapshot"

metrics = json.loads(snapshot[0])

content_object = connection.execute(
    """
    SELECT
        COUNT(*),
        COALESCE(MAX(occurrence_count), 0),
        COALESCE(MAX(format_state), 'absent'),
        COALESCE(MAX(availability_state), 'absent'),
        COALESCE(MAX(size_state), 'absent')
    FROM content_objects
    WHERE active = 1
      AND sha256 = ?
    """,
    (sha256,),
).fetchone()

print(json.dumps({
    "run": run[0],
    "generation": run[1],
    "sourceSnapshotId": run[2],
    "metrics": metrics,
    "objectRows": content_object[0],
    "occurrences": content_object[1],
    "formatState": content_object[2],
    "availability": content_object[3],
    "sizeState": content_object[4],
}))
'@

[System.IO.File]::WriteAllText(
    $Audit,
    $Python.TrimEnd("`r", "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)

function Restore-CargoTargetDirectory {
    if ($null -eq $PreviousCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $PreviousCargoTargetDir
    }
}

function Read-State {
    $Output = & python $Audit $ProfilerDb $Sha
    if ($LASTEXITCODE -ne 0) {
        throw "Delta state audit failed with exit code $LASTEXITCODE."
    }

    return ($Output | ConvertFrom-Json)
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
        [object]$After,

        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    $BeforeJson = $Before | ConvertTo-Json -Depth 6 -Compress
    $AfterJson = $After | ConvertTo-Json -Depth 6 -Compress

    if ($BeforeJson -ne $AfterJson) {
        throw "The profiler changed MailVault database/WAL state during '$Label'."
    }
}

function Invoke-Profile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    $CliArgs = @(
        'profile',
        '--archive', $ArchiveRoot,
        '--workspace', $WorkspaceRoot,
        '--retain-successful-snapshots', $RetainSuccessfulSnapshots.ToString()
    )

    if ($Siegfried) {
        $CliArgs += @('--siegfried', $Siegfried)
    }

    if ($Signature) {
        $CliArgs += @('--signature', $Signature)
    }

    $StdoutLog = Join-Path $Evidence "alpha6-delta-$Nonce-$Label.stdout.log"
    $StderrLog = Join-Path $Evidence "alpha6-delta-$Nonce-$Label.stderr.log"

    Remove-Item `
        -LiteralPath $StdoutLog, $StderrLog `
        -Force `
        -ErrorAction SilentlyContinue

    $Process = Start-Process `
        -FilePath $CliExecutable `
        -ArgumentList $CliArgs `
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
Delta profile '$Label' failed with exit code $($Process.ExitCode).
STDOUT: $StdoutLog
STDERR: $StderrLog
STDERR tail:
$StderrTail
"@
    }

    return [pscustomobject]@{
        StdoutLog = $StdoutLog
        StderrLog = $StderrLog
    }
}

Push-Location $Repo
try {
    # The desktop and CLI packages both produce mailvault-profiler.exe in the
    # default Cargo target directory. Build and run the CLI in an isolated target.
    $env:CARGO_TARGET_DIR = $CliTargetDir

    & $Cargo build --locked --release -p mailvault-profiler-cli --bin mailvault-profiler
    if ($LASTEXITCODE -ne 0) {
        throw "Building the isolated Alpha 6 delta CLI failed with exit code $LASTEXITCODE."
    }

    if (-not (Test-Path -LiteralPath $CliExecutable -PathType Leaf)) {
        throw "Isolated CLI executable was not produced: $CliExecutable"
    }

    $Before = Read-State
    if ([int64]$Before.objectRows -ne 0) {
        throw "The generated marker SHA already exists in the active profile projection."
    }

    $BaselineSourceState = Get-SourceState

    Write-Host @"

STEP 1 OF 2
Attach this exact file to ONE new email sent to a mailbox archived by MailVault:

$Marker

SHA-256: $Sha

Then run the normal MailVault IMAP sync. Do not copy the marker into the
MailVault object store manually. After the sync completes, return here.
"@ -ForegroundColor Cyan
    [void](Read-Host 'Press Enter after the first MailVault sync has completed')

    $FirstSourceStateBeforeProfile = Get-SourceState
    if (
        $FirstSourceStateBeforeProfile.Database.SHA256 -eq
        $BaselineSourceState.Database.SHA256
    ) {
        throw "The MailVault database hash did not change after the first requested sync."
    }

    $FirstExecution = Invoke-Profile -Label 'first'
    $FirstSourceStateAfterProfile = Get-SourceState

    Assert-SourceStateUnchanged `
        -Before $FirstSourceStateBeforeProfile `
        -After $FirstSourceStateAfterProfile `
        -Label 'first delta profile'

    $First = Read-State

    if ([int64]$First.generation -le [int64]$Before.generation) {
        throw 'Generation did not advance after the first real delta.'
    }
    if ([int64]$First.objectRows -ne 1) {
        throw 'The marker did not produce exactly one active content object.'
    }
    if ([int64]$First.occurrences -lt 1) {
        throw 'The marker occurrence was not projected.'
    }
    if ($First.availability -ne 'available' -or $First.sizeState -ne 'match') {
        throw 'The marker did not pass file-stat.'
    }
    if ($First.formatState -in @('uninspected', 'tool_error', 'absent')) {
        throw "The marker did not complete exact-format profiling: $($First.formatState)"
    }
    if (
        [int64]$First.metrics.attachmentOccurrences -le
        [int64]$Before.metrics.attachmentOccurrences
    ) {
        throw 'Attachment occurrence metrics did not advance after the first delta.'
    }

    Write-Host @"

STEP 2 OF 2
Send the SAME marker file in a SECOND new email:

$Marker

Run the normal MailVault IMAP sync again. After the sync completes, return here.
"@ -ForegroundColor Cyan
    [void](Read-Host 'Press Enter after the second MailVault sync has completed')

    $SecondSourceStateBeforeProfile = Get-SourceState
    if (
        $SecondSourceStateBeforeProfile.Database.SHA256 -eq
        $FirstSourceStateAfterProfile.Database.SHA256
    ) {
        throw "The MailVault database hash did not change after the second requested sync."
    }

    $SecondExecution = Invoke-Profile -Label 'second'
    $SecondSourceStateAfterProfile = Get-SourceState

    Assert-SourceStateUnchanged `
        -Before $SecondSourceStateBeforeProfile `
        -After $SecondSourceStateAfterProfile `
        -Label 'second delta profile'

    $Second = Read-State

    if ([int64]$Second.generation -le [int64]$First.generation) {
        throw 'Generation did not advance after the duplicate-SHA delta.'
    }
    if ([int64]$Second.objectRows -ne 1) {
        throw 'Duplicate attachment created more than one active content object.'
    }
    if ([int64]$Second.occurrences -le [int64]$First.occurrences) {
        throw 'Duplicate attachment did not add an occurrence.'
    }
    if (
        [int64]$Second.metrics.attachmentOccurrences -le
        [int64]$First.metrics.attachmentOccurrences
    ) {
        throw 'Attachment occurrence metrics did not advance after the duplicate-SHA delta.'
    }
    if ($Second.availability -ne 'available' -or $Second.sizeState -ne 'match') {
        throw 'The duplicate marker no longer passes file-stat.'
    }
    if ($Second.formatState -in @('uninspected', 'tool_error', 'absent')) {
        throw "The duplicate marker has an invalid format state: $($Second.formatState)"
    }

    $Summary = [ordered]@{
        Status                       = 'REAL_DELTA_AND_SHA_REUSE_GREEN'
        CandidateHead                = (git rev-parse HEAD).Trim()
        MarkerPath                   = $Marker
        SHA256                       = $Sha
        BaselineSourceState          = $BaselineSourceState
        FirstSourceState             = $FirstSourceStateAfterProfile
        SecondSourceState            = $SecondSourceStateAfterProfile
        FirstProfilerReadOnly        = $true
        SecondProfilerReadOnly       = $true
        Before                       = $Before
        First                        = $First
        Second                       = $Second
        FirstExecution               = $FirstExecution
        SecondExecution              = $SecondExecution
        RecordedAtUtc                = [DateTime]::UtcNow.ToString('o')
    }

    $SummaryPath = Join-Path $Evidence "alpha6-real-delta-$Nonce.json"
    [System.IO.File]::WriteAllText(
        $SummaryPath,
        ($Summary | ConvertTo-Json -Depth 12) + "`n",
        [System.Text.UTF8Encoding]::new($false)
    )

    $Summary | ConvertTo-Json -Depth 12
    Get-FileHash -LiteralPath $SummaryPath -Algorithm SHA256
} finally {
    Restore-CargoTargetDirectory
    Pop-Location
}
