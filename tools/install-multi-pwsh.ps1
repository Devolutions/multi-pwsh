[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$Version = 'latest',

    [Parameter(Mandatory = $false)]
    [string]$Owner = 'awakecoding',

    [Parameter(Mandatory = $false)]
    [string]$Repository = 'pwsh-host-rs'
)

$ErrorActionPreference = 'Stop'

function Get-ReleaseArch {
    $candidates = @($env:PROCESSOR_ARCHITECTURE, $env:PROCESSOR_ARCHITEW6432) | Where-Object { $_ }

    foreach ($candidate in $candidates) {
        switch ($candidate.ToUpperInvariant()) {
            'ARM64' { return 'arm64' }
            'AMD64' { return 'x64' }
        }
    }

    if ([Environment]::Is64BitOperatingSystem) {
        return 'x64'
    }

    throw "Unsupported architecture. Supported architectures: AMD64, ARM64"
}

function Test-PathContainsEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $entryNormalized = $Entry.Trim().TrimEnd('\\')
    $segments = $PathValue -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    foreach ($segment in $segments) {
        if ([string]::Equals($segment.Trim().TrimEnd('\\'), $entryNormalized, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

$arch = Get-ReleaseArch
$assetName = "multi-pwsh-windows-$arch.zip"

if ($Version -eq 'latest') {
    $releasePath = 'latest/download'
    $displayVersion = 'latest'
}
else {
    if (-not $Version.StartsWith('v', [System.StringComparison]::OrdinalIgnoreCase)) {
        $Version = "v$Version"
    }

    $releasePath = "download/$Version"
    $displayVersion = $Version
}

$downloadUrl = "https://github.com/$Owner/$Repository/releases/$releasePath/$assetName"
$installRoot = Join-Path $HOME '.pwsh'
$binDir = Join-Path $installRoot 'bin'
$targetExe = Join-Path $binDir 'multi-pwsh.exe'

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("multi-pwsh-install-" + [System.Guid]::NewGuid().ToString('N'))
$archivePath = Join-Path $tempRoot $assetName
$extractDir = Join-Path $tempRoot 'extract'

New-Item -Path $extractDir -ItemType Directory -Force | Out-Null

try {
    Write-Host "Downloading $assetName ($displayVersion)..."

    $invokeParams = @{
        Uri = $downloadUrl
        OutFile = $archivePath
    }

    if ($PSVersionTable.PSEdition -eq 'Desktop') {
        $invokeParams['UseBasicParsing'] = $true
    }

    Invoke-WebRequest @invokeParams

    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    $sourceExe = Join-Path $extractDir 'multi-pwsh.exe'
    if (-not (Test-Path -Path $sourceExe -PathType Leaf)) {
        throw 'Archive did not contain expected binary: multi-pwsh.exe'
    }

    New-Item -Path $binDir -ItemType Directory -Force | Out-Null
    Copy-Item -Path $sourceExe -Destination $targetExe -Force

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (Test-PathContainsEntry -PathValue $userPath -Entry $binDir)) {
        $newUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $binDir } else { "$userPath;$binDir" }
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
        $pathStatus = "Added $binDir to user PATH."
    }
    else {
        $pathStatus = "$binDir is already present in user PATH."
    }

    if (-not (Test-PathContainsEntry -PathValue $env:Path -Entry $binDir)) {
        $env:Path = "$binDir;$env:Path"
    }

    Write-Host "Installed multi-pwsh to $targetExe"
    Write-Host $pathStatus
    Write-Host 'Run: multi-pwsh --help'
}
finally {
    if (Test-Path -Path $tempRoot -PathType Container) {
        Remove-Item -Path $tempRoot -Recurse -Force
    }
}