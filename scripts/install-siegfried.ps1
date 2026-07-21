#Requires -Version 5.1


[CmdletBinding()]
param(
    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string] $Tag = "v1.11.6",

    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string] $RequiredVersion = "1.11.6",

    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string] $RequiredPronomVersion = "v124",

    # Deliberately has no default expression that references $PSScriptRoot.

    # Windows PowerShell can evaluate parameter defaults before $PSScriptRoot

    # is reliably available.

    [Parameter()]
    [string] $Destination,

    [Parameter()]
    [switch] $Force,

    [Parameter()]
    [switch] $ValidateEnvironmentOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

Write-Host "Starting pinned Siegfried installer..."

# Resolve the running script path after the param block. $PSCommandPath describes

# the current script; $MyInvocation.PSScriptRoot describes the caller and must

# not be used as a replacement.

$scriptPath = [string]$PSCommandPath
if ([string]::IsNullOrWhiteSpace($scriptPath)) {
    $scriptPath = [string]$MyInvocation.MyCommand.Path
}
if ([string]::IsNullOrWhiteSpace($scriptPath)) {
    throw "Unable to resolve the current script path. Run this file with PowerShell -File or invoke it as a .ps1 script."
}

$scriptRoot = [System.IO.Path]::GetDirectoryName([System.IO.Path]::GetFullPath($scriptPath))
if ([string]::IsNullOrWhiteSpace($scriptRoot)) {
    throw "Unable to resolve the directory containing install-siegfried.ps1."
}

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot ".."))

if ([string]::IsNullOrWhiteSpace($Destination)) {
    $Destination = Join-Path $repositoryRoot "tools\siegfried\windows-x86_64"
}

# Resolve relative PowerShell paths without requiring the destination to exist.

try {
    $destinationPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination)
}
catch {
    throw "Unable to resolve destination path '$Destination': $($_.Exception.Message)"
}
$destinationPath = [System.IO.Path]::GetFullPath($destinationPath)

$destinationInfo = [System.IO.DirectoryInfo]::new($destinationPath)
$destinationParent = if ($null -ne $destinationInfo.Parent) { $destinationInfo.Parent.FullName } else { $null }
$destinationLeaf = $destinationInfo.Name
if ([string]::IsNullOrWhiteSpace($destinationParent) -or
    [string]::IsNullOrWhiteSpace($destinationLeaf)) {
    throw "Unsafe destination path '$destinationPath'. A non-root directory is required."
}

$verifyScriptPath = Join-Path $scriptRoot "verify-siegfried.ps1"
if (-not (Test-Path -LiteralPath $verifyScriptPath -PathType Leaf)) {
    throw "Required verification script was not found: $verifyScriptPath"
}

if ($ValidateEnvironmentOnly) {
    Write-Host "Siegfried installer environment validation passed:"
    Write-Host "  PowerShell:  $($PSVersionTable.PSVersion)"
    Write-Host "  Script root: $scriptRoot"
    Write-Host "  Repository:  $repositoryRoot"
    Write-Host "  Destination: $destinationPath"
    Write-Host "  Verifier:    $verifyScriptPath"
    return
}

# GitHub requires modern TLS. This is especially important for Windows

# PowerShell 5.1 on older .NET Framework configurations.

try {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor
        [Net.SecurityProtocolType]::Tls12
}
catch {
    throw "Unable to enable TLS 1.2 for GitHub downloads: $($_.Exception.Message)"
}

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

function Write-Utf8NoBom {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Content
    )

    # Set-Content -Encoding utf8NoBOM is not available in Windows PowerShell

    # 5.1, so use the .NET encoder explicitly.

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Invoke-GitHubDownload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Uri,

        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $OutFile,

        [Parameter(Mandatory = $true)]
        [hashtable] $Headers
    )

    $parameters = @{
        Uri = $Uri
        Headers = $Headers
        OutFile = $OutFile
        ErrorAction = "Stop"
    }

    # Windows PowerShell 5.1 may otherwise depend on the legacy Internet

    # Explorer parsing engine.

    if ($PSVersionTable.PSVersion.Major -le 5) {
        $parameters.UseBasicParsing = $true
    }

    Invoke-WebRequest @parameters
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

function Invoke-JsonProbe {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Executable,

        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Signature,

        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        # Do not name this parameter `Home`: PowerShell variable names are
        # case-insensitive, and `$HOME` is a read-only automatic variable.
        [string] $ToolHome
    )

    $probePath = Join-Path $ToolHome (
        "probe-" + [guid]::NewGuid().ToString("N") + ".txt"
    )

    # A zero-byte file causes Siegfried to emit a normal file diagnostic on
    # stderr. Windows PowerShell 5.1 promotes native stderr to its Error stream,
    # which previously aborted the installer under ErrorActionPreference=Stop.
    # Use a deterministic non-empty text object and capture native streams via
    # System.Diagnostics.Process instead of PowerShell redirection.
    $probeEncoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText(
        $probePath,
        "MailVault Collection Profiler Siegfried verification probe.",
        $probeEncoding
    )

    $signatureFullPath = [System.IO.Path]::GetFullPath($Signature)
    $toolHomeFullPath = [System.IO.Path]::GetFullPath($ToolHome)
    $signatureDirectory = [System.IO.Path]::GetDirectoryName($signatureFullPath)
    $signatureArgument = [System.IO.Path]::GetFileName($signatureFullPath)

    if ([string]::IsNullOrWhiteSpace($signatureDirectory) -or
        [string]::IsNullOrWhiteSpace($signatureArgument) -or
        -not [string]::Equals(
            $signatureDirectory.TrimEnd([char[]]"\/"),
            $toolHomeFullPath.TrimEnd([char[]]"\/"),
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        throw "Siegfried probe signature must be a direct child of the configured tool home. Signature: '$signatureFullPath'; home: '$toolHomeFullPath'."
    }

    try {
        # Siegfried 1.11.6's JSON writer escapes scanned filenames but writes
        # the JSON header's signature value verbatim. Passing an absolute
        # Windows path therefore produces invalid JSON (e.g. "E:\tools\...").
        # Resolve the file through -home and pass only its basename.
        $result = Invoke-NativeProcess `
            -Executable $Executable `
            -Arguments @(
                "-home", $toolHomeFullPath,
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

            throw "Siegfried probe failed with exit code $($result.ExitCode): $diagnostic"
        }

        $jsonText = $result.StdOut.Trim()
        if ([string]::IsNullOrWhiteSpace($jsonText)) {
            throw "Siegfried probe returned an empty JSON response. stderr: $($result.StdErr.Trim())"
        }

        try {
            # ConvertFrom-Json -Depth is unavailable in Windows PowerShell 5.1.
            $probe = $jsonText | ConvertFrom-Json
        }
        catch {
            throw "Siegfried probe returned invalid JSON: $($_.Exception.Message). Raw output: $jsonText"
        }

        if ($null -eq $probe -or [string]::IsNullOrWhiteSpace([string] $probe.siegfried)) {
            throw "Siegfried probe JSON did not contain the expected header fields."
        }

        if (-not [string]::IsNullOrWhiteSpace($result.StdErr)) {
            Write-Verbose "Siegfried probe diagnostics: $($result.StdErr.Trim())"
        }

        return $probe
    }
    finally {
        Remove-Item -LiteralPath $probePath -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-SignatureUpdate {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Executable,

        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        # Do not name this parameter `Home`: PowerShell variable names are
        # case-insensitive, and `$HOME` is a read-only automatic variable.
        [string] $ToolHome
    )

    $result = Invoke-NativeProcess `
        -Executable $Executable `
        -Arguments @("-home", $ToolHome, "-update") `
        -TimeoutSeconds 300

    if ($result.ExitCode -ne 0) {
        $diagnostic = (
            $result.StdOut +
            [Environment]::NewLine +
            $result.StdErr
        ).Trim()

        throw "Siegfried signature update failed with exit code $($result.ExitCode): $diagnostic"
    }

    if (-not [string]::IsNullOrWhiteSpace($result.StdErr)) {
        Write-Verbose "Siegfried signature update diagnostics: $($result.StdErr.Trim())"
    }

    return $result.StdOut.Trim()
}

function Invoke-InstalledToolVerification {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateNotNullOrEmpty()]
        [string] $Directory
    )

    # This is a PowerShell script, not a native executable. $LASTEXITCODE can

    # contain a stale value from an earlier native process, so verification

    # must rely on normal PowerShell error propagation instead.

    & $verifyScriptPath -Directory $Directory
}

$manifestPath = Join-Path $destinationPath "tool-manifest.json"
$sfPath = Join-Path $destinationPath "sf.exe"
$signaturePath = Join-Path $destinationPath "default.sig"

if (-not $Force -and
    (Test-Path -LiteralPath $manifestPath -PathType Leaf) -and
    (Test-Path -LiteralPath $sfPath -PathType Leaf) -and
    (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {

    try {
        Invoke-InstalledToolVerification -Directory $destinationPath
    }
    catch {
        throw "An existing Siegfried installation was found but failed verification. Re-run with -Force to replace it. Verification error: $($_.Exception.Message)"
    }

    Write-Host "Pinned Siegfried toolchain is already installed and verified:"
    Write-Host "  $destinationPath"
    return
}

New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null

$tempRoot = Join-Path (
    [System.IO.Path]::GetTempPath()
) ("mailvault-profiler-siegfried-download-" + [guid]::NewGuid().ToString("N"))

$stagingPath = Join-Path $destinationParent (
    ".$destinationLeaf.install-" + [guid]::NewGuid().ToString("N")
)

$backupPath = Join-Path $destinationParent (
    ".$destinationLeaf.backup-" + [guid]::NewGuid().ToString("N")
)

New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
New-Item -ItemType Directory -Path $stagingPath -Force | Out-Null

$installed = $false
$destinationBackedUp = $false

try {
    $headers = @{
        Accept = "application/vnd.github+json"
        "X-GitHub-Api-Version" = "2022-11-28"
        "User-Agent" = "FireXCore-MailVault-Collection-Profiler"
    }

    if (-not [string]::IsNullOrWhiteSpace([string]$env:GITHUB_TOKEN)) {
        $headers.Authorization = "Bearer $($env:GITHUB_TOKEN)"
    }

    $escapedTag = [Uri]::EscapeDataString($Tag)
    $releaseUri = "https://api.github.com/repos/richardlehane/siegfried/releases/tags/$escapedTag"

    Write-Host "Resolving Siegfried release $Tag from GitHub..."
    $release = Invoke-RestMethod `
        -Uri $releaseUri `
        -Headers $headers `
        -Method Get `
        -ErrorAction Stop

    if ([string]$release.tag_name -ne $Tag) {
        throw "GitHub release tag mismatch: requested '$Tag', received '$($release.tag_name)'."
    }
    if ([bool]$release.draft) {
        throw "Refusing to install from draft GitHub release '$Tag'."
    }

    # Pin the non-static Windows x64 archive by its exact upstream name.
    # Siegfried also publishes a *_win64_static.zip build whose signatures are
    # embedded in the executable. The profiler deliberately uses the regular
    # build because it treats sf.exe and default.sig as separately hashed,
    # versioned supply-chain artifacts.
    $releaseVersionSlug = $RequiredVersion -replace '\.', '-'
    $expectedAssetName = "siegfried_${releaseVersionSlug}_win64.zip"

    $assets = @(
        $release.assets | Where-Object {
            [string]$_.name -ieq $expectedAssetName
        }
    )

    if ($assets.Count -ne 1) {
        $available = @(
            $release.assets |
                ForEach-Object { [string]$_.name } |
                Sort-Object
        ) -join ", "

        throw "Expected exactly one pinned Siegfried release asset '$expectedAssetName' for '$Tag'; found $($assets.Count). Available assets: $available"
    }

    $asset = $assets[0]
    Write-Host "Selected pinned release asset:"
    Write-Host "  $expectedAssetName"
    $assetName = [string]$asset.name
    $assetDigest = [string]$asset.digest
    $assetDownloadUrl = [string]$asset.browser_download_url
    $assetSize = [int64]$asset.size

    if ([string]::IsNullOrWhiteSpace($assetName) -or
        [string]::IsNullOrWhiteSpace($assetDownloadUrl) -or
        $assetSize -le 0) {
        throw "GitHub returned incomplete metadata for the selected release asset."
    }

    if ([string]::IsNullOrWhiteSpace($assetDigest) -or
        $assetDigest -notmatch '^sha256:[0-9a-fA-F]{64}$') {
        throw "GitHub did not provide a valid SHA-256 digest for release asset '$assetName'; refusing an unverifiable download."
    }

    $archivePath = Join-Path $tempRoot $assetName

    Write-Host "Downloading verified release asset:"
    Write-Host "  $assetName"
    Invoke-GitHubDownload `
        -Uri $assetDownloadUrl `
        -OutFile $archivePath `
        -Headers $headers

    $downloadedSize = (Get-Item -LiteralPath $archivePath).Length
    if ($downloadedSize -ne $assetSize) {
        throw "Downloaded asset size mismatch: expected $assetSize bytes, got $downloadedSize bytes."
    }

    $expectedAssetSha = $assetDigest.Substring(7).ToLowerInvariant()
    $actualAssetSha = Get-Sha256 -Path $archivePath
    if ($actualAssetSha -ne $expectedAssetSha) {
        throw "Downloaded asset SHA-256 mismatch: expected $expectedAssetSha, got $actualAssetSha."
    }

    Write-Host "Release asset SHA-256 verified."

    $expandedPath = Join-Path $tempRoot "expanded"
    New-Item -ItemType Directory -Path $expandedPath -Force | Out-Null
    Expand-Archive -LiteralPath $archivePath -DestinationPath $expandedPath -Force

    $sourceExecutables = @(
        Get-ChildItem -LiteralPath $expandedPath -Recurse -File -Filter "sf.exe"
    )
    if ($sourceExecutables.Count -ne 1) {
        throw "The verified release asset must contain exactly one sf.exe; found $($sourceExecutables.Count)."
    }

    $stagedSfPath = Join-Path $stagingPath "sf.exe"
    $stagedSignaturePath = Join-Path $stagingPath "default.sig"
    $stagedManifestPath = Join-Path $stagingPath "tool-manifest.json"

    Copy-Item `
        -LiteralPath $sourceExecutables[0].FullName `
        -Destination $stagedSfPath `
        -Force

    $sourceSignatures = @(
        Get-ChildItem -LiteralPath $expandedPath -Recurse -File -Filter "default.sig"
    )

    if ($sourceSignatures.Count -gt 1) {
        throw "The verified release asset contained multiple default.sig files; refusing an ambiguous installation."
    }

    if ($sourceSignatures.Count -eq 1) {
        Copy-Item `
            -LiteralPath $sourceSignatures[0].FullName `
            -Destination $stagedSignaturePath `
            -Force
        $signatureSource = "verified-release-asset"
    }
    else {
        Write-Host "The release archive did not contain default.sig; invoking Siegfried's verified signature updater..."
        $null = Invoke-SignatureUpdate `
            -Executable $stagedSfPath `
            -ToolHome $stagingPath

        if (-not (Test-Path -LiteralPath $stagedSignaturePath -PathType Leaf)) {
            throw "Siegfried update completed without creating default.sig in '$stagingPath'."
        }

        $signatureSource = "siegfried-verified-update"
    }

    Write-Host "Probing installed tool and PRONOM signature..."
    $probe = Invoke-JsonProbe `
        -Executable $stagedSfPath `
        -Signature $stagedSignaturePath `
        -ToolHome $stagingPath

    $observedVersion = [string]$probe.siegfried
    if ($observedVersion -ne $RequiredVersion) {
        throw "Siegfried version mismatch: required '$RequiredVersion', observed '$observedVersion'."
    }

    $requiredPronomNumber = $RequiredPronomVersion.TrimStart([char[]]"vV")
    if ([string]::IsNullOrWhiteSpace($requiredPronomNumber)) {
        throw "RequiredPronomVersion '$RequiredPronomVersion' is invalid."
    }

    $identifierDetails = @(
        $probe.identifiers | ForEach-Object { [string]$_.details }
    )

    $expectedPronomFilePattern = "DROID_SignatureFile_V$([regex]::Escape($requiredPronomNumber))\.xml"
    $pronomMatched = @(
        $identifierDetails | Where-Object {
            $_ -match $expectedPronomFilePattern
        }
    ).Count -gt 0

    if (-not $pronomMatched) {
        throw "PRONOM signature mismatch: required '$RequiredPronomVersion'; identifier details were: $($identifierDetails -join '; ')"
    }

    $manifest = [ordered]@{
        schemaVersion = 1
        sourceRepository = "https://github.com/richardlehane/siegfried"
        releaseTag = $Tag
        releaseAsset = [ordered]@{
            name = $assetName
            browserDownloadUrl = $assetDownloadUrl
            sizeBytes = $assetSize
            githubDigest = $assetDigest
            downloadedSha256 = $actualAssetSha
        }
        tool = [ordered]@{
            name = "siegfried"
            version = $observedVersion
            filename = "sf.exe"
            sha256 = Get-Sha256 -Path $stagedSfPath
        }
        signature = [ordered]@{
            filename = "default.sig"
            version = $RequiredPronomVersion
            sha256 = Get-Sha256 -Path $stagedSignaturePath
            created = $probe.created
            source = $signatureSource
            identifiers = @($probe.identifiers)
        }
        containerExpansion = $false
        acquiredAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
        installer = [ordered]@{
            script = "scripts/install-siegfried.ps1"
            powershellVersion = $PSVersionTable.PSVersion.ToString()
            destination = $destinationPath
        }
    }

    $manifestJson = $manifest | ConvertTo-Json -Depth 100
    Write-Utf8NoBom `
        -Path $stagedManifestPath `
        -Content ($manifestJson + [Environment]::NewLine)

    # Verify the complete staged installation before replacing any working copy.

    Invoke-InstalledToolVerification -Directory $stagingPath

    if (Test-Path -LiteralPath $destinationPath) {
        Move-Item `
            -LiteralPath $destinationPath `
            -Destination $backupPath
        $destinationBackedUp = $true
    }

    try {
        Move-Item `
            -LiteralPath $stagingPath `
            -Destination $destinationPath
        $installed = $true
    }
    catch {
        if ($destinationBackedUp -and
            (Test-Path -LiteralPath $backupPath) -and
            -not (Test-Path -LiteralPath $destinationPath)) {
            Move-Item `
                -LiteralPath $backupPath `
                -Destination $destinationPath
            $destinationBackedUp = $false
        }

        throw
    }

    # Verify once more at the final path so path-sensitive mistakes cannot pass

    # staging validation.

    Invoke-InstalledToolVerification -Directory $destinationPath

    if ($destinationBackedUp -and (Test-Path -LiteralPath $backupPath)) {
        Remove-Item -LiteralPath $backupPath -Recurse -Force
        $destinationBackedUp = $false
    }

    Write-Host ""
    Write-Host "Installed and verified pinned format-identification toolchain:"
    Write-Host "  Siegfried: $RequiredVersion"
    Write-Host "  PRONOM:    $RequiredPronomVersion"
    Write-Host "  Directory: $destinationPath"
}
catch {
    # If the final-path verification failed, remove the newly installed copy.

    if ($installed -and (Test-Path -LiteralPath $destinationPath)) {
        Remove-Item `
            -LiteralPath $destinationPath `
            -Recurse `
            -Force `
            -ErrorAction SilentlyContinue
    }

    # Restore the previously verified/installed directory when one existed.

    if ($destinationBackedUp -and (Test-Path -LiteralPath $backupPath)) {
        Move-Item `
            -LiteralPath $backupPath `
            -Destination $destinationPath

        $destinationBackedUp = $false
    }

    throw
}
finally {
    Remove-Item `
        -LiteralPath $tempRoot `
        -Recurse `
        -Force `
        -ErrorAction SilentlyContinue

    Remove-Item `
        -LiteralPath $stagingPath `
        -Recurse `
        -Force `
        -ErrorAction SilentlyContinue

    # A backup is deliberately retained if restoration could not be completed.

    if (-not $destinationBackedUp) {
        Remove-Item `
            -LiteralPath $backupPath `
            -Recurse `
            -Force `
            -ErrorAction SilentlyContinue
    }
}
