Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-CheckedNative {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string] $FilePath,

        [Parameter()]
        [string[]] $ArgumentList = @()
    )

    & $FilePath @ArgumentList
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        $renderedArguments = $ArgumentList -join " "
        throw "Native command failed with exit code ${exitCode}: ${FilePath} ${renderedArguments}"
    }
}

Invoke-CheckedNative -FilePath "node" -ArgumentList @(
    "scripts/check-release-config.cjs"
)

Invoke-CheckedNative -FilePath "node" -ArgumentList @(
    "scripts/check-docs.cjs"
)

Invoke-CheckedNative -FilePath "cargo" -ArgumentList @(
    "fmt", "--all", "--", "--check"
)

Invoke-CheckedNative -FilePath "cargo" -ArgumentList @(
    "clippy",
    "--workspace",
    "--all-targets",
    "--all-features",
    "--exclude", "mailvault-profiler-desktop",
    "--locked",
    "--",
    "-D", "warnings"
)

Invoke-CheckedNative -FilePath "cargo" -ArgumentList @(
    "test",
    "--workspace",
    "--all-features",
    "--exclude", "mailvault-profiler-desktop",
    "--locked"
)

# On Windows, invoking `npm` from PowerShell resolves to npm.ps1 before npm.cmd.
# The npm PowerShell shim inherits this script's StrictMode and can fail while
# reading invocation metadata that is not present in all host contexts. Calling
# npm.cmd bypasses the shim while preserving npm's native exit code.
$npmExecutable = if ($env:OS -eq "Windows_NT") { "npm.cmd" } else { "npm" }

Invoke-CheckedNative -FilePath $npmExecutable -ArgumentList @("ci")
Invoke-CheckedNative -FilePath $npmExecutable -ArgumentList @("run", "check:rust-syntax")
Invoke-CheckedNative -FilePath $npmExecutable -ArgumentList @("run", "type-check")
Invoke-CheckedNative -FilePath $npmExecutable -ArgumentList @("run", "build")

# The native desktop bundle includes a pinned Siegfried/PRONOM runtime. Acquire and
# verify it before compiling the Tauri context, because bundle resources are part of
# the release contract rather than an optional developer dependency.
if ($env:OS -eq "Windows_NT") {
    # PowerShell scripts report failure through terminating errors. Do not read
    # $LASTEXITCODE here: it is owned by native executables and may be stale.
    & (Join-Path $PSScriptRoot "install-siegfried.ps1")
}

# The core Rust gate intentionally excludes the native Tauri crate so the same
# workspace can be validated on hosts without desktop system libraries. This
# Windows gate runs after the frontend build and must compile every desktop
# target, preventing source packages with an uncompiled Tauri bridge.
Invoke-CheckedNative -FilePath "cargo" -ArgumentList @(
    "clippy",
    "--package", "mailvault-profiler-desktop",
    "--all-targets",
    "--all-features",
    "--locked",
    "--",
    "-D", "warnings"
)
