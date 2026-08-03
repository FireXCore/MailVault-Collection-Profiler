param(
    [Parameter(Mandatory = $true)]
    [string]$ArchiveRoot,

    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot,

    [ValidateSet('source_snapshot', 'metadata_inventory', 'file_stat', 'format_identification')]
    [string]$Stage = 'metadata_inventory',

    [string]$Siegfried,
    [string]$Signature
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
$CliTargetDir = Join-Path $Repo 'target-alpha6-cli-crash'
$CliExecutable = Join-Path $CliTargetDir 'release\mailvault-profiler.exe'
$PreviousCargoTargetDir = $env:CARGO_TARGET_DIR

$Stdout = Join-Path $env:TEMP "mailvault-alpha6-crash-$Stage.out.jsonl"
$Stderr = Join-Path $env:TEMP "mailvault-alpha6-crash-$Stage.err.jsonl"

function Restore-CargoTargetDirectory {
    if ($null -eq $PreviousCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $PreviousCargoTargetDir
    }
}

function Read-LogTail {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [int]$Lines = 80
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return ''
    }

    return (Get-Content -LiteralPath $Path -Tail $Lines -ErrorAction SilentlyContinue) -join "`r`n"
}

Push-Location $Repo
try {
    # The desktop and CLI targets both produce mailvault-profiler.exe in the default
    # Cargo target directory. Build the CLI in an isolated target directory and run
    # that exact executable so a Tauri bundle cannot overwrite the crash harness target.
    $env:CARGO_TARGET_DIR = $CliTargetDir

    & $Cargo build --locked --release -p mailvault-profiler-cli --bin mailvault-profiler
    if ($LASTEXITCODE -ne 0) {
        throw "Building the isolated Alpha 6 CLI failed with exit code $LASTEXITCODE."
    }

    if (-not (Test-Path -LiteralPath $CliExecutable -PathType Leaf)) {
        throw "Isolated CLI executable was not produced: $CliExecutable"
    }

    if (Test-Path -LiteralPath $WorkspaceRoot) {
        Remove-Item -LiteralPath $WorkspaceRoot -Recurse -Force
    }

    Remove-Item $Stdout, $Stderr -Force -ErrorAction SilentlyContinue

    $CliArgs = @(
        'profile',
        '--archive', $ArchiveRoot,
        '--workspace', $WorkspaceRoot,
        '--batch-size', '25',
        '--file-stat-batch-size', '25',
        '--format-batch-size', '25'
    )

    if ($Siegfried) {
        $CliArgs += @('--siegfried', $Siegfried)
    }

    if ($Signature) {
        $CliArgs += @('--signature', $Signature)
    }

    $Process = Start-Process `
        -FilePath $CliExecutable `
        -ArgumentList $CliArgs `
        -RedirectStandardError $Stderr `
        -RedirectStandardOutput $Stdout `
        -PassThru

    $Deadline = (Get-Date).AddMinutes(15)
    $Killed = $false
    $StagePattern = '"stage":"' + $Stage + '"'

    while ((Get-Date) -lt $Deadline) {
        Start-Sleep -Milliseconds 100
        $Process.Refresh()

        $StageObserved =
            (Test-Path -LiteralPath $Stderr -PathType Leaf) -and
            ((Get-Content -LiteralPath $Stderr -Raw -ErrorAction SilentlyContinue) -match $StagePattern)

        if ($StageObserved) {
            Stop-Process -Id $Process.Id -Force
            [void]$Process.WaitForExit(10000)
            $Killed = $true
            break
        }

        if ($Process.HasExited) {
            break
        }
    }

    if (-not $Killed) {
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force
            [void]$Process.WaitForExit(10000)
        }

        $StdoutTail = Read-LogTail -Path $Stdout
        $StderrTail = Read-LogTail -Path $Stderr

        throw @"
Stage $Stage was not observed before process exit/timeout.
CLI executable: $CliExecutable
STDOUT tail:
$StdoutTail
STDERR tail:
$StderrTail
"@
    }

    # Resume by executing the exact same isolated CLI binary, not `cargo run`.
    & $CliExecutable @CliArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Resume after force-kill failed with exit code $LASTEXITCODE."
    }
} finally {
    Restore-CargoTargetDirectory
    Pop-Location
}

$Db = Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
$Python = @'
import json
import sqlite3
import sys

connection = sqlite3.connect(sys.argv[1])
assert connection.execute('pragma quick_check').fetchone()[0] == 'ok'
assert not connection.execute('pragma foreign_key_check').fetchall()
run = connection.execute(
    'select id,state,resume_count from profiler_runs order by generation desc limit 1'
).fetchone()
assert run[1] == 'succeeded' and run[2] > 0
assert connection.execute('select count(*) from profiler_runs').fetchone()[0] == 1
print(json.dumps({'runId': run[0], 'state': run[1], 'resumeCount': run[2]}))
'@

$Audit = Join-Path $env:TEMP 'mailvault-alpha6-crash.py'
[System.IO.File]::WriteAllText(
    $Audit,
    $Python.TrimEnd("`r", "`n") + "`n",
    [System.Text.UTF8Encoding]::new($false)
)

& python $Audit $Db
if ($LASTEXITCODE -ne 0) {
    throw 'Force-kill resume assertions failed.'
}

[pscustomobject]@{
    Status = 'FORCE_KILL_RESUME_GREEN'
    Stage  = $Stage
} | ConvertTo-Json
