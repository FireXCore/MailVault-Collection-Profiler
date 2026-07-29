param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$WorkspaceRoot,
    [string]$Siegfried,
    [string]$Signature,
    [switch]$SkipQuality,
    [switch]$AllowKnownMissing
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$Repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$SourceDb=Join-Path $ArchiveRoot 'database\mailvault.sqlite3'
if(!(Test-Path -LiteralPath $SourceDb -PathType Leaf)){throw "MailVault database not found: $SourceDb"}
if(Test-Path -LiteralPath $WorkspaceRoot){throw 'Use a new empty validation workspace.'}
$Before=(Get-FileHash -LiteralPath $SourceDb -Algorithm SHA256).Hash
Push-Location $Repo
try {
    if(!$SkipQuality){& (Join-Path $PSScriptRoot 'quality-alpha6.ps1')}
    $Args=@('run','--locked','--release','-p','mailvault-profiler-cli','--bin','mailvault-profiler','--',
        'profile','--archive',$ArchiveRoot,'--workspace',$WorkspaceRoot,
        '--retain-successful-snapshots','2')
    if($Siegfried){$Args+=@('--siegfried',$Siegfried)}
    if($Signature){$Args+=@('--signature',$Signature)}
    & cargo @Args
    if($LASTEXITCODE -ne 0){throw 'Initial Alpha 6 profile failed.'}
    & cargo @Args
    if($LASTEXITCODE -ne 0){throw 'Immediate no-change Alpha 6 profile failed.'}
} finally { Pop-Location }
$ProfilerDb=Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
$Python=@'
import json,sqlite3,sys
c=sqlite3.connect(sys.argv[1])
assert c.execute('pragma quick_check').fetchone()[0]=='ok'
assert not c.execute('pragma foreign_key_check').fetchall()
runs=c.execute("select id,run_kind,generation,state,resume_count,source_snapshot_id from profiler_runs order by generation").fetchall()
assert len(runs)>=2 and all(row[3]=='succeeded' for row in runs[-2:])
first,latest=runs[-2],runs[-1]
assert latest[2]==first[2]+1
success_reprocessed=c.execute("select count(*) from file_stat_observations where run_id=? and availability_state='available' and size_state='match'",(latest[0],)).fetchone()[0]
format_row=c.execute("select total_objects,identified,unknown,ambiguous,empty_objects,skipped_unavailable,tool_errors,state from format_identification_runs where baseline_run_id=? order by started_at desc limit 1",(latest[0],)).fetchone()
assert format_row is not None and format_row[7]=='succeeded'
assert success_reprocessed==0, f'unchanged successful file-stat objects were reprocessed: {success_reprocessed}'
assert sum(format_row[1:5])==0, f'unchanged successful format objects were reidentified: {format_row}'
missing=c.execute("select count(*) from content_objects where active=1 and availability_state<>'available'").fetchone()[0]
snapshots=c.execute('select count(*) from source_snapshots').fetchone()[0]
assert snapshots<=2
print(json.dumps({'runs':runs[-2:],'successfulFileStatReprocessed':success_reprocessed,'latestFormatWork':format_row,'activeUnavailable':missing,'retainedSnapshots':snapshots},indent=2))
if missing and sys.argv[2]!='1': raise SystemExit('active unavailable objects remain; resolve them or rerun with -AllowKnownMissing after documenting the exception')
'@
$Audit=Join-Path $env:TEMP 'mailvault-alpha6-real.py'
Set-Content -LiteralPath $Audit -Value $Python -Encoding UTF8
python $Audit $ProfilerDb ([int]$AllowKnownMissing)
if($LASTEXITCODE -ne 0){throw 'Initial/no-change database assertions failed.'}
$After=(Get-FileHash -LiteralPath $SourceDb -Algorithm SHA256).Hash
if($Before -ne $After){throw 'Profiler modified canonical MailVault database.'}
[pscustomobject]@{Status='INITIAL_NO_CHANGE_GREEN';MailVaultSHA256=$After;Workspace=$WorkspaceRoot}|ConvertTo-Json
