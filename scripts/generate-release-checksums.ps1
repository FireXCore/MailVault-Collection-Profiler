[CmdletBinding()]
param(
    [Parameter()]
    [string] $BundleRoot = "target/release/bundle",

    [Parameter()]
    [string] $OutputPath = "SHA256SUMS.txt"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Test-Path $BundleRoot)) {
    throw "Bundle root does not exist: $BundleRoot"
}

$artifacts = Get-ChildItem $BundleRoot -Recurse -File |
    Where-Object { $_.Extension -in @(".exe", ".msi") } |
    Sort-Object Name

if (-not $artifacts) {
    throw "No NSIS or MSI artifacts found under $BundleRoot"
}

$lines = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash $artifact.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($artifact.Name)"
}

$lines | Set-Content -Path $OutputPath -Encoding ascii
Write-Host "Wrote $($artifacts.Count) checksums to $OutputPath"
Get-Content $OutputPath
