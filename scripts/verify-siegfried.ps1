#Requires -Version 5.1


[CmdletBinding()]
param(
    # Deliberately resolved after the param block so Windows PowerShell 5.1

    # never has to evaluate a default expression that depends on $PSScriptRoot.

    [Parameter()]
    [string] $Directory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$scriptPath = [string]$PSCommandPath
if ([string]::IsNullOrWhiteSpace($scriptPath)) {
    $scriptPath = [string]$MyInvocation.MyCommand.Path
}
if ([string]::IsNullOrWhiteSpace($scriptPath)) {
    throw "Unable to resolve the current verification script path."
}

$scriptRoot = [System.IO.Path]::GetDirectoryName(
    [System.IO.Path]::GetFullPath($scriptPath)
)
if ([string]::IsNullOrWhiteSpace($scriptRoot)) {
    throw "Unable to resolve the directory containing verify-siegfried.ps1."
}

$repositoryRoot = [System.IO.Path]::GetFullPath(
    (Join-Path $scriptRoot "..")
)

if ([string]::IsNullOrWhiteSpace($Directory)) {
    $Directory = Join-Path $repositoryRoot "tools\siegfried\windows-x86_64"
}

try {
    $root = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Directory)
}
catch {
    throw "Unable to resolve Siegfried directory '$Directory': $($_.Exception.Message)"
}
$root = [System.IO.Path]::GetFullPath($root)

function Get-Sha256 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Cannot hash missing file: $Path"
    }

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Read-JsonFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Path
    )

    try {
        $jsonText = [System.IO.File]::ReadAllText($Path)
        if ([string]::IsNullOrWhiteSpace($jsonText)) {
            throw "JSON file is empty."
        }

        # ConvertFrom-Json -Depth is not available in Windows PowerShell 5.1.

        return ($jsonText | ConvertFrom-Json)
    }
    catch {
        throw "Unable to parse JSON file '$Path': $($_.Exception.Message)"
    }
}

function ConvertTo-NativeCommandLineArgument {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Value
    )

    # ProcessStartInfo.ArgumentList is unavailable on the .NET Framework used
    # by Windows PowerShell 5.1. Quote arguments according to the Windows
    # CommandLineToArgvW rules so paths containing spaces, quotes, or trailing
    # backslashes reach the native executable unchanged.
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }

    $builder = [System.Text.StringBuilder]::new()
    [void] $builder.Append([char]34)
    $backslashCount = 0

    foreach ($character in $Value.ToCharArray()) {
        $codePoint = [int] $character

        if ($codePoint -eq 92) {
            $backslashCount++
            continue
        }

        if ($codePoint -eq 34) {
            if ($backslashCount -gt 0) {
                [void] $builder.Append([char]92, (($backslashCount * 2) + 1))
            }
            else {
                [void] $builder.Append([char]92)
            }

            [void] $builder.Append([char]34)
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void] $builder.Append([char]92, $backslashCount)
            $backslashCount = 0
        }

        [void] $builder.Append($character)
    }

    # Backslashes immediately before the closing quote must be doubled.
    if ($backslashCount -gt 0) {
        [void] $builder.Append([char]92, ($backslashCount * 2))
    }

    [void] $builder.Append([char]34)
    return $builder.ToString()
}

function Invoke-NativeProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Executable,

        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [string[]] $Arguments,

        [Parameter()]
        [ValidateRange(1, 3600)]
        [int] $TimeoutSeconds = 300
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Native executable was not found: $Executable"
    }

    $quotedArguments = @(
        foreach ($argument in $Arguments) {
            ConvertTo-NativeCommandLineArgument -Value ([string] $argument)
        }
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.Arguments = $quotedArguments -join " "
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo

    try {
        if (-not $process.Start()) {
            throw "Failed to start native executable: $Executable"
        }

        # Start both asynchronous reads before waiting. Reading either stream
        # synchronously first can deadlock when the child fills the other pipe.
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()

        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            try {
                $process.Kill()
            }
            catch {
                # Preserve the timeout as the primary failure.
            }

            $process.WaitForExit()
            throw "Native command timed out after $TimeoutSeconds seconds: $Executable $($startInfo.Arguments)"
        }

        # WaitForExit() without a timeout ensures async stream handlers have
        # finished after the process handle is signaled.
        $process.WaitForExit()

        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()

        return [pscustomobject]@{
            ExitCode = [int] $process.ExitCode
            StdOut = [string] $stdout
            StdErr = [string] $stderr
            CommandLine = "$Executable $($startInfo.Arguments)"
        }
    }
    finally {
        $process.Dispose()
    }
}

$manifestPath = Join-Path $root "tool-manifest.json"
$sfPath = Join-Path $root "sf.exe"
$signaturePath = Join-Path $root "default.sig"

foreach ($requiredPath in @($manifestPath, $sfPath, $signaturePath)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Pinned Siegfried resource is missing: $requiredPath"
    }
}

$manifest = Read-JsonFile -Path $manifestPath

if ([int]$manifest.schemaVersion -ne 1) {
    throw "Unsupported tool-manifest.json schema version '$($manifest.schemaVersion)'."
}

$manifestToolVersion = [string]$manifest.tool.version
$manifestToolHash = ([string]$manifest.tool.sha256).ToLowerInvariant()
$manifestSignatureVersion = [string]$manifest.signature.version
$manifestSignatureHash = ([string]$manifest.signature.sha256).ToLowerInvariant()

if ([string]::IsNullOrWhiteSpace($manifestToolVersion) -or
    $manifestToolHash -notmatch '^[0-9a-f]{64}$' -or
    [string]::IsNullOrWhiteSpace($manifestSignatureVersion) -or
    $manifestSignatureHash -notmatch '^[0-9a-f]{64}$') {
    throw "tool-manifest.json is missing required version or SHA-256 fields."
}

$sfHash = Get-Sha256 -Path $sfPath
$sigHash = Get-Sha256 -Path $signaturePath

if ($sfHash -ne $manifestToolHash) {
    throw "sf.exe SHA-256 does not match tool-manifest.json. Expected $manifestToolHash, observed $sfHash."
}
if ($sigHash -ne $manifestSignatureHash) {
    throw "default.sig SHA-256 does not match tool-manifest.json. Expected $manifestSignatureHash, observed $sigHash."
}

$probePath = Join-Path $root (
    ".verify-probe-" + [guid]::NewGuid().ToString("N") + ".txt"
)

$probeEncoding = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText(
    $probePath,
    "MailVault Collection Profiler Siegfried verification probe.",
    $probeEncoding
)

try {
    $signatureArgument = [System.IO.Path]::GetFileName($signaturePath)
    if ([string]::IsNullOrWhiteSpace($signatureArgument)) {
        throw "Unable to derive a home-relative Siegfried signature filename from '$signaturePath'."
    }

    # Keep the JSON header portable and valid on Windows. Siegfried 1.11.6
    # writes the header's signature value without escaping backslashes.
    $result = Invoke-NativeProcess `
        -Executable $sfPath `
        -Arguments @(
            "-home", $root,
            "-sig", $signatureArgument,
            "-json",
            "-utc",
            $probePath
        ) `
        -TimeoutSeconds 120

    if ($result.ExitCode -ne 0) {
        $diagnostic = (
            $result.StdOut +
            [Environment]::NewLine +
            $result.StdErr
        ).Trim()

        throw "Siegfried verification probe failed with exit code $($result.ExitCode): $diagnostic"
    }

    $jsonText = $result.StdOut.Trim()
    if ([string]::IsNullOrWhiteSpace($jsonText)) {
        throw "Siegfried verification probe returned empty JSON. stderr: $($result.StdErr.Trim())"
    }

    try {
        $probe = $jsonText | ConvertFrom-Json
    }
    catch {
        throw "Siegfried verification probe returned invalid JSON: $($_.Exception.Message). Raw output: $jsonText"
    }

    $runtimeToolVersion = [string] $probe.siegfried
    if ($runtimeToolVersion -ne $manifestToolVersion) {
        throw "Runtime Siegfried version '$runtimeToolVersion' differs from manifest version '$manifestToolVersion'."
    }

    $requiredPronomNumber = $manifestSignatureVersion.TrimStart([char[]] "vV")
    if ([string]::IsNullOrWhiteSpace($requiredPronomNumber)) {
        throw "Manifest PRONOM version '$manifestSignatureVersion' is invalid."
    }

    $identifierDetails = @(
        $probe.identifiers | ForEach-Object { [string] $_.details }
    )

    $expectedPattern = "DROID_SignatureFile_V$([regex]::Escape($requiredPronomNumber))\.xml"
    $matched = @(
        $identifierDetails | Where-Object { $_ -match $expectedPattern }
    ).Count -gt 0

    if (-not $matched) {
        throw "Runtime PRONOM signature differs from tool-manifest.json. Required '$manifestSignatureVersion'; identifier details were: $($identifierDetails -join '; ')"
    }

    if (-not [string]::IsNullOrWhiteSpace($result.StdErr)) {
        Write-Verbose "Siegfried verification diagnostics: $($result.StdErr.Trim())"
    }
}
finally {
    Remove-Item -LiteralPath $probePath -Force -ErrorAction SilentlyContinue
}

Write-Host "Siegfried resources verified:"
Write-Host "  Siegfried: $manifestToolVersion"
Write-Host "  PRONOM:    $manifestSignatureVersion"
Write-Host "  Directory: $root"
