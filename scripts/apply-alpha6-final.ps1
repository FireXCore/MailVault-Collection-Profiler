param(
    [Parameter(Mandatory=$true)][string]$RepositoryRoot,
    [Parameter(Mandatory=$true)][string]$SourceZip,
    [string]$Branch='feat/alpha6-final-continuous-profile'
)
$ErrorActionPreference='Stop'
$RepositoryRoot=(Resolve-Path $RepositoryRoot).Path
if((git -C $RepositoryRoot status --short)){throw 'Repository must be clean before applying Alpha 6.'}
$Temp=Join-Path $env:TEMP ('mailvault-alpha6-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Temp|Out-Null
try{
    Expand-Archive -LiteralPath $SourceZip -DestinationPath $Temp -Force
    $Source=Get-ChildItem $Temp -Directory|Select-Object -First 1
    if(!$Source){throw 'Source ZIP has no root directory.'}
    git -C $RepositoryRoot switch -c $Branch
    robocopy $Source.FullName $RepositoryRoot /MIR /XD .git target node_modules tools /XF '*.sqlite3' '*.log'
    if($LASTEXITCODE -ge 8){throw "robocopy failed: $LASTEXITCODE"}
    Push-Location $RepositoryRoot
    try{
        cargo fmt --all
        cargo fmt --all -- --check
        git diff --check
        node scripts/check-tag-version.cjs v0.1.0-alpha.6
        node scripts/check-docs.cjs
    }finally{Pop-Location}
    Write-Host 'Alpha 6 source applied. Run scripts\quality-alpha6.ps1 next.' -ForegroundColor Green
}finally{Remove-Item $Temp -Recurse -Force -ErrorAction SilentlyContinue}
