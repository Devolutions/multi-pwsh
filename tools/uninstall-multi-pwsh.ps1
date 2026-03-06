[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Remove-PathEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $PathValue
    }

    $entryNormalized = $Entry.Trim().TrimEnd('\\')
    $segments = $PathValue -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    $filtered = $segments | Where-Object {
        -not [string]::Equals($_.Trim().TrimEnd('\\'), $entryNormalized, [System.StringComparison]::OrdinalIgnoreCase)
    }

    return ($filtered -join ';')
}

function Get-MultiPwshHome {
    if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_HOME)) {
        return $env:MULTI_PWSH_HOME
    }

    return (Join-Path $HOME '.pwsh')
}

function Get-MultiPwshBinDir {
    if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_BIN_DIR)) {
        return $env:MULTI_PWSH_BIN_DIR
    }

    return (Join-Path (Get-MultiPwshHome) 'bin')
}

$installHome = Get-MultiPwshHome
$binDir = Get-MultiPwshBinDir
$targetExe = Join-Path $binDir 'multi-pwsh.exe'

if (Test-Path -Path $targetExe -PathType Leaf) {
    Remove-Item -Path $targetExe -Force
    Write-Host "Removed $targetExe"
}
else {
    Write-Host "No installed binary found at $targetExe"
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$newUserPath = Remove-PathEntry -PathValue $userPath -Entry $binDir

if ($newUserPath -ne $userPath) {
    [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    Write-Host "Removed $binDir from user PATH"
}

$newProcessPath = Remove-PathEntry -PathValue $env:Path -Entry $binDir
if ($newProcessPath -ne $env:Path) {
    $env:Path = $newProcessPath
}

if (Test-Path -Path $binDir -PathType Container) {
    $remaining = Get-ChildItem -Path $binDir -Force -ErrorAction SilentlyContinue
    if (-not $remaining) {
        Remove-Item -Path $binDir -Force
        Write-Host "Removed empty directory $binDir"
    }
}

Write-Host 'multi-pwsh uninstall complete'