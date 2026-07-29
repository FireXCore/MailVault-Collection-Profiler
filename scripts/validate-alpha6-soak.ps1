param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$WorkspaceRoot,
    [int]$Cycles=100,
    [string]$Siegfried,
    [string]$Signature
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if($Cycles -lt 1){throw 'Cycles must be positive.'}
$Repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$SourceDb=Join-Path $ArchiveRoot 'database\mailvault.sqlite3'
$Before=(Get-FileHash -LiteralPath $SourceDb -Algorithm SHA256).Hash
$Args=@('run','--locked','--release','-p','mailvault-profiler-cli','--bin','mailvault-profiler','--',
    'profile','--archive',$ArchiveRoot,'--workspace',$WorkspaceRoot,'--retain-successful-snapshots','2')
if($Siegfried){$Args+=@('--siegfried',$Siegfried)}
if($Signature){$Args+=@('--signature',$Signature)}
Push-Location $Repo
try { for($Cycle=1;$Cycle -le $Cycles;$Cycle++){& cargo @Args;if($LASTEXITCODE -ne 0){throw "Soak cycle $Cycle failed."}} }
finally { Pop-Location }
$ProfilerDb=Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
$Python=@'
import json,os,sqlite3,sys
c=sqlite3.connect(sys.argv[1]);assert c.execute('pragma quick_check').fetchone()[0]=='ok';assert not c.execute('pragma foreign_key_check').fetchall()
snapshots=c.execute('select count(*) from source_snapshots').fetchone()[0]
assert snapshots<=2
live=c.execute("select count(*) from profiler_runs where state not in ('succeeded','failed','cancelled')").fetchone()[0]
assert live==0
orphans=c.execute("select count(*) from file_stat_observations o left join profiler_runs r on r.id=o.run_id where r.id is null").fetchone()[0]
assert orphans==0
print(json.dumps({'runs':c.execute('select count(*) from profiler_runs').fetchone()[0],'snapshots':snapshots,'databaseBytes':os.path.getsize(sys.argv[1])}))
'@
$Audit=Join-Path $env:TEMP 'mailvault-alpha6-soak.py';Set-Content $Audit $Python -Encoding UTF8
python $Audit $ProfilerDb
if($LASTEXITCODE -ne 0){throw 'Soak integrity assertions failed.'}
$After=(Get-FileHash -LiteralPath $SourceDb -Algorithm SHA256).Hash
if($Before -ne $After){throw 'Source hash changed during soak.'}
[pscustomobject]@{Status='SOAK_GREEN';Cycles=$Cycles;MailVaultSHA256=$After}|ConvertTo-Json
