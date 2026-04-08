param(
    [Parameter(Mandatory = $true)]
    [string]$PwshAlias,

    [Parameter(Mandatory = $true)]
    [string]$VenvName,

    [Parameter(Mandatory = $true)]
    [string]$VenvRoot,

    [Parameter(Mandatory = $false)]
    [bool]$EnableOnlineTests = $false
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Describe "Venv cmdlet behavior for $PwshAlias" {
    BeforeAll {
        function Normalize-PathText {
            param([Parameter(Mandatory = $true)][string]$PathText)

            if ($IsWindows) {
                $normalized = $PathText.Replace('/', '\\').ToLowerInvariant()
                if ($normalized.StartsWith('\\?\\unc\\')) {
                    return '\\' + $normalized.Substring(8)
                }

                if ($normalized.StartsWith('\\?\\') -or $normalized.StartsWith('\\.\\')) {
                    return $normalized.Substring(4)
                }

                return $normalized
            }

            return $PathText.Replace('\\', '/').ToLowerInvariant()
        }

        function Test-PathUnderRoot {
            param(
                [Parameter(Mandatory = $true)][string]$Path,
                [Parameter(Mandatory = $true)][string]$Root
            )

            $normalizedPath = Normalize-PathText -PathText $Path
            $normalizedRoot = Normalize-PathText -PathText $Root
            if (-not $normalizedRoot.EndsWith([IO.Path]::DirectorySeparatorChar) -and -not $normalizedRoot.EndsWith([IO.Path]::AltDirectorySeparatorChar)) {
                $normalizedRoot = $normalizedRoot + [IO.Path]::DirectorySeparatorChar
            }

            return $normalizedPath.StartsWith($normalizedRoot)
        }

        function Get-VenvModulesRoot {
            return (Join-Path $VenvRoot 'Modules')
        }

        function Invoke-InVenv {
            param(
                [Parameter(Mandatory = $true)][string]$CommandText,
                [Parameter(Mandatory = $false)][switch]$AllowNonZeroExit
            )

            $output = & $PwshAlias -venv $VenvName -NoLogo -NoProfile -NonInteractive -Command $CommandText 2>&1
            $exitCode = $LASTEXITCODE
            $outputText = ($output | ForEach-Object { $_.ToString() }) -join "`n"

            if (-not $AllowNonZeroExit -and $exitCode -ne 0) {
                throw "Command failed with exit code $exitCode.`n$outputText"
            }

            return [pscustomobject]@{
                ExitCode = $exitCode
                Output   = $outputText
            }
        }

        function Wait-StartupHookReady {
            $waitCommand = @'
$moduleHookReady = $false
$psResourceHookReady = $false
for ($i = 0; $i -lt 500; $i++) {
    $importCommand = Get-Command Import-Module -ErrorAction SilentlyContinue
    $installedModuleCommand = Get-Command Get-InstalledModule -ErrorAction SilentlyContinue
    $installedPsResourceCommand = Get-Command Get-InstalledPSResource -ErrorAction SilentlyContinue

    if ($importCommand -and $importCommand.CommandType -eq 'Alias' -and $installedModuleCommand -and $installedModuleCommand.CommandType -eq 'Alias') {
        $moduleHookReady = $true
    }

    if ($installedPsResourceCommand -and ($installedPsResourceCommand.CommandType -eq 'Alias' -or $installedPsResourceCommand.CommandType -eq 'Cmdlet')) {
        $psResourceHookReady = $true
    }

    if ($moduleHookReady -and $psResourceHookReady) {
        break
    }

    Start-Sleep -Milliseconds 10
}

if ($moduleHookReady -and $psResourceHookReady) { 'Ready' } else { 'NotReady' }
'@

            $result = Invoke-InVenv -CommandText $waitCommand
            if ($result.Output.Trim() -ne 'Ready') {
                throw "Startup hook aliases did not become ready for '$PwshAlias'. Output: $($result.Output)"
            }
        }

        function New-SyntheticVenvModule {
            param([Parameter(Mandatory = $true)][string]$Root)

            $moduleVersionPath = Join-Path $Root 'PwshHost.VenvProbe/1.0.0'
            New-Item -ItemType Directory -Path $moduleVersionPath -Force | Out-Null

            $manifestPath = Join-Path $moduleVersionPath 'PwshHost.VenvProbe.psd1'
            $modulePath = Join-Path $moduleVersionPath 'PwshHost.VenvProbe.psm1'

            @'
@{
    RootModule = 'PwshHost.VenvProbe.psm1'
    ModuleVersion = '1.0.0'
    GUID = '1f7cca3d-b0dc-4b4e-9fb4-3e4afe6b22e2'
    Author = 'multi-pwsh'
    Description = 'Synthetic venv probe module for startup-hook tests.'
    FunctionsToExport = @('Get-VenvProbe')
    CmdletsToExport = @()
    VariablesToExport = @()
    AliasesToExport = @()
    PrivateData = @{
        PSData = @{
            Tags = @('venv', 'probe')
            ProjectUri = 'https://example.invalid/multi-pwsh'
        }
    }
}
'@ | Set-Content -Path $manifestPath -Encoding utf8

            @'
function Get-VenvProbe {
    'ok'
}
'@ | Set-Content -Path $modulePath -Encoding utf8
        }


        if (-not (Get-Command $PwshAlias -ErrorAction SilentlyContinue)) {
            throw "Alias command '$PwshAlias' was not found on PATH."
        }

        if (-not (Test-Path -LiteralPath $VenvRoot -PathType Container)) {
            throw "Resolved venv root does not exist: $VenvRoot"
        }

        Wait-StartupHookReady
        New-SyntheticVenvModule -Root (Get-VenvModulesRoot)
    }

    It 'sets PSMODULE_VENV_PATH to the venv root' {
        $result = Invoke-InVenv -CommandText 'Write-Output $env:PSMODULE_VENV_PATH'
        Normalize-PathText -PathText $result.Output.Trim() | Should -Be (Normalize-PathText -PathText $VenvRoot)
    }

    It 'prepends the venv Modules path in PSModulePath' {
        $result = Invoke-InVenv -CommandText 'Write-Output $env:PSModulePath'
        $entries = [Environment]::ExpandEnvironmentVariables($result.Output.Trim()).Split([IO.Path]::PathSeparator, [StringSplitOptions]::RemoveEmptyEntries)
        $entries[0] | Should -Not -BeNullOrEmpty
        Normalize-PathText -PathText $entries[0] | Should -Be (Normalize-PathText -PathText (Get-VenvModulesRoot))
    }

    It 'overrides module-management commands in hosted session' {
        $commandQuery = @'
$names = @(
    'Import-Module',
    'Install-Module',
    'Get-InstalledModule',
    'Get-PSRepository',
    'Set-PSRepository',
    'Register-PSRepository',
    'Unregister-PSRepository',
    'Install-PSResource',
    'Get-InstalledPSResource'
)

$results = [ordered]@{}
foreach ($name in $names) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        $results[$name] = $null
        continue
    }

    $results[$name] = [ordered]@{
        Type = $command.CommandType.ToString()
        Definition = $command.Definition
    }
}

$results | ConvertTo-Json -Compress -Depth 5
'@

        $result = Invoke-InVenv -CommandText $commandQuery
        $commands = $result.Output | ConvertFrom-Json -AsHashtable

        foreach ($name in @('Import-Module', 'Install-Module', 'Get-InstalledModule', 'Get-PSRepository', 'Set-PSRepository', 'Register-PSRepository', 'Unregister-PSRepository', 'Install-PSResource')) {
            $commands[$name] | Should -Not -BeNullOrEmpty
            $commands[$name].Type | Should -Be 'Alias'
        }

        $commands['Get-InstalledPSResource'] | Should -Not -BeNullOrEmpty
        @('Alias', 'Cmdlet') | Should -Contain $commands['Get-InstalledPSResource'].Type
    }

    It 'finds and imports synthetic module from the venv' {
        $listQuery = "Get-Module -ListAvailable PwshHost.VenvProbe | Select-Object -First 1 -ExpandProperty ModuleBase"
        $listResult = Invoke-InVenv -CommandText $listQuery
        $moduleBase = $listResult.Output.Trim()

        $moduleBase | Should -Not -BeNullOrEmpty
        (Test-PathUnderRoot -Path $moduleBase -Root (Get-VenvModulesRoot)) | Should -BeTrue

        $importQuery = "Import-Module PwshHost.VenvProbe -ErrorAction Stop; Get-VenvProbe"
        $importResult = Invoke-InVenv -CommandText $importQuery
        $importResult.Output.Trim() | Should -Be 'ok'
    }

    It 'keeps Get-InstalledModule scoped to venv content' {
        $query = "Import-Module PowerShellGet -ErrorAction Stop; Get-InstalledModule PwshHost.VenvProbe -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation"
        $result = Invoke-InVenv -CommandText $query
        $installedLocation = $result.Output.Trim()

        $installedLocation | Should -Not -BeNullOrEmpty
        (Test-PathUnderRoot -Path $installedLocation -Root (Get-VenvModulesRoot)) | Should -BeTrue
    }

    It 'keeps Get-InstalledPSResource scoped to venv content' {
        $query = "Get-InstalledPSResource PwshHost.VenvProbe -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation"
        $result = Invoke-InVenv -CommandText $query
        $installedLocation = $result.Output.Trim()

        $installedLocation | Should -Not -BeNullOrEmpty
        (Test-PathUnderRoot -Path $installedLocation -Root (Get-VenvModulesRoot)) | Should -BeTrue
    }

    It 'runs Get-PSRepository without explicit PowerShellGet import' {
        $query = "(Get-PSRepository -Name PSGallery -ErrorAction Stop).Name"
        $result = Invoke-InVenv -CommandText $query
        $result.Output.Trim() | Should -Be 'PSGallery'
    }

    It 'supports Install-PSResource parameters and installs into venv (online)' -Skip:(-not $EnableOnlineTests) {
        $query = @'
$ProgressPreference = 'SilentlyContinue'
Install-PSResource -Name Yayaml -Repository PSGallery -TrustRepository -Quiet -Reinstall -ErrorAction Stop
    $locations = @(Get-Module -ListAvailable Yayaml -ErrorAction Stop | Select-Object -ExpandProperty ModuleBase)
[pscustomobject]@{ Locations = $locations } | ConvertTo-Json -Compress
'@
        $result = Invoke-InVenv -CommandText $query
        $jsonLine = ($result.Output -split "`n" | Where-Object { $_.TrimStart().StartsWith('{') } | Select-Object -Last 1)
        $jsonLine | Should -Not -BeNullOrEmpty
        $payload = ConvertFrom-Json -InputObject $jsonLine
        $installedLocations = @()

        if ($payload.Locations -is [string]) {
            $installedLocations = @($payload.Locations)
        }
        elseif ($null -ne $payload.Locations) {
            $installedLocations = @($payload.Locations | ForEach-Object { $_.ToString() })
        }

        $installedLocations.Count | Should -BeGreaterThan 0
        ($installedLocations | Where-Object { Test-PathUnderRoot -Path $_ -Root (Get-VenvModulesRoot) } | Measure-Object).Count | Should -BeGreaterThan 0
    }

    It 'supports Install-Module into venv (online)' -Skip:(-not $EnableOnlineTests) {
        $query = @'
$ProgressPreference = 'SilentlyContinue'
Install-Module -Name Yayaml -Repository PSGallery -Force -AllowClobber -AcceptLicense -Confirm:$false -ErrorAction Stop
Import-Module PowerShellGet -ErrorAction Stop
$locations = @(Get-InstalledModule Yayaml -ErrorAction Stop | Select-Object -ExpandProperty InstalledLocation)
[pscustomobject]@{ Locations = $locations } | ConvertTo-Json -Compress
'@
        $result = Invoke-InVenv -CommandText $query
        $jsonLine = ($result.Output -split "`n" | Where-Object { $_.TrimStart().StartsWith('{') } | Select-Object -Last 1)
        $jsonLine | Should -Not -BeNullOrEmpty
        $payload = ConvertFrom-Json -InputObject $jsonLine
        $installedLocations = @()

        if ($payload.Locations -is [string]) {
            $installedLocations = @($payload.Locations)
        }
        elseif ($null -ne $payload.Locations) {
            $installedLocations = @($payload.Locations | ForEach-Object { $_.ToString() })
        }

        $installedLocations.Count | Should -BeGreaterThan 0
        ($installedLocations | Where-Object { Test-PathUnderRoot -Path $_ -Root (Get-VenvModulesRoot) } | Measure-Object).Count | Should -BeGreaterThan 0
    }
}
