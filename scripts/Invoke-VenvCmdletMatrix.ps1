[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string[]]$Aliases,

    [Parameter(Mandatory = $false)]
    [switch]$EnableOnlineTests,

    [Parameter(Mandatory = $false)]
    [switch]$KeepVenv,

    [Parameter(Mandatory = $false)]
    [string]$VenvPrefix = 'pester-venv'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-MultiPwshHome {
    if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_HOME)) {
        return $env:MULTI_PWSH_HOME
    }

    return (Join-Path $HOME '.pwsh')
}

function Get-VenvRoot {
    if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_VENV_DIR)) {
        return $env:MULTI_PWSH_VENV_DIR
    }

    return (Join-Path (Get-MultiPwshHome) 'venv')
}

function Get-InstalledVersionAliases {
    $commands = Get-Command 'pwsh-*' -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandType -eq 'Application' }

    $names = foreach ($command in $commands) {
        $leaf = Split-Path -Leaf $command.Source

        if ($IsWindows) {
            if ($leaf.EndsWith('.cmd', [System.StringComparison]::OrdinalIgnoreCase)) {
                continue
            }

            if ($leaf.EndsWith('.exe', [System.StringComparison]::OrdinalIgnoreCase)) {
                $leaf = [IO.Path]::GetFileNameWithoutExtension($leaf)
            }
        }

        if ($leaf -match '^pwsh-\d+\.\d+\.\d+([.-].+)?$') {
            $leaf
        }
    }

    return $names | Sort-Object -Unique
}

if (-not (Get-Module -ListAvailable Pester)) {
    throw "Pester was not found. Install it with: Install-Module Pester -Scope CurrentUser"
}

Import-Module Pester -ErrorAction Stop

$resolvedAliases = @(
    if ($Aliases -and $Aliases.Count -gt 0) {
        $Aliases | Sort-Object -Unique
    }
    else {
        Get-InstalledVersionAliases
    }
)

if (-not $resolvedAliases -or $resolvedAliases.Count -eq 0) {
    throw 'No installed pwsh version aliases were found (expected names like pwsh-7.4.13).'
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$testPath = Join-Path $repoRoot 'tests\pester\Venv.Cmdlets.Tests.ps1'
if (-not (Test-Path -LiteralPath $testPath -PathType Leaf)) {
    throw "Test file not found at $testPath"
}

$venvRoot = Get-VenvRoot
New-Item -ItemType Directory -Path $venvRoot -Force | Out-Null

$results = New-Object System.Collections.Generic.List[object]
$failedAliases = New-Object System.Collections.Generic.List[string]

foreach ($aliasName in $resolvedAliases) {
    $sanitizedAlias = ($aliasName -replace '[^A-Za-z0-9]+', '-').Trim('-')
    $venvName = '{0}-{1}-{2:yyyyMMddHHmmssfff}' -f $VenvPrefix, $sanitizedAlias, (Get-Date)
    $venvPath = Join-Path $venvRoot $venvName

    Write-Host "`n=== [$aliasName] creating venv '$venvName' ===" -ForegroundColor Cyan
    & multi-pwsh venv create $venvName | Out-Host

    try {
        $container = New-PesterContainer -Path $testPath -Data @{
            PwshAlias = $aliasName
            VenvName = $venvName
            VenvRoot = $venvPath
            EnableOnlineTests = [bool]$EnableOnlineTests
        }

        $run = Invoke-Pester -Container $container -Output Detailed -PassThru

        $results.Add([pscustomobject]@{
            Alias = $aliasName
            Failed = $run.FailedCount
            Passed = $run.PassedCount
            Skipped = $run.SkippedCount
            Duration = $run.Duration
        })

        if ($run.FailedCount -gt 0) {
            $failedAliases.Add($aliasName)
        }
    }
    finally {
        if (-not $KeepVenv) {
            & multi-pwsh venv delete $venvName | Out-Host
        }
        else {
            Write-Host "Keeping venv: $venvPath" -ForegroundColor Yellow
        }
    }
}

Write-Host "`n=== Matrix summary ===" -ForegroundColor Cyan
$results | Sort-Object Alias | Format-Table -AutoSize | Out-Host

if ($failedAliases.Count -gt 0) {
    throw ("Venv cmdlet matrix failed for aliases: {0}" -f ($failedAliases -join ', '))
}

Write-Host 'All alias runs passed.' -ForegroundColor Green
