[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $false)]
    [ValidateSet('Debug', 'Release', IgnoreCase = $true)]
    [string]$Configuration = 'Release',

    [Parameter(Mandatory = $false)]
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-RepositoryRoot {
    return (Split-Path -Parent $PSScriptRoot)
}

function Get-InstallBinDir {
    if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_BIN_DIR)) {
        return $env:MULTI_PWSH_BIN_DIR
    }

    $installHome = if (-not [string]::IsNullOrWhiteSpace($env:MULTI_PWSH_HOME)) {
        $env:MULTI_PWSH_HOME
    }
    else {
        Join-Path $HOME '.pwsh'
    }

    return (Join-Path $installHome 'bin')
}

function Get-BinaryName {
    if ($IsWindows) {
        return 'multi-pwsh.exe'
    }

    return 'multi-pwsh'
}

function Get-BuiltBinaryPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepositoryRoot,

        [Parameter(Mandatory = $true)]
        [string]$ConfigurationName
    )

    return (Join-Path $RepositoryRoot (Join-Path 'target' (Join-Path $ConfigurationName.ToLowerInvariant() (Get-BinaryName))))
}

function Build-MultiPwshBinary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepositoryRoot,

        [Parameter(Mandatory = $true)]
        [string]$ConfigurationName
    )

    $cargoArgs = @('build', '-p', 'multi-pwsh')
    if ($ConfigurationName.Equals('Release', [System.StringComparison]::OrdinalIgnoreCase)) {
        $cargoArgs += '--release'
    }

    Write-Host ("Building multi-pwsh ({0})..." -f $ConfigurationName.ToLowerInvariant())
    Push-Location $RepositoryRoot
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

function Get-CandidateBlockingProcesses {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinDir,

        [Parameter(Mandatory = $true)]
        [string]$TargetPath
    )

    $targetFullPath = [System.IO.Path]::GetFullPath($TargetPath)
    $binDirFullPath = [System.IO.Path]::GetFullPath($BinDir)
    $binDirPrefix = $binDirFullPath.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar

    $results = New-Object System.Collections.Generic.List[object]

    foreach ($process in Get-Process -ErrorAction SilentlyContinue) {
        $processPath = $null
        try {
            $processPath = $process.Path
        }
        catch {
        }

        if ([string]::IsNullOrWhiteSpace($processPath)) {
            try {
                $processPath = $process.MainModule.FileName
            }
            catch {
            }
        }

        $matchesPath = $false
        if (-not [string]::IsNullOrWhiteSpace($processPath)) {
            try {
                $processFullPath = [System.IO.Path]::GetFullPath($processPath)
                $matchesPath = [string]::Equals(
                    $processFullPath,
                    $targetFullPath,
                    [System.StringComparison]::OrdinalIgnoreCase
                ) -or $processFullPath.StartsWith($binDirPrefix, [System.StringComparison]::OrdinalIgnoreCase)
            }
            catch {
            }
        }

        $matchesName = $process.ProcessName -eq 'multi-pwsh' -or $process.ProcessName -like 'pwsh*'
        if (-not $matchesPath -and -not $matchesName) {
            continue
        }

        $results.Add([pscustomobject]@{
                Id = $process.Id
                Name = $process.ProcessName
                Path = if ($processPath) { $processPath } else { '' }
            })
    }

    return $results |
        Sort-Object Name, Id -Unique |
        Where-Object { $_.Id -ne $PID }
}

function Copy-BinaryWithPrompt {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourcePath,

        [Parameter(Mandatory = $true)]
        [string]$TargetPath,

        [Parameter(Mandatory = $true)]
        [string]$BinDir
    )

    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

    $copyAction = {
        Copy-Item -LiteralPath $SourcePath -Destination $TargetPath -Force
        if (-not $IsWindows) {
            & chmod 0755 $TargetPath
        }
    }

    try {
        & $copyAction
        return
    }
    catch {
        $copyError = $_
        $blockingProcesses = @(Get-CandidateBlockingProcesses -BinDir $BinDir -TargetPath $TargetPath)
        if ($blockingProcesses.Count -eq 0) {
            throw
        }

        Write-Warning 'Copy failed. Running PowerShell or multi-pwsh processes may be locking the installed binary.'
        $blockingProcesses | Format-Table -AutoSize | Out-Host
        $answer = Read-Host 'Kill these processes and retry the copy? [y/N]'
        if ($answer -notmatch '^(?i:y|yes)$') {
            throw $copyError
        }

        foreach ($blockingProcess in $blockingProcesses) {
            Stop-Process -Id $blockingProcess.Id -Force -ErrorAction Stop
        }

        Start-Sleep -Milliseconds 250
        & $copyAction
    }
}

$repositoryRoot = Get-RepositoryRoot
$binaryName = Get-BinaryName
$binDir = Get-InstallBinDir
$builtBinaryPath = Get-BuiltBinaryPath -RepositoryRoot $repositoryRoot -ConfigurationName $Configuration
$targetPath = Join-Path $binDir $binaryName

if (-not $SkipBuild) {
    Build-MultiPwshBinary -RepositoryRoot $repositoryRoot -ConfigurationName $Configuration
}

if (-not (Test-Path -LiteralPath $builtBinaryPath -PathType Leaf)) {
    throw "Built binary was not found at $builtBinaryPath"
}

if ($PSCmdlet.ShouldProcess($targetPath, "Install locally built multi-pwsh")) {
    Copy-BinaryWithPrompt -SourcePath $builtBinaryPath -TargetPath $targetPath -BinDir $binDir
    Write-Host "Installed $binaryName to $targetPath"
}