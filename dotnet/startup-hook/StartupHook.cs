using System;
using System.IO;
using System.Reflection;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading;

public static class StartupHook
{
    private const string ForceModulePathProperty = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
    private const string LogPathProperty = "PWSH_STARTUP_HOOK_LOG_PATH";
    private const string StrategyProperty = "PWSH_STARTUP_HOOK_STRATEGY";
    private const string PowerShellGetPatchHelperName = "__PWSH_HOST_PATCH_POWERSHELLGET_VENV";
    private const string VenvInstalledModuleHelperName = "__PWSH_HOST_GET_VENV_INSTALLED_MODULE";
    private const string InstallModuleWrapperName = "Install-Module";
    private const string GetInstalledModuleWrapperName = "Get-InstalledModule";

    private static string? s_forcedModulePath;
    private static string? s_logPath;
    private static string? s_strategy;

    public static void Initialize()
    {
        s_forcedModulePath = ReadConfigurationValue(ForceModulePathProperty);
        s_logPath = ReadConfigurationValue(LogPathProperty);
        s_strategy = ReadConfigurationValue(StrategyProperty);

        Environment.SetEnvironmentVariable("DOTNET_STARTUP_HOOKS", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_LOG_PATH", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_STRATEGY", null);

        try
        {
            WriteLog($"startup hook entered; initial PSModulePath={Environment.GetEnvironmentVariable("PSModulePath")}");

            if (string.IsNullOrWhiteSpace(s_forcedModulePath))
            {
                WriteLog("no forced module path provided");
                return;
            }

            if (!string.IsNullOrWhiteSpace(s_strategy)
                && !string.Equals(s_strategy, "module-path", StringComparison.OrdinalIgnoreCase))
            {
                throw new NotSupportedException($"unsupported startup hook strategy '{s_strategy}'");
            }

            Assembly sma = Assembly.Load("System.Management.Automation");
            Type moduleIntrinsics = sma.GetType("System.Management.Automation.ModuleIntrinsics", throwOnError: true)!;

            ConfigureModulePathOverride(sma, moduleIntrinsics);
            Environment.SetEnvironmentVariable("PSModulePath", s_forcedModulePath);
            RuntimeHelpers.RunClassConstructor(moduleIntrinsics.TypeHandle);
            WriteLog($"configured module-path override; rewritten PSModulePath={Environment.GetEnvironmentVariable("PSModulePath")}");
            BeginPowerShellGetCommandOverrides();

            Environment.SetEnvironmentVariable("PSModulePath", s_forcedModulePath);
            WriteLog($"pre-seeded PSModulePath={Environment.GetEnvironmentVariable("PSModulePath")}");
        }
        catch (Exception ex)
        {
            WriteLog(ex.ToString());
            throw;
        }
    }

    private static string? ReadConfigurationValue(string name)
    {
        object? runtimeProperty = AppContext.GetData(name);
        if (runtimeProperty is string stringValue && !string.IsNullOrWhiteSpace(stringValue))
        {
            return stringValue;
        }

        string? environmentValue = Environment.GetEnvironmentVariable(name);
        if (!string.IsNullOrWhiteSpace(environmentValue))
        {
            return environmentValue;
        }

        return null;
    }

    private static void WriteLog(string message)
    {
        if (string.IsNullOrWhiteSpace(s_logPath))
        {
            return;
        }

        string? directory = Path.GetDirectoryName(s_logPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        File.AppendAllText(s_logPath, $"{DateTime.UtcNow:O} {message}{Environment.NewLine}");
    }

    private static void BeginPowerShellGetCommandOverrides()
    {
        Thread thread = new(() =>
        {
            try
            {
                WaitForPrimaryRunspaceAndInstallPowerShellGetOverrides();
            }
            catch (Exception ex)
            {
                WriteLog($"failed to install PowerShellGet overrides: {ex}");
            }
        })
        {
            IsBackground = true,
            Name = "pwsh-host-powershellget-overrides",
        };

        thread.Start();
    }

    private static void WaitForPrimaryRunspaceAndInstallPowerShellGetOverrides()
    {
        PropertyInfo primaryRunspaceProperty = typeof(Runspace).GetProperty(
            "PrimaryRunspace",
            BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic)!;
        PropertyInfo executionContextProperty = typeof(Runspace).GetProperty(
            "ExecutionContext",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!;

        for (int attempt = 0; attempt < 10_000; attempt++)
        {
            Runspace? primaryRunspace = primaryRunspaceProperty.GetValue(null) as Runspace;
            if (primaryRunspace is not null)
            {
                object executionContext = executionContextProperty.GetValue(primaryRunspace)!;
                InstallPowerShellGetOverrides(executionContext);
                WriteLog("installed PowerShellGet command overrides");
                return;
            }

            Thread.Sleep(1);
        }

        WriteLog("timed out waiting for primary runspace to install PowerShellGet overrides");
    }

    private static void InstallPowerShellGetOverrides(object executionContext)
    {
        object sessionState = executionContext.GetType().GetProperty(
            "EngineSessionState",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(executionContext)!;
        MethodInfo setFunction = sessionState.GetType().GetMethod(
            "SetFunction",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(ScriptBlock), typeof(bool) },
            modifiers: null)!;

        string escapedForcedModulePath = (s_forcedModulePath ?? string.Empty).Replace("'", "''", StringComparison.Ordinal);
        InstallFunction(setFunction, sessionState, PowerShellGetPatchHelperName, BuildPowerShellGetPatchHelperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, VenvInstalledModuleHelperName, BuildVenvInstalledModuleHelperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, InstallModuleWrapperName, BuildInstallModuleWrapperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, GetInstalledModuleWrapperName, BuildGetInstalledModuleWrapperScript(escapedForcedModulePath));
    }

    private static void InstallFunction(MethodInfo setFunction, object sessionState, string functionName, string script)
    {
        ScriptBlock scriptBlock = ScriptBlock.Create(script);
        _ = setFunction.Invoke(sessionState, new object[] { functionName, scriptBlock, true });
    }

    private static string BuildPowerShellGetPatchHelperScript(string escapedForcedModulePath)
    {
        return $$"""
[CmdletBinding()]
param()

$forcedModulePath = '{{escapedForcedModulePath}}'
Microsoft.PowerShell.Core\Import-Module PowerShellGet -Scope Local -ErrorAction Stop | Out-Null
$module = Microsoft.PowerShell.Core\Get-Module PowerShellGet -ErrorAction Stop
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
}

process
{
    $powerShellGetResults = @(PowerShellGet\Get-InstalledModule @PSBoundParameters | Microsoft.PowerShell.Core\Where-Object {
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

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string GetModulePathReplacement()
    {
        return s_forcedModulePath ?? string.Empty;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string GetConfigModulePathReplacement(object powerShellConfig, int scope)
    {
        return s_forcedModulePath ?? string.Empty;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static System.Collections.Generic.IEnumerable<string> GetEnumeratedModulePathReplacement(bool includeSystemModulePath, object context)
    {
        var yieldedPaths = new System.Collections.Generic.HashSet<string>(StringComparer.OrdinalIgnoreCase);
        string? forcedPath = s_forcedModulePath;

        if (!string.IsNullOrWhiteSpace(forcedPath) && yieldedPaths.Add(forcedPath))
        {
            yield return forcedPath;
        }

        Assembly sma = Assembly.Load("System.Management.Automation");
        Type moduleIntrinsics = sma.GetType("System.Management.Automation.ModuleIntrinsics", throwOnError: true)!;
        MethodInfo getPsHomeModulePath = moduleIntrinsics.GetMethod(
            "GetPSHomeModulePath",
            BindingFlags.Static | BindingFlags.NonPublic)!;
        string? psHomeModulePath = getPsHomeModulePath.Invoke(null, null) as string;

        if (!string.IsNullOrWhiteSpace(psHomeModulePath) && yieldedPaths.Add(psHomeModulePath))
        {
            yield return psHomeModulePath;
        }
    }

    private static void ConfigureModulePathOverride(Assembly sma, Type moduleIntrinsics)
    {
        MethodInfo getPersonalModulePath = moduleIntrinsics.GetMethod(
            "GetPersonalModulePath",
            BindingFlags.Static | BindingFlags.NonPublic)!;
        MethodInfo getSharedModulePath = moduleIntrinsics.GetMethod(
            "GetSharedModulePath",
            BindingFlags.Static | BindingFlags.NonPublic)!;
        MethodInfo getEnumeratedModulePath = moduleIntrinsics.GetMethod(
            "GetModulePath",
            BindingFlags.Static | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(bool), sma.GetType("System.Management.Automation.ExecutionContext", throwOnError: true)! },
            modifiers: null)!;
        MethodInfo pathReplacement = typeof(StartupHook).GetMethod(
            nameof(GetModulePathReplacement),
            BindingFlags.NonPublic | BindingFlags.Static)!;
        MethodInfo enumeratedPathReplacement = typeof(StartupHook).GetMethod(
            nameof(GetEnumeratedModulePathReplacement),
            BindingFlags.NonPublic | BindingFlags.Static)!;

        Type configScope = sma.GetType("System.Management.Automation.Configuration.ConfigScope", throwOnError: true)!;
        Type powerShellConfig = sma.GetType("System.Management.Automation.Configuration.PowerShellConfig", throwOnError: true)!;
        MethodInfo getConfigModulePath = powerShellConfig.GetMethod(
            "GetModulePath",
            BindingFlags.Instance | BindingFlags.NonPublic,
            binder: null,
            types: new[] { configScope },
            modifiers: null)!;
        MethodInfo configReplacement = typeof(StartupHook).GetMethod(
            nameof(GetConfigModulePathReplacement),
            BindingFlags.NonPublic | BindingFlags.Static)!;

        PatchMethod(getPersonalModulePath, pathReplacement);
        PatchMethod(getSharedModulePath, pathReplacement);
        PatchMethod(getEnumeratedModulePath, enumeratedPathReplacement);
        PatchMethod(getConfigModulePath, configReplacement);
    }

    private static unsafe void PatchMethod(MethodInfo target, MethodInfo replacement)
    {
        if (IntPtr.Size != 8)
        {
            throw new PlatformNotSupportedException("This startup hook only supports x64 processes.");
        }

        RuntimeHelpers.PrepareMethod(target.MethodHandle);
        RuntimeHelpers.PrepareMethod(replacement.MethodHandle);

        IntPtr targetPtr = target.MethodHandle.GetFunctionPointer();
        IntPtr replacementPtr = replacement.MethodHandle.GetFunctionPointer();

        const uint PageExecuteReadWrite = 0x40;
        const int PatchSize = 12;

        if (!VirtualProtect(targetPtr, (nuint)PatchSize, PageExecuteReadWrite, out uint oldProtect))
        {
            throw new InvalidOperationException($"VirtualProtect failed: {Marshal.GetLastWin32Error()}");
        }

        byte* site = (byte*)targetPtr;
        site[0] = 0x48;
        site[1] = 0xB8;
        *((ulong*)(site + 2)) = (ulong)replacementPtr;
        site[10] = 0xFF;
        site[11] = 0xE0;

        _ = FlushInstructionCache(GetCurrentProcess(), targetPtr, (nuint)PatchSize);
        _ = VirtualProtect(targetPtr, (nuint)PatchSize, oldProtect, out _);
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool VirtualProtect(IntPtr address, nuint size, uint newProtect, out uint oldProtect);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool FlushInstructionCache(IntPtr process, IntPtr baseAddress, nuint size);
}