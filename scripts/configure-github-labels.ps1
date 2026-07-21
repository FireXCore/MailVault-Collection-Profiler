[CmdletBinding()]
param(
    [Parameter()]
    [string] $Repository = "FireXCore/mailvault-collection-profiler"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI (gh) is required. Install it and run: gh auth login"
}

$labels = @(
    @{ Name = "bug"; Color = "d73a4a"; Description = "Confirmed or reported defect" },
    @{ Name = "enhancement"; Color = "a2eeef"; Description = "New or improved capability" },
    @{ Name = "feature"; Color = "0e8a16"; Description = "User-facing feature" },
    @{ Name = "compatibility"; Color = "5319e7"; Description = "MailVault schema or layout compatibility" },
    @{ Name = "security"; Color = "b60205"; Description = "Security-sensitive change; public label only for non-confidential work" },
    @{ Name = "documentation"; Color = "0075ca"; Description = "Documentation and public media" },
    @{ Name = "dependencies"; Color = "0366d6"; Description = "Dependency update" },
    @{ Name = "rust"; Color = "dea584"; Description = "Rust crates and native pipeline" },
    @{ Name = "frontend"; Color = "61dafb"; Description = "React, TypeScript and Vite" },
    @{ Name = "ci"; Color = "1d76db"; Description = "Continuous integration and release automation" },
    @{ Name = "maintenance"; Color = "c5def5"; Description = "Maintenance without user-visible behavior change" },
    @{ Name = "breaking-change"; Color = "e99695"; Description = "Requires explicit migration or compatibility action" },
    @{ Name = "triage"; Color = "fbca04"; Description = "Needs maintainer classification" },
    @{ Name = "skip-changelog"; Color = "ededed"; Description = "Exclude from generated release notes" }
)

foreach ($label in $labels) {
    gh label create $label.Name `
        --repo $Repository `
        --color $label.Color `
        --description $label.Description `
        --force

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to configure label: $($label.Name)"
    }
}

Write-Host "Configured $($labels.Count) labels in $Repository"
