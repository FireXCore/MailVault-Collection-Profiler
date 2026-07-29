param(
    [Parameter(Mandatory=$true)][string]$ArchiveRoot,
    [Parameter(Mandatory=$true)][string]$ContinuousWorkspace,
    [Parameter(Mandatory=$true)][string]$RebuildWorkspace,
    [string]$Siegfried,
    [string]$Signature
)
$ErrorActionPreference='Stop'
if(Test-Path $RebuildWorkspace){throw 'Rebuild workspace must be new and empty.'}
$Args=@('run','--locked','--release','-p','mailvault-profiler-cli','--bin','mailvault-profiler','--',
    'profile','--archive',$ArchiveRoot,'--workspace',$RebuildWorkspace,'--rebuild')
if($Siegfried){$Args+=@('--siegfried',$Siegfried)}
if($Signature){$Args+=@('--signature',$Signature)}
& cargo @Args
if($LASTEXITCODE -ne 0){throw 'Independent rebuild failed.'}
$ContinuousDb=Join-Path $ContinuousWorkspace 'profiler\profiler.sqlite3'
$RebuildDb=Join-Path $RebuildWorkspace 'profiler\profiler.sqlite3'
$Python=@'
import hashlib,json,sqlite3,sys

def digest(db):
    c=sqlite3.connect(db)
    run=c.execute("select id,source_snapshot_id from profiler_runs where state='succeeded' order by generation desc limit 1").fetchone()
    if not run: raise SystemExit(f'no successful run in {db}')
    run_id,snapshot=run
    queries={
      'messages':("select source_message_id,archive_id,account_id,provider_thread_namespace,provider_thread_value,rfc_message_id,subject_raw,subject_normalized,header_date,raw_path,raw_sha256,raw_size_bytes,parse_defects_json from source_messages where snapshot_id=? order by source_message_id",(snapshot,)),
      'occurrences':("select source_occurrence_id,source_message_id,generation_id,uid,flags_json,labels_json,internal_date,rfc822_size,modseq,selected_for_raw,fetch_status,last_error from source_message_occurrences where snapshot_id=? order by source_occurrence_id",(snapshot,)),
      'participants':("select source_participant_id,source_message_id,role,ordinal,name,address,domain from source_participants where snapshot_id=? order by source_participant_id",(snapshot,)),
      'parts':("select source_part_id,source_message_id,part_path,parent_part_path,role,declared_mime_type,detected_mime_type,content_disposition,content_id,filename_original,filename_safe,charset,transfer_encoding,size_bytes,sha256,blob_path,headers_json,defects_json from source_parts where snapshot_id=? order by source_part_id",(snapshot,)),
      'relations':("select source_relation_id,source_message_id,target_message_id,relation_type,evidence_type,confidence from source_message_relations where snapshot_id=? order by source_relation_id",(snapshot,)),
      'content':("select sha256,expected_size_bytes,source_detected_mime_type,canonical_path_display,active,availability_state,size_state,format_state,primary_puid,primary_format_name,primary_format_version,primary_format_mime_type,format_match_count,extension_checked,extension_mismatch from content_objects where active=1 order by sha256",()),
    }
    out={}
    for name,(sql,args) in queries.items():
        h=hashlib.sha256(); count=0
        for row in c.execute(sql,args):
            h.update(json.dumps(row,ensure_ascii=False,separators=(',',':'),default=str).encode());h.update(b'\n');count+=1
        out[name]={'count':count,'sha256':h.hexdigest()}
    assert c.execute('pragma quick_check').fetchone()[0]=='ok'
    assert not c.execute('pragma foreign_key_check').fetchall()
    return out
left=digest(sys.argv[1]);right=digest(sys.argv[2])
print(json.dumps({'continuous':left,'rebuild':right,'equal':left==right},indent=2))
if left!=right: raise SystemExit('cumulative incremental projection differs from independent rebuild')
'@
$Audit=Join-Path $env:TEMP 'mailvault-alpha6-equivalence.py'
Set-Content $Audit $Python -Encoding UTF8
python $Audit $ContinuousDb $RebuildDb
if($LASTEXITCODE -ne 0){throw 'Rebuild equivalence failed.'}
[pscustomobject]@{Status='REBUILD_EQUIVALENCE_GREEN';Continuous=$ContinuousWorkspace;Rebuild=$RebuildWorkspace}|ConvertTo-Json
