param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$WorkspaceRoot,
    [string]$Siegfried,
    [string]$Signature
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$Repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ProfilerDb=Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
if(!(Test-Path -LiteralPath $ProfilerDb)){throw 'Run validate-alpha6-real.ps1 first.'}
$Evidence=Join-Path $WorkspaceRoot 'acceptance-evidence'
New-Item -ItemType Directory -Force -Path $Evidence|Out-Null
$Marker=Join-Path $Evidence 'alpha6-duplicate-sha-marker.bin'
$Bytes=[Text.Encoding]::UTF8.GetBytes("MailVault Alpha 6 duplicate SHA acceptance marker`ncontract=v1`n")
[IO.File]::WriteAllBytes($Marker,$Bytes)
$Sha=(Get-FileHash -LiteralPath $Marker -Algorithm SHA256).Hash.ToLowerInvariant()
$Python=@'
import json,sqlite3,sys
c=sqlite3.connect(sys.argv[1]);sha=sys.argv[2]
run=c.execute("select id,generation,source_snapshot_id from profiler_runs where state='succeeded' order by generation desc limit 1").fetchone()
metrics=json.loads(c.execute('select source_metrics_json from source_snapshots where id=?',(run[2],)).fetchone()[0])
obj=c.execute("select count(*),coalesce(max(occurrence_count),0),coalesce(max(format_state),'absent'),coalesce(max(availability_state),'absent'),coalesce(max(size_state),'absent') from content_objects where active=1 and sha256=?",(sha,)).fetchone()
print(json.dumps({'run':run[0],'generation':run[1],'metrics':metrics,'objectRows':obj[0],'occurrences':obj[1],'formatState':obj[2],'availability':obj[3],'sizeState':obj[4]}))
'@
$Audit=Join-Path $env:TEMP 'mailvault-alpha6-delta.py'
Set-Content -LiteralPath $Audit -Value $Python -Encoding UTF8
function Read-State { (python $Audit $ProfilerDb $Sha|ConvertFrom-Json) }
function Invoke-Profile {
    $Args=@('run','--locked','--release','-p','mailvault-profiler-cli','--bin','mailvault-profiler','--',
        'profile','--archive',$ArchiveRoot,'--workspace',$WorkspaceRoot)
    if($Siegfried){$Args+=@('--siegfried',$Siegfried)}
    if($Signature){$Args+=@('--signature',$Signature)}
    Push-Location $Repo
    try { & cargo @Args; if($LASTEXITCODE -ne 0){throw 'Delta profile failed.'} }
    finally { Pop-Location }
}
$Before=Read-State
Write-Host "Attach this exact file to one new email, run MailVault IMAP sync, then press Enter:`n$Marker`nSHA-256: $Sha" -ForegroundColor Cyan
[void](Read-Host)
Invoke-Profile
$First=Read-State
if($First.generation -le $Before.generation){throw 'Generation did not advance after first real delta.'}
if($First.objectRows -ne 1){throw 'The marker did not produce exactly one active content object.'}
if($First.occurrences -lt 1){throw 'The marker occurrence was not projected.'}
if($First.availability -ne 'available' -or $First.sizeState -ne 'match'){throw 'The marker did not pass file-stat.'}
if($First.formatState -eq 'uninspected' -or $First.formatState -eq 'tool_error'){throw 'The marker did not complete exact-format profiling.'}
Write-Host 'Send the same marker file in a second email, sync MailVault again, then press Enter.' -ForegroundColor Cyan
[void](Read-Host)
Invoke-Profile
$Second=Read-State
if($Second.generation -le $First.generation){throw 'Generation did not advance after duplicate-SHA delta.'}
if($Second.objectRows -ne 1){throw 'Duplicate attachment created more than one content object.'}
if($Second.occurrences -le $First.occurrences){throw 'Duplicate attachment did not add an occurrence.'}
[pscustomobject]@{Status='REAL_DELTA_AND_SHA_REUSE_GREEN';SHA256=$Sha;Before=$Before;First=$First;Second=$Second}|ConvertTo-Json -Depth 8
