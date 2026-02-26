[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$')]
    [string]$Version,

    [Parameter(Mandatory = $false)]
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$cratesRoot = Join-Path $repoRoot 'crates'

if (-not (Test-Path -Path $cratesRoot -PathType Container)) {
    throw "Crates directory not found: $cratesRoot"
}

$cargoFiles = Get-ChildItem -Path $cratesRoot -Directory |
    ForEach-Object { Join-Path $_.FullName 'Cargo.toml' } |
    Where-Object { Test-Path -Path $_ -PathType Leaf }

if (-not $cargoFiles) {
    throw "No crate Cargo.toml files found under $cratesRoot"
}

$encoding = New-Object System.Text.UTF8Encoding($false)
$updated = @()

foreach ($cargoFile in $cargoFiles) {
    $content = [System.IO.File]::ReadAllText($cargoFile)

    $pattern = '(?ms)^(\[package\]\s*.*?^version\s*=\s*")(?<current>[^"]+)(")'
    $match = [System.Text.RegularExpressions.Regex]::Match($content, $pattern)
    if (-not $match.Success) {
        throw "Could not find package version field in $cargoFile"
    }

    $currentVersion = $match.Groups['current'].Value
    if ($currentVersion -eq $Version) {
        continue
    }

    $replacement = $match.Groups[1].Value + $Version + $match.Groups[3].Value
    $newContent = $content.Substring(0, $match.Index) + $replacement + $content.Substring($match.Index + $match.Length)

    if (-not $DryRun) {
        [System.IO.File]::WriteAllText($cargoFile, $newContent, $encoding)
    }

    $updated += $cargoFile
}

if ($updated.Count -eq 0) {
    Write-Host "All crate package versions are already $Version"
    exit 0
}

Write-Host "Updated crate versions to ${Version}:"
$updated | ForEach-Object { Write-Host " - $_" }
