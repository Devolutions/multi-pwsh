<#
.SYNOPSIS
    Downloads and installs PowerShell Core on GitHub Actions runners.

.DESCRIPTION
    This script downloads and installs PowerShell Core from the official GitHub releases.
    Supports Windows, macOS, and Linux runners with various architectures.

.PARAMETER Version
    The version to install: 'stable', 'latest', 'preview', or a specific version (e.g., '7.4.0')

.PARAMETER Architecture
    The architecture: 'x64', 'x86', 'arm64', 'arm32', or 'auto'

.EXAMPLE
    ./Install-PowerShell.ps1 -Version stable -Architecture auto
    ./Install-PowerShell.ps1 -Version 7.4.0 -Architecture x64
    ./Install-PowerShell.ps1 -Version preview -Architecture arm64
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$Version = "stable",

    [Parameter(Mandatory = $false)]
    [ValidateSet("x64", "x86", "arm64", "arm32", "auto")]
    [string]$Architecture = "auto"
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

#region Helper Functions

function Invoke-WithRetry {
    <#
    .SYNOPSIS
        Executes a script block with retry logic and exponential backoff.
    #>
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$ScriptBlock,

        [Parameter(Mandatory = $true)]
        [int]$MaxRetries,

        [Parameter(Mandatory = $true)]
        [int]$InitialDelaySeconds,

        [Parameter(Mandatory = $true)]
        [string]$OperationName
    )

    $attempt = 0
    $lastError = $null

    while ($attempt -lt $MaxRetries) {
        $attempt++
        try {
            return & $ScriptBlock
        }
        catch {
            $lastError = $_
            if ($attempt -lt $MaxRetries) {
                $delay = $InitialDelaySeconds * [math]::Pow(2, $attempt - 1)
                Write-Host "   ⚠️  $OperationName failed (attempt $attempt/$MaxRetries): $($_.Exception.Message)"
                Write-Host "   🔄 Retrying in $delay seconds..."
                Start-Sleep -Seconds $delay
            }
        }
    }

    throw "Failed after $MaxRetries attempts: $lastError"
}

function Write-ActionOutput {
    param(
        [string]$Name,
        [string]$Value
    )
    $outputFile = $env:GITHUB_OUTPUT
    if ($outputFile) {
        "$Name=$Value" | Out-File -FilePath $outputFile -Append -Encoding utf8
    }
    Write-Host "::notice::Output $Name=$Value"
}

function Write-ActionPath {
    param([string]$Path)
    $pathFile = $env:GITHUB_PATH
    if ($pathFile) {
        $Path | Out-File -FilePath $pathFile -Append -Encoding utf8
    }
}

function Get-RunnerOS {
    if ($IsWindows -or $env:OS -eq "Windows_NT") {
        return "windows"
    }
    elseif ($IsMacOS) {
        return "macos"
    }
    elseif ($IsLinux) {
        return "linux"
    }
    else {
        throw "Unsupported operating system"
    }
}

function Get-RunnerArchitecture {
    param([string]$RequestedArch)

    if ($RequestedArch -ne "auto") {
        return $RequestedArch
    }

    # Auto-detect architecture
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64" { return "x64" }
        "X86" { return "x86" }
        "Arm64" { return "arm64" }
        "Arm" { return "arm32" }
        default { return "x64" }
    }
}

function Get-PowerShellReleases {
    param([bool]$IncludePrerelease = $false)

    $apiUrl = "https://api.github.com/repos/PowerShell/PowerShell/releases"
    $headers = @{
        "Accept" = "application/vnd.github.v3+json"
        "User-Agent" = "setup-pwsh-action"
    }

    # Add GitHub token if available for higher rate limits
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
    }

    try {
        $releases = Invoke-RestMethod -Uri $apiUrl -Headers $headers -Method Get
        return $releases
    }
    catch {
        throw "Failed to fetch releases from GitHub API: $_"
    }
}

function Get-TargetRelease {
    param(
        [string]$Version,
        [array]$Releases
    )

    switch ($Version.ToLower()) {
        "latest" {
            # Get the latest release (from /releases/latest) - currently 7.5.x
            $release = $Releases | Where-Object { -not $_.prerelease } | Select-Object -First 1
            if (-not $release) {
                throw "No latest release found"
            }
            Write-Host "   • Using latest release track"
            return $release
        }
        { $_ -in "stable", "lts" } {
            # Get the latest LTS/stable release - currently 7.4.x (supported until Nov 2026)
            # Note: 7.2.x LTS reached end-of-support in Nov 2024
            $release = $Releases | Where-Object {
                -not $_.prerelease -and $_.tag_name -match '^v7\.4\.'
            } | Select-Object -First 1

            if (-not $release) {
                throw "No LTS/stable release found (7.4.x)"
            }
            Write-Host "   • Using LTS/stable release track (7.4.x - supported until Nov 2026)"
            return $release
        }
        "preview" {
            # Get latest prerelease
            $release = $Releases | Where-Object { $_.prerelease } | Select-Object -First 1
            if (-not $release) {
                throw "No preview release found"
            }
            Write-Host "   • Using preview release track"
            return $release
        }
        default {
            # Specific version - normalize version string
            $targetVersion = $Version
            if (-not $targetVersion.StartsWith("v")) {
                $targetVersion = "v$targetVersion"
            }

            $release = $Releases | Where-Object { $_.tag_name -eq $targetVersion }
            if (-not $release) {
                # Try without 'v' prefix
                $release = $Releases | Where-Object { $_.tag_name -eq $Version }
            }
            if (-not $release) {
                throw "Release version '$Version' not found. Please check available versions at https://github.com/PowerShell/PowerShell/releases"
            }
            return $release
        }
    }
}

function Get-AssetPattern {
    param(
        [string]$OS,
        [string]$Arch
    )

    $patterns = @{
        "windows" = @{
            "x64"   = "PowerShell-*-win-x64.zip"
            "x86"   = "PowerShell-*-win-x86.zip"
            "arm64" = "PowerShell-*-win-arm64.zip"
        }
        "macos" = @{
            "x64"   = "powershell-*-osx-x64.tar.gz"
            "arm64" = "powershell-*-osx-arm64.tar.gz"
        }
        "linux" = @{
            "x64"   = "powershell-*-linux-x64.tar.gz"
            "arm64" = "powershell-*-linux-arm64.tar.gz"
            "arm32" = "powershell-*-linux-arm32.tar.gz"
        }
    }

    if (-not $patterns.ContainsKey($OS)) {
        throw "Unsupported OS: $OS"
    }

    $osPatterns = $patterns[$OS]
    if (-not $osPatterns.ContainsKey($Arch)) {
        throw "Architecture '$Arch' is not supported on $OS. Supported architectures: $($osPatterns.Keys -join ', ')"
    }

    return $osPatterns[$Arch]
}

function Find-ReleaseAsset {
    param(
        [object]$Release,
        [string]$Pattern
    )

    # Convert wildcard pattern to regex
    $regexPattern = "^" + ($Pattern -replace "\*", ".*") + "$"

    $asset = $Release.assets | Where-Object { $_.name -match $regexPattern } | Select-Object -First 1

    if (-not $asset) {
        $availableAssets = ($Release.assets | Select-Object -ExpandProperty name) -join "`n  "
        throw "No matching asset found for pattern '$Pattern'. Available assets:`n  $availableAssets"
    }

    return $asset
}

function Get-InstallPath {
    param(
        [string]$OS,
        [string]$Version
    )

    $basePath = switch ($OS) {
        "windows" { Join-Path $env:RUNNER_TOOL_CACHE "pwsh" }
        "macos"   { Join-Path $env:RUNNER_TOOL_CACHE "pwsh" }
        "linux"   { Join-Path $env:RUNNER_TOOL_CACHE "pwsh" }
        default   { Join-Path $env:TEMP "pwsh" }
    }

    # Use runner tool cache if available, otherwise use temp
    if (-not $env:RUNNER_TOOL_CACHE) {
        $basePath = switch ($OS) {
            "windows" { Join-Path $env:LOCALAPPDATA "pwsh" }
            "macos"   { Join-Path $env:HOME ".local/pwsh" }
            "linux"   { Join-Path $env:HOME ".local/pwsh" }
        }
    }

    return Join-Path $basePath $Version
}

function Install-PowerShellFromArchive {
    param(
        [string]$ArchivePath,
        [string]$InstallPath,
        [string]$OS
    )

    # Create installation directory
    if (Test-Path $InstallPath) {
        Remove-Item $InstallPath -Recurse -Force
    }
    New-Item -ItemType Directory -Path $InstallPath -Force | Out-Null

    Write-Host "📦 Extracting to: $InstallPath"

    if ($OS -eq "windows") {
        # Use Expand-Archive for Windows zip files
        Expand-Archive -Path $ArchivePath -DestinationPath $InstallPath -Force
    }
    else {
        # Use tar for Unix tar.gz files
        $tarArgs = @("-xzf", $ArchivePath, "-C", $InstallPath)
        $result = & tar @tarArgs 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to extract archive: $result"
        }

        # Make pwsh executable
        $pwshPath = Join-Path $InstallPath "pwsh"
        if (Test-Path $pwshPath) {
            & chmod +x $pwshPath
        }
    }

    return $InstallPath
}

#endregion

#region Main Script

Write-Host "🚀 Setup PowerShell Core Action"
Write-Host "================================"
Write-Host ""

# Detect OS and architecture
$os = Get-RunnerOS
$arch = Get-RunnerArchitecture -RequestedArch $Architecture

Write-Host "📋 Configuration:"
Write-Host "   • Requested Version: $Version"
Write-Host "   • Operating System: $os"
Write-Host "   • Architecture: $arch"
Write-Host ""

# Fetch releases
Write-Host "🔍 Fetching PowerShell releases from GitHub..."
$releases = Get-PowerShellReleases

# Find target release
Write-Host "🎯 Finding target release..."
$release = Get-TargetRelease -Version $Version -Releases $releases
$releaseVersion = $release.tag_name -replace "^v", ""

Write-Host "   • Selected release: $($release.tag_name) ($(if ($release.prerelease) { 'preview' } else { 'stable' }))"

# Check if already installed
$installPath = Get-InstallPath -OS $os -Version $releaseVersion
$pwshExe = if ($os -eq "windows") { Join-Path $installPath "pwsh.exe" } else { Join-Path $installPath "pwsh" }

if (Test-Path $pwshExe) {
    Write-Host "✅ PowerShell $releaseVersion is already installed at: $installPath"
    Write-ActionOutput -Name "version" -Value $releaseVersion
    Write-ActionOutput -Name "path" -Value $installPath
    Write-ActionPath -Path $installPath
    exit 0
}

# Find download asset
$pattern = Get-AssetPattern -OS $os -Arch $arch
Write-Host "🔎 Looking for asset matching: $pattern"

$asset = Find-ReleaseAsset -Release $release -Pattern $pattern
Write-Host "   • Found asset: $($asset.name)"
Write-Host "   • Size: $([math]::Round($asset.size / 1MB, 2)) MB"

# Download asset
$downloadPath = Join-Path ([System.IO.Path]::GetTempPath()) $asset.name
Write-Host ""
Write-Host "⬇️  Downloading $($asset.name)..."

try {
    $headers = @{
        "User-Agent" = "setup-pwsh-action"
    }

    Invoke-WithRetry -ScriptBlock {
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $downloadPath -Headers $headers
    } -MaxRetries 10 -InitialDelaySeconds 10 -OperationName "Download"

    Write-Host "   • Downloaded to: $downloadPath"
}
catch {
    throw "Failed to download asset: $_"
}

# Install
Write-Host ""
Write-Host "📦 Installing PowerShell $releaseVersion..."

try {
    $installPath = Install-PowerShellFromArchive -ArchivePath $downloadPath -InstallPath $installPath -OS $os

    # Cleanup
    Remove-Item $downloadPath -Force -ErrorAction SilentlyContinue
}
catch {
    # Cleanup on failure
    Remove-Item $downloadPath -Force -ErrorAction SilentlyContinue
    throw "Installation failed: $_"
}

# Add to PATH
Write-Host ""
Write-Host "🔧 Adding to PATH..."
Write-ActionPath -Path $installPath

# Set outputs
Write-ActionOutput -Name "version" -Value $releaseVersion
Write-ActionOutput -Name "path" -Value $installPath

# Verify installation
$pwshExe = if ($os -eq "windows") { Join-Path $installPath "pwsh.exe" } else { Join-Path $installPath "pwsh" }

if (Test-Path $pwshExe) {
    Write-Host ""
    Write-Host "✅ PowerShell $releaseVersion installed successfully!"
    Write-Host "   • Path: $installPath"

    # Display version info
    $versionOutput = & $pwshExe -Command '$PSVersionTable | ConvertTo-Json -Compress'
    Write-Host "   • Version Info: $versionOutput"
}
else {
    throw "Installation verification failed - pwsh executable not found at: $pwshExe"
}

Write-Host ""
Write-Host "🎉 Setup complete!"

#endregion
