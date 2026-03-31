Set-StrictMode -Version Latest

function Show-Banner {
    param([string]$Text)

    Write-Host ''
    Write-Host ('=' * 78) -ForegroundColor DarkCyan
    Write-Host $Text -ForegroundColor Cyan
    Write-Host ('=' * 78) -ForegroundColor DarkCyan
}

function Pause-Demo {
    param([int]$PauseSeconds)

    if ($PauseSeconds -gt 0) {
        Start-Sleep -Seconds $PauseSeconds
    }
}

function Invoke-DemoStep {
    param(
        [string]$Caption,
        [string]$DisplayedCommand,
        [scriptblock]$Action,
        [int]$PauseSeconds = 2
    )

    Show-Banner $Caption
    Write-Host "PS> $DisplayedCommand" -ForegroundColor Yellow
    Pause-Demo -PauseSeconds $PauseSeconds
    & $Action
    Pause-Demo -PauseSeconds $PauseSeconds
}

function Write-NormalizedOutput {
    param(
        [Parameter(ValueFromPipeline = $true)]
        $InputObject
    )

    process {
        $text = $InputObject.ToString()

        if (-not [string]::IsNullOrWhiteSpace($HOME)) {
            $text = $text -replace [regex]::Escape($HOME), '~'
        }

        Write-Host $text
    }
}

function New-DemoContext {
    param(
        [Parameter(Mandatory = $true)]
        [string]$DemoName,

        [switch]$KeepArtifacts
    )

    $demoHome = Join-Path $HOME ".pwsh-demo\$DemoName"
    $demoBin = Join-Path $demoHome 'bin'
    $demoCache = Join-Path $HOME '.pwsh\cache'
    $demoVenv = Join-Path $demoHome 'venv'

    $previous = [pscustomobject]@{
        MultiPwshHome = [Environment]::GetEnvironmentVariable('MULTI_PWSH_HOME', 'Process')
        MultiPwshBinDir = [Environment]::GetEnvironmentVariable('MULTI_PWSH_BIN_DIR', 'Process')
        MultiPwshCacheDir = [Environment]::GetEnvironmentVariable('MULTI_PWSH_CACHE_DIR', 'Process')
        MultiPwshVenvDir = [Environment]::GetEnvironmentVariable('MULTI_PWSH_VENV_DIR', 'Process')
        Path = $env:PATH
    }

    if (Test-Path -LiteralPath $demoHome) {
        Remove-Item -LiteralPath $demoHome -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $demoHome, $demoBin, $demoVenv | Out-Null

    $env:MULTI_PWSH_HOME = $demoHome
    $env:MULTI_PWSH_BIN_DIR = $demoBin
    $env:MULTI_PWSH_CACHE_DIR = $demoCache
    $env:MULTI_PWSH_VENV_DIR = $demoVenv
    $env:PATH = "$demoBin;$($env:PATH)"

    [pscustomobject]@{
        DemoName = $DemoName
        Home = $demoHome
        BinDir = $demoBin
        CacheDir = $demoCache
        VenvDir = $demoVenv
        Previous = $previous
        KeepArtifacts = [bool]$KeepArtifacts
    }
}

function Remove-DemoContext {
    param(
        [Parameter(Mandatory = $true)]
        $Context
    )

    $env:PATH = $Context.Previous.Path

    foreach ($entry in @(
        @{ Name = 'MULTI_PWSH_HOME'; Value = $Context.Previous.MultiPwshHome },
        @{ Name = 'MULTI_PWSH_BIN_DIR'; Value = $Context.Previous.MultiPwshBinDir },
        @{ Name = 'MULTI_PWSH_CACHE_DIR'; Value = $Context.Previous.MultiPwshCacheDir },
        @{ Name = 'MULTI_PWSH_VENV_DIR'; Value = $Context.Previous.MultiPwshVenvDir }
    )) {
        if ($null -eq $entry.Value) {
            Remove-Item "Env:$($entry.Name)" -ErrorAction SilentlyContinue
        }
        else {
            Set-Item "Env:$($entry.Name)" -Value $entry.Value
        }
    }

    if (-not $Context.KeepArtifacts -and (Test-Path -LiteralPath $Context.Home)) {
        Remove-Item -LiteralPath $Context.Home -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Show-DemoContext {
    param([Parameter(Mandatory = $true)]$Context)

    ([pscustomobject]@{
        DemoHome = $Context.Home
        BinDir = $Context.BinDir
        CacheDir = $Context.CacheDir
        VenvDir = $Context.VenvDir
    } | Format-List | Out-String -Width 200).TrimEnd() | Write-NormalizedOutput
}

function Get-DemoBinItemPath {
    param(
        [Parameter(Mandatory = $true)]$Context,
        [Parameter(Mandatory = $true)][string]$CommandName
    )

    if ($IsWindows) {
        return (Join-Path $Context.BinDir "$CommandName.exe")
    }

    Join-Path $Context.BinDir $CommandName
}
