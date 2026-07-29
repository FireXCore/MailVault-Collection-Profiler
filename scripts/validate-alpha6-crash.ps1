param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$WorkspaceRoot,
    [ValidateSet('source_snapshot','metadata_inventory','file_stat','format_identification')]
    [string]$Stage='metadata_inventory',
    [string]$Siegfried,
    [string]$Signature
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$Repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if(Test-Path -LiteralPath $WorkspaceRoot){Remove-Item -LiteralPath $WorkspaceRoot -Recurse -Force}
$Stdout=Join-Path $env:TEMP "mailvault-alpha6-crash-$Stage.out.jsonl"
$Stderr=Join-Path $env:TEMP "mailvault-alpha6-crash-$Stage.err.jsonl"
Remove-Item $Stdout,$Stderr -Force -ErrorAction SilentlyContinue
$Args=@('run','--locked','--release','-p','mailvault-profiler-cli','--bin','mailvault-profiler','--',
    'profile','--archive',$ArchiveRoot,'--workspace',$WorkspaceRoot,'--batch-size','25',
    '--file-stat-batch-size','25','--format-batch-size','25')
if($Siegfried){$Args+=@('--siegfried',$Siegfried)}
if($Signature){$Args+=@('--signature',$Signature)}
Push-Location $Repo
try {
    $Process=Start-Process cargo -ArgumentList $Args -RedirectStandardError $Stderr -RedirectStandardOutput $Stdout -PassThru
    $Deadline=(Get-Date).AddMinutes(15);$Killed=$false
    while((Get-Date)-lt $Deadline){
        Start-Sleep -Milliseconds 200
        $Process.Refresh()
        if($Process.HasExited){break}
        if((Test-Path $Stderr)-and((Get-Content $Stderr -Raw) -match ('"stage":"'+$Stage+'"'))){
            Stop-Process -Id $Process.Id -Force
            $Killed=$true
            break
        }
    }
    if(!$Killed){if(!$Process.HasExited){Stop-Process -Id $Process.Id -Force};throw "Stage $Stage was not observed before process exit/timeout."}
    & cargo @Args
    if($LASTEXITCODE -ne 0){throw 'Resume after force-kill failed.'}
} finally { Pop-Location }
$Db=Join-Path $WorkspaceRoot 'profiler\profiler.sqlite3'
$Python=@'
import sqlite3,sys,json
c=sqlite3.connect(sys.argv[1]);assert c.execute('pragma quick_check').fetchone()[0]=='ok';assert not c.execute('pragma foreign_key_check').fetchall()
r=c.execute('select id,state,resume_count from profiler_runs order by generation desc limit 1').fetchone()
assert r[1]=='succeeded' and r[2]>0
assert c.execute('select count(*) from profiler_runs').fetchone()[0]==1
print(json.dumps({'runId':r[0],'state':r[1],'resumeCount':r[2]}))
'@
$Audit=Join-Path $env:TEMP 'mailvault-alpha6-crash.py';Set-Content $Audit $Python -Encoding UTF8
python $Audit $Db
if($LASTEXITCODE -ne 0){throw 'Force-kill resume assertions failed.'}
[pscustomobject]@{Status='FORCE_KILL_RESUME_GREEN';Stage=$Stage}|ConvertTo-Json
