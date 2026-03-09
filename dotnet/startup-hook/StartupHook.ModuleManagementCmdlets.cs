using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using Microsoft.Management.Infrastructure;
using Microsoft.PowerShell.Commands;

public static partial class StartupHook
{
    private static readonly CmdletInfo s_importModuleCmdlet = new("Import-Module", typeof(ImportModuleCommand));

    public static IReadOnlyList<PSModuleInfo> InvokeImportModule(IDictionary<string, object> boundParameters)
    {
        EnsurePowerShellGetDependenciesLoaded(boundParameters);

        return InvokeImportModuleWithoutAlias(() =>
        {
            using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
            powerShell.AddCommand(s_importModuleCmdlet);

            foreach (KeyValuePair<string, object> entry in boundParameters)
            {
                string parameterName = entry.Key;
                if (string.Equals(parameterName, "PassThru", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                powerShell.AddParameter(parameterName, entry.Value);
            }

            powerShell.AddParameter("PassThru");

            PSModuleInfo[] importedModules = powerShell.Invoke<PSModuleInfo>().ToArray();
            PatchImportedModules(importedModules);
            return importedModules;
        });
    }

    private static void EnsurePowerShellGetDependenciesLoaded(IDictionary<string, object> boundParameters)
    {
        if (!IsPlainPowerShellGetImport(boundParameters)
            || GetLiveModuleFromCurrentRunspace("PackageManagement") is not null)
        {
            return;
        }

        string bundledPackageManagementManifest = Path.Combine(GetPsHomeModulesPath(), "PackageManagement", "PackageManagement.psd1");
        string packageManagementImportTarget = File.Exists(bundledPackageManagementManifest)
            ? bundledPackageManagementManifest
            : "PackageManagement";

        _ = InvokeImportModuleWithoutAlias(() =>
        {
            using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
            powerShell.AddCommand(s_importModuleCmdlet);
            powerShell.AddParameter("Name", packageManagementImportTarget);
            powerShell.AddParameter("Global");
            powerShell.AddParameter("PassThru");
            powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
            _ = powerShell.Invoke<PSModuleInfo>();
            return 0;
        });
    }

    private static bool IsPlainPowerShellGetImport(IDictionary<string, object> boundParameters)
    {
        if (!boundParameters.TryGetValue("Name", out object? nameValue)
            || nameValue is not string[] names
            || names.Length != 1
            || !string.Equals(names[0], "PowerShellGet", StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        foreach (string parameterName in boundParameters.Keys)
        {
            if (string.Equals(parameterName, "Name", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "PassThru", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "Verbose", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "Debug", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "ErrorAction", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "WarningAction", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "InformationAction", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "ProgressAction", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "WhatIf", StringComparison.OrdinalIgnoreCase)
                || string.Equals(parameterName, "Confirm", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            return false;
        }

        return true;
    }

    private static T InvokeImportModuleWithoutAlias<T>(Func<T> action)
    {
        object? sessionState = GetCurrentRunspaceSessionState();
        if (sessionState is null)
        {
            return action();
        }

        MethodInfo getAlias = sessionState.GetType().GetMethod(
            "GetAlias",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(CommandOrigin) },
            modifiers: null)!;
        AliasInfo? alias = getAlias.Invoke(sessionState, new object[] { ImportModuleCommandName, CommandOrigin.Internal }) as AliasInfo;
        if (alias is null || !string.Equals(alias.Definition, ImportModuleCmdletHelperName, StringComparison.OrdinalIgnoreCase))
        {
            return action();
        }

        MethodInfo removeAlias = sessionState.GetType().GetMethod(
            "RemoveAlias",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(bool) },
            modifiers: null)!;
        MethodInfo setAliasValue = sessionState.GetType().GetMethod(
            "SetAliasValue",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(string), typeof(ScopedItemOptions), typeof(bool), typeof(CommandOrigin) },
            modifiers: null)!;

        _ = removeAlias.Invoke(sessionState, new object[] { ImportModuleCommandName, true });
        try
        {
            return action();
        }
        finally
        {
            _ = setAliasValue.Invoke(
                sessionState,
                new object[] { ImportModuleCommandName, ImportModuleCmdletHelperName, ScopedItemOptions.AllScope, true, CommandOrigin.Internal }
            );
        }
    }

    private static void PatchImportedModules(IEnumerable<PSModuleInfo> importedModules)
    {
        foreach (PSModuleInfo module in importedModules)
        {
            if (string.Equals(module.Name, "PowerShellGet", StringComparison.OrdinalIgnoreCase))
            {
                PatchPowerShellGetModuleForVenv(GetLiveModuleFromCurrentRunspace(module.Name) ?? module);
            }
        }
    }

    private static object? GetCurrentRunspaceSessionState()
    {
        Runspace? runspace = Runspace.DefaultRunspace;
        if (runspace is null)
        {
            return null;
        }

        object executionContext = runspace.GetType().GetProperty(
            "ExecutionContext",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(runspace)!;

        return executionContext.GetType().GetProperty(
            "EngineSessionState",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(executionContext);
    }

    private static PSModuleInfo? GetLiveModuleFromCurrentRunspace(string moduleName)
    {
        Runspace? runspace = Runspace.DefaultRunspace;
        if (runspace is null)
        {
            return null;
        }

        object executionContext = runspace.GetType().GetProperty(
            "ExecutionContext",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(runspace)!;
        object modules = executionContext.GetType().GetProperty(
            "Modules",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(executionContext)!;
        object moduleTable = modules.GetType().GetProperty(
            "ModuleTable",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)!
            .GetValue(modules)!;

        foreach (PSModuleInfo module in ((System.Collections.IDictionary)moduleTable).Values)
        {
            if (string.Equals(module.Name, moduleName, StringComparison.OrdinalIgnoreCase))
            {
                return module;
            }
        }

        return null;
    }
}

[Cmdlet(VerbsData.Import, "Module", DefaultParameterSetName = NameParameterSet)]
[OutputType(typeof(PSModuleInfo))]
public sealed class StartupHookImportModuleCommand : PSCmdlet
{
    private const string NameParameterSet = "Name";
    private const string FullyQualifiedNameParameterSet = "FullyQualifiedName";
    private const string PSSessionParameterSet = "PSSession";
    private const string CimSessionParameterSet = "CimSession";
    private const string ModuleInfoParameterSet = "ModuleInfo";
    private const string AssemblyParameterSet = "Assembly";

    [Parameter(Position = 0, Mandatory = true, ParameterSetName = NameParameterSet, ValueFromPipelineByPropertyName = true)]
    [Parameter(Position = 0, Mandatory = true, ParameterSetName = PSSessionParameterSet, ValueFromPipelineByPropertyName = true)]
    [Parameter(Position = 0, Mandatory = true, ParameterSetName = CimSessionParameterSet, ValueFromPipelineByPropertyName = true)]
    public string[]? Name { get; set; }

    [Parameter(Mandatory = true, ParameterSetName = FullyQualifiedNameParameterSet, ValueFromPipelineByPropertyName = true)]
    public ModuleSpecification[]? FullyQualifiedName { get; set; }

    [Parameter(Mandatory = true, ParameterSetName = ModuleInfoParameterSet, ValueFromPipeline = true)]
    public PSModuleInfo[]? ModuleInfo { get; set; }

    [Parameter(Mandatory = true, ParameterSetName = AssemblyParameterSet, ValueFromPipeline = true)]
    public System.Reflection.Assembly[]? Assembly { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string[]? Function { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string[]? Cmdlet { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string[]? Variable { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string[]? Alias { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string? Prefix { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public SwitchParameter Global { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    [Parameter(ParameterSetName = PSSessionParameterSet)]
    [Parameter(ParameterSetName = CimSessionParameterSet)]
    public SwitchParameter PassThru { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    [Parameter(ParameterSetName = PSSessionParameterSet)]
    [Parameter(ParameterSetName = CimSessionParameterSet)]
    public SwitchParameter Force { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public SwitchParameter NoClobber { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public SwitchParameter AsCustomObject { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public SwitchParameter DisableNameChecking { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = PSSessionParameterSet)]
    [Parameter(ParameterSetName = CimSessionParameterSet)]
    public SwitchParameter SkipEditionCheck { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Parameter(ParameterSetName = ModuleInfoParameterSet)]
    [Parameter(ParameterSetName = AssemblyParameterSet)]
    public string? Scope { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    public Version? MinimumVersion { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    public Version? MaximumVersion { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    [Alias("Version")]
    public Version? RequiredVersion { get; set; }

    [Parameter(ParameterSetName = NameParameterSet)]
    [Parameter(ParameterSetName = FullyQualifiedNameParameterSet)]
    public Guid? Guid { get; set; }

    [Parameter(Mandatory = true, ParameterSetName = PSSessionParameterSet)]
    public PSSession? PSSession { get; set; }

    [Parameter(Mandatory = true, ParameterSetName = CimSessionParameterSet)]
    public CimSession? CimSession { get; set; }

    [Parameter(ParameterSetName = CimSessionParameterSet)]
    public Uri? CimResourceUri { get; set; }

    [Parameter(ParameterSetName = CimSessionParameterSet)]
    public string? CimNamespace { get; set; }

    protected override void ProcessRecord()
    {
        IReadOnlyList<PSModuleInfo> importedModules = StartupHook.InvokeImportModule(MyInvocation.BoundParameters);
        if (!PassThru)
        {
            return;
        }

        foreach (PSModuleInfo module in importedModules)
        {
            WriteObject(module);
        }
    }
}

[Cmdlet(VerbsLifecycle.Install, "Module", DefaultParameterSetName = "NameParameterSet", SupportsShouldProcess = true, ConfirmImpact = ConfirmImpact.Medium)]
public sealed class StartupHookInstallModuleCommand : PSCmdlet
{
    [Parameter(ParameterSetName = "NameParameterSet", Mandatory = true, Position = 0, ValueFromPipelineByPropertyName = true)]
    public string[]? Name { get; set; }

    [Parameter(ParameterSetName = "InputObject", Mandatory = true, Position = 0, ValueFromPipeline = true, ValueFromPipelineByPropertyName = true)]
    public PSObject[]? InputObject { get; set; }

    [Parameter(ParameterSetName = "NameParameterSet", ValueFromPipelineByPropertyName = true)]
    public string? MinimumVersion { get; set; }

    [Parameter(ParameterSetName = "NameParameterSet", ValueFromPipelineByPropertyName = true)]
    public string? MaximumVersion { get; set; }

    [Parameter(ParameterSetName = "NameParameterSet", ValueFromPipelineByPropertyName = true)]
    public string? RequiredVersion { get; set; }

    [Parameter(ParameterSetName = "NameParameterSet")]
    public string[]? Repository { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public PSCredential? Credential { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public string? Scope { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public Uri? Proxy { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public PSCredential? ProxyCredential { get; set; }

    [Parameter]
    public SwitchParameter AllowClobber { get; set; }

    [Parameter]
    public SwitchParameter SkipPublisherCheck { get; set; }

    [Parameter]
    public SwitchParameter Force { get; set; }

    [Parameter(ParameterSetName = "NameParameterSet")]
    public SwitchParameter AllowPrerelease { get; set; }

    [Parameter]
    public SwitchParameter AcceptLicense { get; set; }

    [Parameter]
    public SwitchParameter PassThru { get; set; }

    protected override void ProcessRecord()
    {
        StartupHook.EnsurePowerShellGetVenvPatched();
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("PowerShellGet\\Install-Module");
        foreach (KeyValuePair<string, object> entry in MyInvocation.BoundParameters)
        {
            powerShell.AddParameter(entry.Key, entry.Value);
        }

        _ = powerShell.Invoke();

        if (ParameterSetName != "NameParameterSet")
        {
            return;
        }

        foreach (PSObject installedModule in StartupHook.CompleteInstallModuleForVenv(
                     Name,
                     MinimumVersion,
                     RequiredVersion,
                     MaximumVersion,
                     Repository,
                     Credential,
                     Proxy,
                     ProxyCredential,
                     AllowPrerelease.IsPresent,
                     PassThru.IsPresent))
        {
            WriteObject(installedModule);
        }
    }
}

[Cmdlet(VerbsCommon.Get, "InstalledModule")]
public sealed class StartupHookGetInstalledModuleCommand : PSCmdlet
{
    [Parameter(Position = 0, ValueFromPipelineByPropertyName = true)]
    public string[]? Name { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public string? MinimumVersion { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public string? RequiredVersion { get; set; }

    [Parameter(ValueFromPipelineByPropertyName = true)]
    public string? MaximumVersion { get; set; }

    [Parameter]
    public SwitchParameter AllVersions { get; set; }

    [Parameter]
    public SwitchParameter AllowPrerelease { get; set; }

    protected override void ProcessRecord()
    {
        StartupHook.EnsurePowerShellGetVenvPatched();
        foreach (PSObject installedModule in StartupHook.InvokeGetInstalledModule(
                     Name,
                     MinimumVersion,
                     RequiredVersion,
                     MaximumVersion,
                     AllVersions.IsPresent,
                     AllowPrerelease.IsPresent))
        {
            WriteObject(installedModule);
        }
    }
}

[Cmdlet(VerbsLifecycle.Install, "PSResource", DefaultParameterSetName = "NameParameterSet", SupportsShouldProcess = true, ConfirmImpact = ConfirmImpact.Medium)]
public sealed class StartupHookInstallPSResourceCommand : PSCmdlet
{
    [Parameter(ParameterSetName = "NameParameterSet", Mandatory = true, Position = 0, ValueFromPipelineByPropertyName = true)]
    public string[]? Name { get; set; }

    [Parameter(ParameterSetName = "InputObjectParameterSet", Mandatory = true, Position = 0, ValueFromPipeline = true, ValueFromPipelineByPropertyName = true)]
    public PSObject[]? InputObject { get; set; }

    public string? Version { get; set; }
    public string[]? Repository { get; set; }
    public PSCredential? Credential { get; set; }
    public string? Scope { get; set; }
    public SwitchParameter TrustRepository { get; set; }
    public SwitchParameter Quiet { get; set; }
    public SwitchParameter Prerelease { get; set; }
    public SwitchParameter Reinstall { get; set; }
    public SwitchParameter NoClobber { get; set; }
    public SwitchParameter AcceptLicense { get; set; }
    public SwitchParameter PassThru { get; set; }
    public SwitchParameter SkipDependencyCheck { get; set; }
    public SwitchParameter AuthenticodeCheck { get; set; }
    public string? TemporaryPath { get; set; }
    public object? RequiredResource { get; set; }
    public string? RequiredResourceFile { get; set; }

    protected override void ProcessRecord()
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("Microsoft.PowerShell.PSResourceGet\\Install-PSResource");
        foreach (KeyValuePair<string, object> entry in MyInvocation.BoundParameters)
        {
            powerShell.AddParameter(entry.Key, entry.Value);
        }

        PSObject[] nativeResults = powerShell.Invoke().ToArray();
        if (!PassThru)
        {
            return;
        }

        foreach (PSObject resource in StartupHook.CompleteInstallPSResourceForVenv(
                     nativeResults,
                     Name,
                     InputObject,
                     Version,
                     Repository,
                     Credential,
                     TrustRepository.IsPresent,
                     Quiet.IsPresent,
                     Prerelease.IsPresent,
                     AcceptLicense.IsPresent,
                     SkipDependencyCheck.IsPresent,
                     AuthenticodeCheck.IsPresent,
                     TemporaryPath))
        {
            WriteObject(resource);
        }
    }
}

[Cmdlet(VerbsCommon.Get, "InstalledPSResource")]
public sealed class StartupHookGetInstalledPSResourceCommand : PSCmdlet
{
    [Parameter(Position = 0, ValueFromPipelineByPropertyName = true)]
    public string[]? Name { get; set; }

    public string? Version { get; set; }
    public string? Scope { get; set; }
    public string? Path { get; set; }

    protected override void ProcessRecord()
    {
        foreach (PSObject resource in StartupHook.InvokeGetInstalledPSResource(Name, Version, Scope, Path))
        {
            WriteObject(resource);
        }
    }
}