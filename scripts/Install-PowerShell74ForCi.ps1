[CmdletBinding()]
param(
    [ValidateSet('7.4', '7.7')]
    [string]$VersionLine = '7.4'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-GitHubHeaders {
    $headers = @{
        "User-Agent" = "multi-pwsh-ci"
    }

    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_TOKEN)) {
        $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
    }

    $headers
}

function Get-PowerShellRelease {
    param([Parameter(Mandatory = $true)][string]$Line)

    $headers = Get-GitHubHeaders
    $releases = Invoke-RestMethod `
        -Headers $headers `
        -Uri "https://api.github.com/repos/PowerShell/PowerShell/releases?per_page=100"

    $tagPattern = "^v$([regex]::Escape($Line))\.\d+$"
    $release = $releases |
        Where-Object { -not $_.prerelease -and $_.tag_name -match $tagPattern } |
        Sort-Object { [version]$_.tag_name.TrimStart("v") } -Descending |
        Select-Object -First 1

    if ($null -eq $release) {
        throw "Unable to find a PowerShell $Line.x release."
    }

    $release
}

function Get-AssetName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    $arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
    $archName = switch ($arch) {
        "Arm64" { "arm64" }
        "X64" { "x64" }
        default { throw "Unsupported process architecture for PowerShell CI bootstrap: $arch" }
    }

    if ($IsWindows) {
        "PowerShell-$Version-win-$archName.zip"
    }
    elseif ($IsMacOS) {
        "powershell-$Version-osx-$archName.tar.gz"
    }
    else {
        "powershell-$Version-linux-$archName.tar.gz"
    }
}

function New-PwshShim {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PwshExe,

        [Parameter(Mandatory = $true)]
        [string]$Version,

        [Parameter(Mandatory = $true)]
        [string]$Line,

        [Parameter(Mandatory = $true)]
        [string]$TempRoot
    )

    $shimDir = Join-Path $TempRoot "pwsh-$Line-shims"
    New-Item -Path $shimDir -ItemType Directory -Force | Out-Null

    $minorShimName = if ($IsWindows) { "pwsh-$Line.cmd" } else { "pwsh-$Line" }
    $patchShimName = if ($IsWindows) { "pwsh-$Version.cmd" } else { "pwsh-$Version" }

    foreach ($shimName in @($minorShimName, $patchShimName)) {
        $shimPath = Join-Path $shimDir $shimName

        if ($IsWindows) {
            $escapedExe = $PwshExe.Replace("%", "%%")
            "@echo off`r`n`"$escapedExe`" %*`r`n" | Set-Content -Path $shimPath -Encoding ascii -NoNewline
        }
        else {
            @"
#!/usr/bin/env sh
exec "$PwshExe" "$@"
"@ | Set-Content -Path $shimPath -Encoding utf8NoBOM -NoNewline
            chmod +x $shimPath
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_PATH)) {
        $shimDir | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
    }

    $env:PATH = "$shimDir$([IO.Path]::PathSeparator)$env:PATH"
}

$tempRoot = if (-not [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    $env:RUNNER_TEMP
}
else {
    [IO.Path]::GetTempPath()
}

$release = Get-PowerShellRelease -Line $VersionLine
$version = $release.tag_name.TrimStart("v")
$assetName = Get-AssetName -Version $version
$asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1

if ($null -eq $asset) {
    throw "Unable to find PowerShell asset '$assetName' in release $($release.tag_name)."
}

$installDir = Join-Path $tempRoot "pwsh-$version"
$archivePath = Join-Path $tempRoot $assetName

if (Test-Path -Path $installDir) {
    Remove-Item -Path $installDir -Recurse -Force
}

New-Item -Path $installDir -ItemType Directory -Force | Out-Null

Write-Host "Downloading $assetName..."
Invoke-WebRequest -Uri $asset.browser_download_url -Headers (Get-GitHubHeaders) -OutFile $archivePath

if ($assetName.EndsWith(".zip", [StringComparison]::OrdinalIgnoreCase)) {
    Expand-Archive -Path $archivePath -DestinationPath $installDir -Force
}
else {
    tar -xzf $archivePath -C $installDir
}

$pwshExeName = if ($IsWindows) { "pwsh.exe" } else { "pwsh" }
$pwshExe = Join-Path $installDir $pwshExeName

if (-not (Test-Path -Path $pwshExe -PathType Leaf)) {
    throw "PowerShell executable was not found after extraction: $pwshExe"
}

if (-not $IsWindows) {
    chmod +x $pwshExe
}

New-PwshShim -PwshExe $pwshExe -Version $version -Line $VersionLine -TempRoot $tempRoot

$reportedVersion = (& $pwshExe -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()').Trim()
if ($reportedVersion -ne $version) {
    throw "The installed PowerShell reported version '$reportedVersion' but release '$version' was requested."
}

$environmentPrefix = if ($VersionLine -eq '7.4') { 'Pwsh' } else { 'Pwsh77' }
if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
    "$($environmentPrefix)ExePath=$pwshExe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    "$($environmentPrefix)QualifiedVersion=$version" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}

if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_STEP_SUMMARY)) {
    "Installed PowerShell $version (latest released $VersionLine.x) at ``$pwshExe``." |
        Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Encoding utf8 -Append
}

Write-Host "Installed PowerShell $version for CI: $pwshExe"
Write-Host "Qualified PowerShell version: $reportedVersion"
