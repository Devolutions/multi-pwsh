[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Contains {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Text,

        [Parameter(Mandatory = $true)]
        [string]$Expected,

        [Parameter(Mandatory = $true)]
        [string]$Context
    )

    if (-not $Text.Contains($Expected)) {
        throw "Expected $Context to contain '$Expected'.`nActual output:`n$Text"
    }
}

function Invoke-MultiPwsh {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,

        [switch]$UseMachinePrivileges,

        [int[]]$AllowedExitCodes = @(0)
    )

    $description = "multi-pwsh $($Arguments -join ' ')"

    if ($UseMachinePrivileges -and -not $IsWindows) {
        $output = & sudo env `
            "PATH=${env:PATH}" `
            "GITHUB_TOKEN=${env:GITHUB_TOKEN}" `
            "MULTI_PWSH_CACHE_DIR=${env:MULTI_PWSH_CACHE_DIR}" `
            "MULTI_PWSH_CACHE_KEEP=${env:MULTI_PWSH_CACHE_KEEP}" `
            $script:MultiPwshExe @Arguments 2>&1 | Out-String
    }
    else {
    $output = & $script:MultiPwshExe @Arguments 2>&1 | Out-String
    }

    $exitCode = $LASTEXITCODE
    $output = $output.Trim()

    if ($AllowedExitCodes -notcontains $exitCode) {
        throw "Command failed with exit code ${exitCode}: $description`n$output"
    }

    $global:LASTEXITCODE = 0

    [pscustomobject]@{
        ExitCode = $exitCode
        Output   = $output
    }
}

function Invoke-PwshAlias {
    param(
        [Parameter(Mandatory = $true)]
        [string]$AliasPath
    )

    $output = & $AliasPath -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()' 2>&1 | Out-String
    $exitCode = $LASTEXITCODE
    $output = $output.Trim()

    if ($exitCode -ne 0) {
        throw "Alias invocation failed with exit code ${exitCode}: $AliasPath`n$output"
    }

    $global:LASTEXITCODE = 0

    $output
}

function Get-UserScopeRoot {
    if ($IsWindows) {
        Join-Path $env:LOCALAPPDATA "PowerShell"
    }
    else {
        Join-Path $HOME ".pwsh"
    }
}

function Get-UserScopeBin {
    Join-Path (Get-UserScopeRoot) "bin"
}

function Get-MachineScopeRoot {
    if ($IsWindows) {
        Join-Path $env:ProgramFiles "PowerShell"
    }
    elseif ($IsMacOS) {
        "/usr/local/microsoft/powershell"
    }
    else {
        "/opt/microsoft/powershell"
    }
}

function Get-MachineScopeBin {
    if ($IsWindows) {
        Join-Path (Get-MachineScopeRoot) "bin"
    }
    else {
        "/usr/local/bin"
    }
}

function Get-AliasPath {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("user", "machine")]
        [string]$Scope,

        [Parameter(Mandatory = $true)]
        [string]$AliasName
    )

    $binDir = if ($Scope -eq "user") { Get-UserScopeBin } else { Get-MachineScopeBin }
    $fileName = if ($IsWindows) { "$AliasName.exe" } else { $AliasName }
    Join-Path $binDir $fileName
}

function Assert-ListContainsVersions {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Output,

        [Parameter(Mandatory = $true)]
        [string]$Context
    )

    foreach ($expected in @("7.4", "7.5", "7.6")) {
        Assert-Contains -Text $Output -Expected $expected -Context $Context
    }
}

$script:MultiPwshExe = (Get-Command multi-pwsh -CommandType Application).Source
Assert-True -Condition ([string]::IsNullOrWhiteSpace($script:MultiPwshExe) -eq $false) -Message "multi-pwsh is not installed on PATH"

$userRoot = Get-UserScopeRoot
$userBin = Get-UserScopeBin
$machineRoot = Get-MachineScopeRoot
$machineBin = Get-MachineScopeBin

Write-Host "multi-pwsh executable: ${script:MultiPwshExe}"
Write-Host "User root: $userRoot"
Write-Host "User bin: $userBin"
Write-Host "Machine root: $machineRoot"
Write-Host "Machine bin: $machineBin"

$installMatrix = @(
    @{ Selector = "7.4"; InstallArgs = @(); ExpectedPrefix = "7.4." },
    @{ Selector = "7.5"; InstallArgs = @(); ExpectedPrefix = "7.5." },
    @{ Selector = "7.6"; InstallArgs = @("--include-prerelease"); ExpectedPrefix = "7.6." }
)

foreach ($scope in @("user", "machine")) {
    $useMachinePrivileges = ($scope -eq "machine" -and -not $IsWindows)

    foreach ($install in $installMatrix) {
        $args = @("install", $install.Selector, "--scope", $scope) + $install.InstallArgs
        if ($scope -eq "machine" -and $IsWindows) {
            $args += @("--no-register-manifest")
        }
        $result = Invoke-MultiPwsh -Arguments $args -UseMachinePrivileges:$useMachinePrivileges
        Write-Host $result.Output
    }
}

$userList = Invoke-MultiPwsh -Arguments @("list", "--scope", "user")
$machineList = Invoke-MultiPwsh -Arguments @("list", "--scope", "machine") -UseMachinePrivileges:(-not $IsWindows)
$allList = Invoke-MultiPwsh -Arguments @("list", "--scope", "all")

Assert-Contains -Text $userList.Output -Expected $userRoot -Context "user scope list"
Assert-Contains -Text $userList.Output -Expected $userBin -Context "user scope list"
Assert-ListContainsVersions -Output $userList.Output -Context "user scope list"

Assert-Contains -Text $machineList.Output -Expected $machineRoot -Context "machine scope list"
Assert-Contains -Text $machineList.Output -Expected $machineBin -Context "machine scope list"
Assert-ListContainsVersions -Output $machineList.Output -Context "machine scope list"

Assert-Contains -Text $allList.Output -Expected $userRoot -Context "all scope list"
Assert-Contains -Text $allList.Output -Expected $machineRoot -Context "all scope list"
Assert-ListContainsVersions -Output $allList.Output -Context "all scope list"

$resolvedVersions = @{}

foreach ($scope in @("user", "machine")) {
    $sevenSixVersion = $null

    foreach ($install in $installMatrix) {
        $lineAliasName = "pwsh-$($install.Selector)"
        $lineAliasPath = Get-AliasPath -Scope $scope -AliasName $lineAliasName

        Assert-True -Condition (Test-Path -Path $lineAliasPath) -Message "Expected alias to exist: $lineAliasPath"

        $resolvedVersion = Invoke-PwshAlias -AliasPath $lineAliasPath
        Assert-True `
            -Condition ($resolvedVersion.StartsWith($install.ExpectedPrefix)) `
            -Message "Expected $lineAliasPath to resolve to a version starting with $($install.ExpectedPrefix), but got $resolvedVersion"

        $patchAliasName = "pwsh-$resolvedVersion"
        $patchAliasPath = Get-AliasPath -Scope $scope -AliasName $patchAliasName
        Assert-True -Condition (Test-Path -Path $patchAliasPath) -Message "Expected patch alias to exist: $patchAliasPath"

        $patchVersion = Invoke-PwshAlias -AliasPath $patchAliasPath
        Assert-True `
            -Condition ($patchVersion -eq $resolvedVersion) `
            -Message "Expected $patchAliasPath to resolve to $resolvedVersion, but got $patchVersion"

        if ($install.Selector -eq "7.6") {
            $sevenSixVersion = $resolvedVersion
        }

        $resolvedVersions["$scope-$($install.Selector)"] = $resolvedVersion
    }

    $majorAliasPath = Get-AliasPath -Scope $scope -AliasName "pwsh-7"
    Assert-True -Condition (Test-Path -Path $majorAliasPath) -Message "Expected major alias to exist: $majorAliasPath"

    $majorVersion = Invoke-PwshAlias -AliasPath $majorAliasPath
    Assert-True `
        -Condition ($majorVersion -eq $sevenSixVersion) `
        -Message "Expected $majorAliasPath to resolve to $sevenSixVersion, but got $majorVersion"
}

$ambiguousVersion = $resolvedVersions["user-7.4"]
$ambiguousUninstall = Invoke-MultiPwsh -Arguments @("uninstall", $ambiguousVersion) -AllowedExitCodes @(1)
Assert-Contains `
    -Text $ambiguousUninstall.Output `
    -Expected "installed in both user and machine scopes" `
    -Context "ambiguous uninstall output"

Write-Host "Scoped install smoke test completed successfully."
