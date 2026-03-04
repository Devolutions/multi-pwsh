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
$releaseWorkflow = Join-Path $repoRoot '.github/workflows/release.yml'

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
$workflowUpdated = $false

foreach ($cargoFile in $cargoFiles) {
    $content = [System.IO.File]::ReadAllText($cargoFile)

    $pattern = '(?ms)^(\[package\]\s*.*?^version\s*=\s*")(?<current>[^"\r\n]+)(")'
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

if (Test-Path -Path $releaseWorkflow -PathType Leaf) {
    $workflowContent = [System.IO.File]::ReadAllText($releaseWorkflow)
    $workflowPattern = '(?m)(?<prefix>for example\s+v)(?<current>\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)'
    $workflowMatch = [System.Text.RegularExpressions.Regex]::Match($workflowContent, $workflowPattern)

    if ($workflowMatch.Success -and $workflowMatch.Groups['current'].Value -ne $Version) {
        $replacement = $workflowMatch.Groups['prefix'].Value + $Version
        $newWorkflowContent = $workflowContent.Substring(0, $workflowMatch.Index) +
            $replacement +
            $workflowContent.Substring($workflowMatch.Index + $workflowMatch.Length)

        if (-not $DryRun) {
            [System.IO.File]::WriteAllText($releaseWorkflow, $newWorkflowContent, $encoding)
        }

        $workflowUpdated = $true
    }
}

if ($updated.Count -eq 0 -and -not $workflowUpdated) {
    Write-Host "All crate package versions are already $Version"
    if (Test-Path -Path $releaseWorkflow -PathType Leaf) {
        Write-Host "No release workflow example tag needed updating"
    }
    exit 0
}

if ($updated.Count -gt 0) {
    Write-Host "Updated crate versions to ${Version}:"
    $updated | ForEach-Object { Write-Host " - $_" }
}

if ($workflowUpdated) {
    Write-Host "Updated release workflow example tag in: $releaseWorkflow"
}
