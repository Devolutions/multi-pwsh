using System;

public static partial class StartupHook
{
    private static string BuildPowerShellGetPatchHelperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding()]
param()

$forcedModulePath = '{{escapedForcedModulePath}}'
$module = Microsoft.PowerShell.Core\Get-Module PowerShellGet -ErrorAction SilentlyContinue | Microsoft.PowerShell.Utility\Select-Object -First 1
if (-not $module) {
    $smaAssemblyDirectory = Microsoft.PowerShell.Management\Split-Path -Parent ([System.Management.Automation.PSObject].Assembly.Location)
    $bundledPackageManagementManifest = Microsoft.PowerShell.Management\Join-Path $smaAssemblyDirectory 'Modules/PackageManagement/PackageManagement.psd1'
    $bundledPowerShellGetManifest = Microsoft.PowerShell.Management\Join-Path $smaAssemblyDirectory 'Modules/PowerShellGet/PowerShellGet.psd1'
    if (-not (Microsoft.PowerShell.Core\Get-Module PackageManagement -ErrorAction SilentlyContinue | Microsoft.PowerShell.Utility\Select-Object -First 1)) {
        if (Microsoft.PowerShell.Management\Test-Path $bundledPackageManagementManifest -PathType Leaf) {
            Microsoft.PowerShell.Core\Import-Module $bundledPackageManagementManifest -Scope Local -ErrorAction Stop | Out-Null
        }
    }
    if (Microsoft.PowerShell.Management\Test-Path $bundledPowerShellGetManifest -PathType Leaf) {
        Microsoft.PowerShell.Core\Import-Module $bundledPowerShellGetManifest -Scope Local -ErrorAction Stop | Out-Null
    }
    else {
        Microsoft.PowerShell.Core\Import-Module PowerShellGet -Scope Local -ErrorAction Stop | Out-Null
    }
    $module = Microsoft.PowerShell.Core\Get-Module PowerShellGet -ErrorAction Stop | Microsoft.PowerShell.Utility\Select-Object -First 1
}
$sessionState = $module.SessionState
$currentUserModules = $sessionState.PSVariable.GetValue('MyDocumentsModulesPath')
if ([string]::Equals($currentUserModules, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase)) {
    return
}

$programFilesModulesPath = $sessionState.PSVariable.GetValue('ProgramFilesModulesPath')
$programFilesScriptsPath = $sessionState.PSVariable.GetValue('ProgramFilesScriptsPath')
$sessionState.PSVariable.Set('MyDocumentsModulesPath', $forcedModulePath)
$sessionState.PSVariable.Set('MyDocumentsScriptsPath', $forcedModulePath)
$sessionState.PSVariable.Set('PSGetPath', [pscustomobject]@{
    AllUsersModules = $programFilesModulesPath
    AllUsersScripts = $programFilesScriptsPath
    CurrentUserModules = $forcedModulePath
    CurrentUserScripts = $forcedModulePath
    PSTypeName = 'Microsoft.PowerShell.Commands.PSGetPath'
})
$sessionState.PSVariable.Set('PSGetInstalledModules', $null)
& $module {
    param($forcedModulePath)

    function Test-ModuleInstalled
    {
        [CmdletBinding(PositionalBinding=$false)]
        [OutputType('PSModuleInfo')]
        Param
        (
            [Parameter(Mandatory=$true)]
            [ValidateNotNullOrEmpty()]
            [string]
            $Name,

            [Parameter()]
            [string]
            $RequiredVersion
        )

        $forcedModulePathPrefix = $forcedModulePath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
        $availableModule = Microsoft.PowerShell.Core\Get-Module -ListAvailable -Name $Name -Verbose:$false |
            Microsoft.PowerShell.Core\Where-Object {
                $moduleBase = $_.ModuleBase
                $moduleBase -and (
                    [string]::Equals($moduleBase, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase) -or
                    $moduleBase.StartsWith($forcedModulePathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
                )
            } |
            Microsoft.PowerShell.Core\Where-Object {
                -not (Test-ModuleSxSVersionSupport) `
                -or (-not $RequiredVersion) `
                -or ($RequiredVersion.Trim() -eq $_.Version.ToString()) `
                -or (Test-ItemPrereleaseVersionRequirements -Version $_.Version -RequiredVersion $RequiredVersion)
            } |
            Microsoft.PowerShell.Utility\Select-Object -Unique -First 1 -ErrorAction Ignore

        return $availableModule
    }
} $forcedModulePath
""";
    }

    private static string BuildVenvInstalledModuleHelperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding()]
param(
    [Parameter(Position=0)]
    [string[]]
    ${Name},

    [string]
    ${MinimumVersion},

    [string]
    ${RequiredVersion},

    [string]
    ${MaximumVersion},

    [switch]
    ${AllVersions}
)

$forcedModulePath = '{{escapedForcedModulePath}}'
$forcedModulePathPrefix = $forcedModulePath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
$candidateModules = @()
if ($Name -and $Name.Count -gt 0) {
    foreach ($moduleName in $Name) {
        $candidateModules += Microsoft.PowerShell.Core\Get-Module -ListAvailable -Name $moduleName -Verbose:$false -ErrorAction SilentlyContinue
    }
}
else {
    $candidateModules = @(Microsoft.PowerShell.Core\Get-Module -ListAvailable -Verbose:$false -ErrorAction SilentlyContinue)
}

$filteredModules = $candidateModules |
    Microsoft.PowerShell.Core\Where-Object {
        $moduleBase = $_.ModuleBase
        $moduleBase -and (
            [string]::Equals($moduleBase, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase) -or
            $moduleBase.StartsWith($forcedModulePathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
        )
    } |
    Microsoft.PowerShell.Core\Where-Object {
        (-not $RequiredVersion -or $_.Version.ToString() -eq $RequiredVersion) -and
        (-not $MinimumVersion -or $_.Version -ge [version]$MinimumVersion) -and
        (-not $MaximumVersion -or $_.Version -le [version]$MaximumVersion)
    }

if (-not $AllVersions) {
    $filteredModules = $filteredModules |
        Microsoft.PowerShell.Utility\Sort-Object Name, Version -Descending |
        Microsoft.PowerShell.Utility\Group-Object Name |
        Microsoft.PowerShell.Core\ForEach-Object { $_.Group | Microsoft.PowerShell.Utility\Select-Object -First 1 }
}

$filteredModules |
    Microsoft.PowerShell.Utility\Sort-Object Name, Version -Descending |
    Microsoft.PowerShell.Core\ForEach-Object {
        $installedDate = $null
        try {
            $installedDate = (Microsoft.PowerShell.Management\Get-Item $_.ModuleBase -ErrorAction Stop).LastWriteTime
        }
        catch {
        }

        $result = [pscustomobject]@{
            Name = $_.Name
            Version = $_.Version.ToString()
            Repository = $null
            Description = $_.Description
            InstalledLocation = $_.ModuleBase
            InstalledDate = $installedDate
            Type = 'Module'
        }
        $result.PSTypeNames.Insert(0, 'Microsoft.PowerShell.Commands.PSRepositoryItemInfo')
        $result
    }
""";
    }

    private static string BuildInstallModuleWrapperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding(DefaultParameterSetName='NameParameterSet', SupportsShouldProcess=$true, ConfirmImpact='Medium', HelpUri='https://go.microsoft.com/fwlink/?LinkID=398573')]
param(
    [Parameter(ParameterSetName='NameParameterSet', Mandatory=$true, Position=0, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNullOrEmpty()]
    [string[]]
    ${Name},

    [Parameter(ParameterSetName='InputObject', Mandatory=$true, Position=0, ValueFromPipeline=$true, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [psobject[]]
    ${InputObject},

    [Parameter(ParameterSetName='NameParameterSet', ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${MinimumVersion},

    [Parameter(ParameterSetName='NameParameterSet', ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${MaximumVersion},

    [Parameter(ParameterSetName='NameParameterSet', ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${RequiredVersion},

    [Parameter(ParameterSetName='NameParameterSet')]
    [ValidateNotNullOrEmpty()]
    [string[]]
    ${Repository},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [pscredential]
    [System.Management.Automation.CredentialAttribute()]
    ${Credential},

    [ValidateSet('CurrentUser','AllUsers')]
    [string]
    ${Scope},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNullOrEmpty()]
    [uri]
    ${Proxy},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [pscredential]
    [System.Management.Automation.CredentialAttribute()]
    ${ProxyCredential},

    [switch]
    ${AllowClobber},

    [switch]
    ${SkipPublisherCheck},

    [switch]
    ${Force},

    [Parameter(ParameterSetName='NameParameterSet')]
    [switch]
    ${AllowPrerelease},

    [switch]
    ${AcceptLicense},

    [switch]
    ${PassThru})

begin
{
    & {{PowerShellGetPatchHelperName}}
    $forcedModulePath = '{{escapedForcedModulePath}}'
}

process
{
    PowerShellGet\Install-Module @PSBoundParameters

    if ($PSCmdlet.ParameterSetName -eq 'NameParameterSet') {
        $missingNames = @()
        foreach ($moduleName in $Name) {
            $installedInVenv = & {{VenvInstalledModuleHelperName}} -Name $moduleName -RequiredVersion $RequiredVersion -MinimumVersion $MinimumVersion -MaximumVersion $MaximumVersion
            if (-not $installedInVenv) {
                $missingNames += $moduleName
            }
        }

        $savePerformed = $false
        if ($missingNames.Count -gt 0) {
            $saveParameters = @{
                Name = $missingNames
                Path = $forcedModulePath
                Force = $true
                ErrorAction = 'Stop'
            }

            foreach ($parameterName in 'MinimumVersion', 'MaximumVersion', 'RequiredVersion', 'Credential', 'Proxy', 'ProxyCredential', 'Repository') {
                if ($PSBoundParameters.ContainsKey($parameterName)) {
                    $saveParameters[$parameterName] = $PSBoundParameters[$parameterName]
                }
            }

            if ($PSBoundParameters.ContainsKey('AllowPrerelease')) {
                $saveParameters['AllowPrerelease'] = $true
            }

            PowerShellGet\Save-Module @saveParameters | Microsoft.PowerShell.Core\Out-Null
            $savePerformed = $true
        }

        if ($PassThru -and $savePerformed) {
            & {{VenvInstalledModuleHelperName}} -Name $Name -RequiredVersion $RequiredVersion -MinimumVersion $MinimumVersion -MaximumVersion $MaximumVersion
        }
    }
}
<##

.ForwardHelpTargetName Install-Module
.ForwardHelpCategory Function

#>
""";
    }

    private static string BuildVenvInstalledPSResourceHelperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding()]
param(
    [Parameter(Position=0)]
    [string[]]
    ${Name},

    [string]
    ${Version},

    [string]
    ${Path}
)

Microsoft.PowerShell.Core\Import-Module Microsoft.PowerShell.PSResourceGet -Scope Local -ErrorAction Stop | Out-Null
$targetPath = if ($Path) { $Path } else { '{{escapedForcedModulePath}}' }
$targetPathPrefix = $targetPath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
$candidateManifestPaths = @()
if ($Name -and $Name.Count -gt 0) {
    foreach ($moduleName in $Name) {
        $moduleRoot = Microsoft.PowerShell.Management\Join-Path $targetPath $moduleName
        if (Microsoft.PowerShell.Management\Test-Path $moduleRoot -PathType Container) {
            $candidateManifestPaths += Microsoft.PowerShell.Management\Get-ChildItem -Path $moduleRoot -Filter *.psd1 -File -Recurse -ErrorAction SilentlyContinue
        }
    }
}
else {
    $candidateManifestPaths = @(Microsoft.PowerShell.Management\Get-ChildItem -Path $targetPath -Filter *.psd1 -File -Recurse -ErrorAction SilentlyContinue)
}

$filteredModules = $candidateManifestPaths |
    Microsoft.PowerShell.Utility\Sort-Object FullName -Unique |
    Microsoft.PowerShell.Core\Where-Object {
        $moduleBase = Microsoft.PowerShell.Management\Split-Path $_.FullName -Parent
        $moduleBase -and (
            [string]::Equals($moduleBase, $targetPath, [System.StringComparison]::OrdinalIgnoreCase) -or
            $moduleBase.StartsWith($targetPathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
        )
    } |
    Microsoft.PowerShell.Core\ForEach-Object {
        try {
            Microsoft.PowerShell.Core\Test-ModuleManifest -Path $_.FullName -ErrorAction Stop
        }
        catch {
        }
    } |
    Microsoft.PowerShell.Core\Where-Object {
        (-not $Name -or $Name.Count -eq 0 -or $_.Name -in $Name) -and
        (-not $Version -or $_.Version.ToString() -eq $Version)
    } |
    Microsoft.PowerShell.Utility\Sort-Object Name, Version -Descending |
    Microsoft.PowerShell.Utility\Group-Object Path |
    Microsoft.PowerShell.Core\ForEach-Object { $_.Group | Microsoft.PowerShell.Utility\Select-Object -First 1 }

$filteredModules |
    Microsoft.PowerShell.Utility\Sort-Object Name, Version -Descending |
    Microsoft.PowerShell.Core\ForEach-Object {
        $privateData = $null
        if ($_.PrivateData -and $_.PrivateData.PSData) {
            $privateData = $_.PrivateData.PSData
        }

        $installedDate = $null
        try {
            $installedDate = (Microsoft.PowerShell.Management\Get-Item $_.ModuleBase -ErrorAction Stop).LastWriteTime
        }
        catch {
        }

        $tags = @()
        if ($privateData -and $privateData.Tags) {
            $tags = @($privateData.Tags)
        }
        elseif ($_.Tags) {
            $tags = @($_.Tags)
        }

        $prerelease = ''
        if ($privateData -and $privateData.Prerelease) {
            $prerelease = [string]$privateData.Prerelease
        }

        $projectUri = $null
        if ($privateData -and $privateData.ProjectUri) {
            $projectUri = $privateData.ProjectUri
        }
        elseif ($_.ProjectUri) {
            $projectUri = $_.ProjectUri
        }

        $resource = [pscustomobject]@{
            AdditionalMetadata = $null
            Author = $_.Author
            CompanyName = $_.CompanyName
            Copyright = $_.Copyright
            Dependencies = $_.RequiredModules
            Description = $_.Description
            IconUri = if ($privateData) { $privateData.IconUri } else { $null }
            Includes = $null
            InstalledDate = $installedDate
            InstalledLocation = $targetPath
            IsPrerelease = -not [string]::IsNullOrEmpty($prerelease)
            LicenseUri = if ($privateData) { $privateData.LicenseUri } else { $null }
            Name = $_.Name
            Prerelease = $prerelease
            ProjectUri = $projectUri
            PublishedDate = $null
            ReleaseNotes = if ($privateData) { $privateData.ReleaseNotes } else { $null }
            Repository = $null
            RepositorySourceLocation = $null
            Tags = $tags
            Type = [Microsoft.PowerShell.PSResourceGet.UtilClasses.ResourceType]::Module
            UpdatedDate = $null
            Version = $_.Version
        }
        $resource.PSTypeNames.Insert(0, 'Microsoft.PowerShell.PSResourceGet.UtilClasses.PSResourceInfo')
        $resource
    }
""";
    }

    private static string BuildInstallPSResourceWrapperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding(DefaultParameterSetName='NameParameterSet', SupportsShouldProcess=$true, ConfirmImpact='Medium')]
param(
    [Parameter(ParameterSetName='NameParameterSet', Mandatory=$true, Position=0, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNullOrEmpty()]
    [string[]]
    ${Name},

    [Parameter(ParameterSetName='InputObjectParameterSet', Mandatory=$true, Position=0, ValueFromPipeline=$true, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [psobject[]]
    ${InputObject},

    [string]
    ${Version},

    [string[]]
    ${Repository},

    [pscredential]
    [System.Management.Automation.CredentialAttribute()]
    ${Credential},

    [ValidateSet('CurrentUser','AllUsers')]
    [string]
    ${Scope},

    [switch]
    ${TrustRepository},

    [switch]
    ${Quiet},

    [switch]
    ${Prerelease},

    [switch]
    ${Reinstall},

    [switch]
    ${NoClobber},

    [switch]
    ${AcceptLicense},

    [switch]
    ${PassThru},

    [switch]
    ${SkipDependencyCheck},

    [switch]
    ${AuthenticodeCheck},

    [string]
    ${TemporaryPath},

    [object]
    ${RequiredResource},

    [string]
    ${RequiredResourceFile})

begin
{
    Microsoft.PowerShell.Core\Import-Module Microsoft.PowerShell.PSResourceGet -Scope Local -ErrorAction Stop | Out-Null
    $forcedModulePath = '{{escapedForcedModulePath}}'
    $forcedModulePathPrefix = $forcedModulePath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
}

process
{
    $nativeResults = @(Microsoft.PowerShell.PSResourceGet\Install-PSResource @PSBoundParameters)
    $savePerformed = $false
    $savedNames = @()

    if ($PSCmdlet.ParameterSetName -eq 'NameParameterSet') {
        $moduleNamesToSave = @()
        foreach ($resourceName in $Name | Microsoft.PowerShell.Utility\Select-Object -Unique) {
            if (& {{VenvInstalledPSResourceHelperName}} -Name $resourceName -Version $Version -Path $forcedModulePath) {
                continue
            }

            $candidate = $null
            try {
                $findParameters = @{
                    Name = $resourceName
                    ErrorAction = 'Stop'
                }

                foreach ($parameterName in 'Version', 'Repository', 'Credential') {
                    if ($PSBoundParameters.ContainsKey($parameterName)) {
                        $findParameters[$parameterName] = $PSBoundParameters[$parameterName]
                    }
                }

                if ($PSBoundParameters.ContainsKey('Prerelease')) {
                    $findParameters['Prerelease'] = $true
                }

                $candidate = @(Microsoft.PowerShell.PSResourceGet\Find-PSResource @findParameters | Microsoft.PowerShell.Utility\Select-Object -First 1)
            }
            catch {
            }

            if ($candidate.Count -gt 0) {
                $candidateType = [string]$candidate[0].Type
                if ([string]::Equals($candidateType, 'Module', [System.StringComparison]::OrdinalIgnoreCase) -or $candidateType -eq '1') {
                    $moduleNamesToSave += $resourceName
                }
            }
        }

        if ($moduleNamesToSave.Count -gt 0) {
            $saveParameters = @{
                Name = $moduleNamesToSave
                Path = $forcedModulePath
                ErrorAction = 'Stop'
            }

            foreach ($parameterName in 'Version', 'Repository', 'Credential', 'AuthenticodeCheck', 'AcceptLicense', 'Prerelease', 'Quiet', 'SkipDependencyCheck', 'TemporaryPath', 'TrustRepository', 'WhatIf', 'Confirm') {
                if ($PSBoundParameters.ContainsKey($parameterName)) {
                    $saveParameters[$parameterName] = $PSBoundParameters[$parameterName]
                }
            }

            Microsoft.PowerShell.PSResourceGet\Save-PSResource @saveParameters | Microsoft.PowerShell.Core\Out-Null
            $savePerformed = $true
            $savedNames = $moduleNamesToSave
        }
    }
    elseif ($PSCmdlet.ParameterSetName -eq 'InputObjectParameterSet') {
        $moduleInputsToSave = @()
        foreach ($resource in $InputObject) {
            if (-not $resource) {
                continue
            }

            $nameProperty = $resource.PSObject.Properties.Match('Name')
            if ($nameProperty.Count -eq 0) {
                continue
            }

            $resourceName = [string]$resource.Name
            if ([string]::IsNullOrWhiteSpace($resourceName)) {
                continue
            }

            if (& {{VenvInstalledPSResourceHelperName}} -Name $resourceName -Version $Version -Path $forcedModulePath) {
                continue
            }

            $typeProperty = $resource.PSObject.Properties.Match('Type')
            if ($typeProperty.Count -eq 0) {
                continue
            }

            $resourceType = [string]$resource.Type
            if (-not [string]::Equals($resourceType, 'Module', [System.StringComparison]::OrdinalIgnoreCase) -and $resourceType -ne '1') {
                continue
            }

            $moduleInputsToSave += $resource
            $savedNames += $resourceName
        }

        if ($moduleInputsToSave.Count -gt 0) {
            $saveParameters = @{
                InputObject = $moduleInputsToSave
                Path = $forcedModulePath
                ErrorAction = 'Stop'
            }

            foreach ($parameterName in 'Repository', 'Credential', 'AuthenticodeCheck', 'AcceptLicense', 'Prerelease', 'Quiet', 'SkipDependencyCheck', 'TemporaryPath', 'TrustRepository', 'WhatIf', 'Confirm') {
                if ($PSBoundParameters.ContainsKey($parameterName)) {
                    $saveParameters[$parameterName] = $PSBoundParameters[$parameterName]
                }
            }

            Microsoft.PowerShell.PSResourceGet\Save-PSResource @saveParameters | Microsoft.PowerShell.Core\Out-Null
            $savePerformed = $true
        }
    }

    if ($PassThru) {
        $filteredNativeResults = @($nativeResults | Microsoft.PowerShell.Core\Where-Object {
            $installedLocation = $null
            if ($_.PSObject.Properties.Match('InstalledLocation').Count -gt 0) {
                $installedLocation = $_.InstalledLocation
            }

            -not $installedLocation -or
            [string]::Equals($installedLocation, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase) -or
            $installedLocation.StartsWith($forcedModulePathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
        })

        $fallbackResults = @()
        if ($savePerformed) {
            $requestedNames = if ($savedNames.Count -gt 0) { $savedNames | Microsoft.PowerShell.Utility\Select-Object -Unique } else { $Name }
            $fallbackResults = @(& {{VenvInstalledPSResourceHelperName}} -Name $requestedNames -Version $Version -Path $forcedModulePath)
        }

        if ($filteredNativeResults.Count -gt 0 -and $fallbackResults.Count -gt 0) {
            $reportedKeys = $filteredNativeResults | Microsoft.PowerShell.Core\ForEach-Object {
                $_.Name.ToString() + '|' + $_.Version.ToString() + '|' + $_.InstalledLocation
            }
            $fallbackResults = $fallbackResults | Microsoft.PowerShell.Core\Where-Object {
                ($_.Name.ToString() + '|' + $_.Version.ToString() + '|' + $_.InstalledLocation) -notin $reportedKeys
            }
        }

        $filteredNativeResults
        $fallbackResults
    }
}
<##

.ForwardHelpTargetName Install-PSResource
.ForwardHelpCategory Function

#>
""";
    }

    private static string BuildGetInstalledPSResourceWrapperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding()]
param(
    [Parameter(Position=0, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNullOrEmpty()]
    [string[]]
    ${Name},

    [string]
    ${Version},

    [ValidateSet('CurrentUser','AllUsers')]
    [string]
    ${Scope},

    [string]
    ${Path}
)

begin
{
    Microsoft.PowerShell.Core\Import-Module Microsoft.PowerShell.PSResourceGet -Scope Local -ErrorAction Stop | Out-Null
    $forcedModulePath = '{{escapedForcedModulePath}}'
    $targetPath = if ($PSBoundParameters.ContainsKey('Path') -and -not [string]::IsNullOrWhiteSpace($Path)) { $Path } else { $forcedModulePath }
    $targetPathPrefix = $targetPath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
}

process
{
    if (-not [string]::Equals($targetPath, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase)) {
        Microsoft.PowerShell.PSResourceGet\Get-InstalledPSResource @PSBoundParameters
        return
    }

    $nativeParameters = @{}
    foreach ($entry in $PSBoundParameters.GetEnumerator()) {
        if ($entry.Key -ne 'Scope' -and $entry.Key -ne 'ErrorAction') {
            $nativeParameters[$entry.Key] = $entry.Value
        }
    }
    $nativeParameters['Path'] = $forcedModulePath
    $nativeParameters['ErrorAction'] = 'SilentlyContinue'

    $nativeResults = @(Microsoft.PowerShell.PSResourceGet\Get-InstalledPSResource @nativeParameters | Microsoft.PowerShell.Core\Where-Object {
        $installedLocation = $_.InstalledLocation
        $installedLocation -and (
            [string]::Equals($installedLocation, $targetPath, [System.StringComparison]::OrdinalIgnoreCase) -or
            $installedLocation.StartsWith($targetPathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
        )
    })

    $fallbackResults = @(& {{VenvInstalledPSResourceHelperName}} -Name $Name -Version $Version -Path $targetPath)
    if ($nativeResults.Count -gt 0 -and $fallbackResults.Count -gt 0) {
        $reportedKeys = $nativeResults | Microsoft.PowerShell.Core\ForEach-Object {
            $_.Name.ToString() + '|' + $_.Version.ToString() + '|' + $_.InstalledLocation
        }
        $fallbackResults = $fallbackResults | Microsoft.PowerShell.Core\Where-Object {
            ($_.Name.ToString() + '|' + $_.Version.ToString() + '|' + $_.InstalledLocation) -notin $reportedKeys
        }
    }

    $nativeResults
    $fallbackResults
}
<##

.ForwardHelpTargetName Get-InstalledPSResource
.ForwardHelpCategory Function

#>
""";
    }

    private static string BuildGetInstalledModuleWrapperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding(HelpUri='https://go.microsoft.com/fwlink/?LinkId=526863')]
param(
    [Parameter(Position=0, ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNullOrEmpty()]
    [string[]]
    ${Name},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${MinimumVersion},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${RequiredVersion},

    [Parameter(ValueFromPipelineByPropertyName=$true)]
    [ValidateNotNull()]
    [string]
    ${MaximumVersion},

    [switch]
    ${AllVersions},

    [switch]
    ${AllowPrerelease})

begin
{
    & {{PowerShellGetPatchHelperName}}
    $forcedModulePath = '{{escapedForcedModulePath}}'
    $forcedModulePathPrefix = $forcedModulePath.TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)) + [System.IO.Path]::DirectorySeparatorChar
    $nativeParameters = @{}
    foreach ($entry in $PSBoundParameters.GetEnumerator()) {
        if ($entry.Key -ne 'ErrorAction') {
            $nativeParameters[$entry.Key] = $entry.Value
        }
    }
    $nativeParameters['ErrorAction'] = 'SilentlyContinue'
}

process
{
    $powerShellGetResults = @(PowerShellGet\Get-InstalledModule @nativeParameters | Microsoft.PowerShell.Core\Where-Object {
        $installedLocation = $_.InstalledLocation
        $installedLocation -and (
            [string]::Equals($installedLocation, $forcedModulePath, [System.StringComparison]::OrdinalIgnoreCase) -or
            $installedLocation.StartsWith($forcedModulePathPrefix, [System.StringComparison]::OrdinalIgnoreCase)
        )
    })

    $fallbackResults = @(& {{VenvInstalledModuleHelperName}} -Name $Name -RequiredVersion $RequiredVersion -MinimumVersion $MinimumVersion -MaximumVersion $MaximumVersion -AllVersions:$AllVersions)
    if ($powerShellGetResults.Count -gt 0) {
        $reportedLocations = $powerShellGetResults | Microsoft.PowerShell.Core\ForEach-Object { $_.InstalledLocation }
        $fallbackResults = $fallbackResults | Microsoft.PowerShell.Core\Where-Object { $_.InstalledLocation -notin $reportedLocations }
    }

    $powerShellGetResults
    $fallbackResults
}
<##

.ForwardHelpTargetName Get-InstalledModule
.ForwardHelpCategory Function

#>
""";
    }
}