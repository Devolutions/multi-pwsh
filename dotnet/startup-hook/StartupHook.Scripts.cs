using System;

public static partial class StartupHook
{
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
    [StartupHook]::EnsurePowerShellGetVenvPatched()
}

process
{
    PowerShellGet\Install-Module @PSBoundParameters

    if ($PSCmdlet.ParameterSetName -eq 'NameParameterSet') {
        [StartupHook]::CompleteInstallModuleForVenv(
            $Name,
            $MinimumVersion,
            $RequiredVersion,
            $MaximumVersion,
            $Repository,
            $Credential,
            $Proxy,
            $ProxyCredential,
            $AllowPrerelease.IsPresent,
            $PassThru.IsPresent
        )
    }
}
<##

.ForwardHelpTargetName Install-Module
.ForwardHelpCategory Function

#>
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
}

process
{
    $nativeResults = @(Microsoft.PowerShell.PSResourceGet\Install-PSResource @PSBoundParameters)
    if ($PassThru) {
        [StartupHook]::CompleteInstallPSResourceForVenv(
            $nativeResults,
            $Name,
            $InputObject,
            $Version,
            $Repository,
            $Credential,
            $TrustRepository.IsPresent,
            $Quiet.IsPresent,
            $Prerelease.IsPresent,
            $AcceptLicense.IsPresent,
            $SkipDependencyCheck.IsPresent,
            $AuthenticodeCheck.IsPresent,
            $TemporaryPath
        )
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
}

process
{
    [StartupHook]::InvokeGetInstalledPSResource($Name, $Version, $Scope, $Path)
}
<##

.ForwardHelpTargetName Get-InstalledPSResource
.ForwardHelpCategory Function

#>
""";
    }

    private static string BuildGetModuleWrapperScript()
    {
        return """
[CmdletBinding(DefaultParameterSetName='Loaded')]
param(
    [Parameter(Position=0, ValueFromPipelineByPropertyName=$true)]
    [string[]]
    ${Name},

    [Microsoft.PowerShell.Commands.ModuleSpecification[]]
    ${FullyQualifiedName},

    [switch]
    ${ListAvailable},

    [switch]
    ${All},

    [switch]
    ${Refresh},

    [System.Management.Automation.Runspaces.PSSession]
    ${PSSession},

    [Microsoft.Management.Infrastructure.CimSession]
    ${CimSession},

    [uri]
    ${CimResourceUri},

    [string]
    ${CimNamespace},

    [switch]
    ${SkipEditionCheck},

    [Alias('PSEdition')]
    [string]
    ${RequestedPSEdition}
)

process
{
    $nativeBoundParameters = @{}
    foreach ($entry in $PSBoundParameters.GetEnumerator()) {
        $nativeBoundParameters[$entry.Key] = $entry.Value
    }

    if ($nativeBoundParameters.ContainsKey('RequestedPSEdition')) {
        $nativeBoundParameters['PSEdition'] = $nativeBoundParameters['RequestedPSEdition']
        $nativeBoundParameters.Remove('RequestedPSEdition')
    }

    $results = @(Microsoft.PowerShell.Core\Get-Module @nativeBoundParameters)

    if (-not $ListAvailable) {
        foreach ($module in ($results | Microsoft.PowerShell.Core\Where-Object { $_.Name -eq 'PowerShellGet' })) {
            [StartupHook]::PatchPowerShellGetModuleForVenv($module)
        }
    }

    $results
}
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
    [StartupHook]::EnsurePowerShellGetVenvPatched()
}

process
{
    [StartupHook]::InvokeGetInstalledModule(
        $Name,
        $MinimumVersion,
        $RequiredVersion,
        $MaximumVersion,
        $AllVersions.IsPresent,
        $AllowPrerelease.IsPresent
    )
}
<##

.ForwardHelpTargetName Get-InstalledModule
.ForwardHelpCategory Function

#>
""";
    }
}