using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;

public static partial class StartupHook
{
    private static readonly object s_powerShellGetPatchLock = new();

    public static void EnsurePowerShellGetVenvPatched()
    {
        lock (s_powerShellGetPatchLock)
        {
            EnsurePowerShellGetVenvPatchedCore(runspace: null);
        }
    }

    public static void PatchPowerShellGetModuleForVenv(PSModuleInfo module)
    {
        lock (s_powerShellGetPatchLock)
        {
            string? moduleVenvPath = GetModuleVenvPath();
            if (string.IsNullOrWhiteSpace(moduleVenvPath))
            {
                return;
            }

            PatchPowerShellGetModuleSessionState(module, moduleVenvPath);
        }
    }

    private static void EnsurePowerShellGetVenvPatched(Runspace runspace)
    {
        lock (s_powerShellGetPatchLock)
        {
            EnsurePowerShellGetVenvPatchedCore(runspace);
        }
    }

    public static IEnumerable<PSObject> CompleteInstallModuleForVenv(
        string[]? name,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion,
        string[]? repository,
        PSCredential? credential,
        Uri? proxy,
        PSCredential? proxyCredential,
        bool allowPrerelease,
        bool passThru)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath) || name is not { Length: > 0 })
        {
            yield break;
        }

        string[] missingNames = name
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .Where(moduleName => !GetFallbackInstalledModules(
                new[] { moduleName },
                minimumVersion,
                requiredVersion,
                maximumVersion,
                false,
                moduleVenvPath).Any())
            .ToArray();

        if (missingNames.Length == 0)
        {
            yield break;
        }

        InvokeSaveModule(
            missingNames,
            minimumVersion,
            requiredVersion,
            maximumVersion,
            repository,
            credential,
            proxy,
            proxyCredential,
            allowPrerelease,
            moduleVenvPath);

        if (!passThru)
        {
            yield break;
        }

        foreach (PSObject installedModule in GetFallbackInstalledModules(
                     name,
                     minimumVersion,
                     requiredVersion,
                     maximumVersion,
                     false,
                     moduleVenvPath))
        {
            yield return installedModule;
        }
    }

    public static IEnumerable<PSObject> InvokeGetInstalledModule(
        string[]? name,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion,
        bool allVersions,
        bool allowPrerelease)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            yield break;
        }

        List<PSObject> nativeResults = InvokeNativeGetInstalledModule(
            name,
            minimumVersion,
            requiredVersion,
            maximumVersion,
            allVersions,
            allowPrerelease,
            moduleVenvPath);
        HashSet<string> reportedLocations = new(StringComparer.OrdinalIgnoreCase);

        foreach (PSObject nativeResult in nativeResults)
        {
            string? installedLocation = nativeResult.Properties["InstalledLocation"]?.Value?.ToString();
            if (!string.IsNullOrWhiteSpace(installedLocation))
            {
                _ = reportedLocations.Add(installedLocation);
            }

            yield return nativeResult;
        }

        foreach (PSObject fallbackResult in GetFallbackInstalledModules(
                     name,
                     minimumVersion,
                     requiredVersion,
                     maximumVersion,
                     allVersions,
                     moduleVenvPath))
        {
            string? installedLocation = fallbackResult.Properties["InstalledLocation"]?.Value?.ToString();
            if (string.IsNullOrWhiteSpace(installedLocation) || reportedLocations.Add(installedLocation))
            {
                yield return fallbackResult;
            }
        }
    }

    private static List<PSObject> InvokeNativeGetInstalledModule(
        string[]? name,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion,
        bool allVersions,
        bool allowPrerelease,
        string moduleVenvPath)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("PowerShellGet\\Get-InstalledModule");
        AddStringArrayParameter(powerShell, "Name", name);
        AddStringParameter(powerShell, "MinimumVersion", minimumVersion);
        AddStringParameter(powerShell, "RequiredVersion", requiredVersion);
        AddStringParameter(powerShell, "MaximumVersion", maximumVersion);

        if (allVersions)
        {
            powerShell.AddParameter("AllVersions");
        }

        if (allowPrerelease)
        {
            powerShell.AddParameter("AllowPrerelease");
        }

        powerShell.AddParameter("ErrorAction", ActionPreference.SilentlyContinue);

        return powerShell
            .Invoke()
            .Where(result => IsPathUnderRoot(result.Properties["InstalledLocation"]?.Value?.ToString(), moduleVenvPath))
            .ToList();
    }

    private static IEnumerable<PSObject> GetFallbackInstalledModules(
        string[]? name,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion,
        bool allVersions,
        string moduleVenvPath)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("Microsoft.PowerShell.Core\\Get-Module");
        powerShell.AddParameter("ListAvailable");
        AddStringArrayParameter(powerShell, "Name", name);

        IEnumerable<PSModuleInfo> filteredModules = powerShell
            .Invoke<PSModuleInfo>()
            .Where(module => IsPathUnderRoot(module.ModuleBase, moduleVenvPath))
            .Where(module => ModuleVersionMatches(module, minimumVersion, requiredVersion, maximumVersion));

        IOrderedEnumerable<PSModuleInfo> orderedModules = filteredModules
            .OrderByDescending(module => module.Name, StringComparer.OrdinalIgnoreCase)
            .ThenByDescending(module => module.Version);

        if (!allVersions)
        {
            orderedModules = orderedModules
                .GroupBy(module => module.Name, StringComparer.OrdinalIgnoreCase)
                .Select(group => group.First())
                .OrderByDescending(module => module.Name, StringComparer.OrdinalIgnoreCase)
                .ThenByDescending(module => module.Version);
        }

        foreach (PSModuleInfo module in orderedModules)
        {
            yield return CreateInstalledModuleResult(module);
        }
    }

    private static PSObject CreateInstalledModuleResult(PSModuleInfo module)
    {
        DateTime? installedDate = null;
        try
        {
            installedDate = Directory.GetLastWriteTime(module.ModuleBase);
        }
        catch
        {
        }

        PSObject result = new();
        result.Properties.Add(new PSNoteProperty("Name", module.Name));
        result.Properties.Add(new PSNoteProperty("Version", module.Version.ToString()));
        result.Properties.Add(new PSNoteProperty("Repository", null));
        result.Properties.Add(new PSNoteProperty("Description", module.Description));
        result.Properties.Add(new PSNoteProperty("InstalledLocation", module.ModuleBase));
        result.Properties.Add(new PSNoteProperty("InstalledDate", installedDate));
        result.Properties.Add(new PSNoteProperty("Type", "Module"));
        result.TypeNames.Insert(0, "Microsoft.PowerShell.Commands.PSRepositoryItemInfo");
        return result;
    }

    private static void InvokeSaveModule(
        string[] name,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion,
        string[]? repository,
        PSCredential? credential,
        Uri? proxy,
        PSCredential? proxyCredential,
        bool allowPrerelease,
        string moduleVenvPath)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("PowerShellGet\\Save-Module");
        powerShell.AddParameter("Name", name);
        powerShell.AddParameter("Path", moduleVenvPath);
        powerShell.AddParameter("Force");
        powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
        AddStringParameter(powerShell, "MinimumVersion", minimumVersion);
        AddStringParameter(powerShell, "RequiredVersion", requiredVersion);
        AddStringParameter(powerShell, "MaximumVersion", maximumVersion);
        AddStringArrayParameter(powerShell, "Repository", repository);
        AddObjectParameter(powerShell, "Credential", credential);
        AddObjectParameter(powerShell, "Proxy", proxy);
        AddObjectParameter(powerShell, "ProxyCredential", proxyCredential);

        if (allowPrerelease)
        {
            powerShell.AddParameter("AllowPrerelease");
        }

        _ = powerShell.Invoke();
    }

    private static void EnsurePowerShellGetVenvPatchedCore(Runspace? runspace)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            return;
        }

        PSModuleInfo module = EnsurePowerShellGetModuleLoaded(runspace);
        PatchPowerShellGetModuleSessionState(module, moduleVenvPath);
    }

    private static PSModuleInfo EnsurePowerShellGetModuleLoaded(Runspace? runspace)
    {
        PSModuleInfo? module = GetLoadedModule(runspace, "PowerShellGet");
        if (module is not null)
        {
            return module;
        }

        string bundledPackageManagementManifest = Path.Combine(GetPsHomeModulesPath(), "PackageManagement", "PackageManagement.psd1");
        string bundledPowerShellGetManifest = Path.Combine(GetPsHomeModulesPath(), "PowerShellGet", "PowerShellGet.psd1");

        if (GetLoadedModule(runspace, "PackageManagement") is null && File.Exists(bundledPackageManagementManifest))
        {
            ImportModule(runspace, bundledPackageManagementManifest);
        }

        if (File.Exists(bundledPowerShellGetManifest))
        {
            module = ImportModule(runspace, bundledPowerShellGetManifest);
        }
        else
        {
            module = ImportModule(runspace, "PowerShellGet");
        }

        return module;
    }

    private static PSModuleInfo? GetLoadedModule(Runspace? runspace, string moduleName)
    {
        using PowerShell powerShell = CreatePowerShell(runspace);
        powerShell.AddCommand("Microsoft.PowerShell.Core\\Get-Module");
        powerShell.AddParameter("Name", moduleName);
        powerShell.AddParameter("ErrorAction", ActionPreference.SilentlyContinue);
        return powerShell.Invoke<PSModuleInfo>().FirstOrDefault();
    }

    private static PSModuleInfo ImportModule(Runspace? runspace, string moduleNameOrPath)
    {
        using PowerShell powerShell = CreatePowerShell(runspace);
        powerShell.AddCommand("Microsoft.PowerShell.Core\\Import-Module");
        powerShell.AddParameter("Name", moduleNameOrPath);
        powerShell.AddParameter("PassThru");
        powerShell.AddParameter("Global");
        powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
        return powerShell.Invoke<PSModuleInfo>().First();
    }

    private static void PatchPowerShellGetModuleSessionState(PSModuleInfo module, string moduleVenvPath)
    {
        SessionState moduleSessionState = module.SessionState;
        PSVariableIntrinsics variables = moduleSessionState.PSVariable;

        object? programFilesModulesPath = variables.GetValue("ProgramFilesModulesPath");
        object? programFilesScriptsPath = variables.GetValue("ProgramFilesScriptsPath");
        variables.Set("MyDocumentsModulesPath", moduleVenvPath);
        variables.Set("MyDocumentsScriptsPath", moduleVenvPath);
        variables.Set("PSGetPath", CreatePsGetPathObject(programFilesModulesPath, programFilesScriptsPath, moduleVenvPath));
        variables.Set("PSGetInstalledModules", null);

        object internalSessionState = GetInternalSessionState(moduleSessionState);
        MethodInfo setFunction = internalSessionState.GetType().GetMethod(
            "SetFunction",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(ScriptBlock), typeof(bool) },
            modifiers: null)!;
        ScriptBlock scriptBlock = moduleSessionState.InvokeCommand.NewScriptBlock(BuildPowerShellGetTestModuleInstalledScript());
        _ = setFunction.Invoke(internalSessionState, new object[] { "Test-ModuleInstalled", scriptBlock, true });
    }

    public static IEnumerable<PSModuleInfo> InvokePowerShellGetTestModuleInstalled(string name, string? requiredVersion)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            return Array.Empty<PSModuleInfo>();
        }

        using PowerShell powerShell = CreatePowerShell(runspace: null);
        powerShell.AddCommand("Microsoft.PowerShell.Core\\Get-Module");
        powerShell.AddParameter("ListAvailable");
        powerShell.AddParameter("Name", name);
        powerShell.AddParameter("Verbose", false);
        IEnumerable<PSModuleInfo> modules = powerShell
            .Invoke<PSModuleInfo>()
            .Where(module => IsPathUnderRoot(module.ModuleBase, moduleVenvPath))
            .Where(module => PowerShellGetRequiredVersionMatches(module, requiredVersion))
            .Take(1)
            .ToArray();

        return modules;
    }

    private static bool PowerShellGetRequiredVersionMatches(PSModuleInfo module, string? requiredVersion)
    {
        if (string.IsNullOrWhiteSpace(requiredVersion))
        {
            return true;
        }

        string moduleVersion = module.Version.ToString();
        if (string.Equals(requiredVersion.Trim(), moduleVersion, StringComparison.Ordinal))
        {
            return true;
        }

        string? prerelease = GetPsObjectPropertyString(GetPsObjectPropertyValue(module.PrivateData, "PSData"), "Prerelease");
        if (string.IsNullOrWhiteSpace(prerelease))
        {
            return false;
        }

        return string.Equals(requiredVersion.Trim(), $"{moduleVersion}-{prerelease}", StringComparison.OrdinalIgnoreCase);
    }

    private static object GetInternalSessionState(SessionState sessionState)
    {
        FieldInfo internalField = typeof(SessionState).GetField("_sessionState", BindingFlags.Instance | BindingFlags.NonPublic)!;
        return internalField.GetValue(sessionState)!;
    }

    private static PSObject CreatePsGetPathObject(object? allUsersModules, object? allUsersScripts, string currentUserPath)
    {
        PSObject result = new();
        result.Properties.Add(new PSNoteProperty("AllUsersModules", allUsersModules));
        result.Properties.Add(new PSNoteProperty("AllUsersScripts", allUsersScripts));
        result.Properties.Add(new PSNoteProperty("CurrentUserModules", currentUserPath));
        result.Properties.Add(new PSNoteProperty("CurrentUserScripts", currentUserPath));
        result.TypeNames.Insert(0, "Microsoft.PowerShell.Commands.PSGetPath");
        return result;
    }

    private static string BuildPowerShellGetTestModuleInstalledScript()
    {
        return "[CmdletBinding(PositionalBinding=$false)]\n" +
               "param(\n" +
               "    [Parameter(Mandatory=$true)][ValidateNotNullOrEmpty()][string]$Name,\n" +
               "    [Parameter()][string]$RequiredVersion\n" +
               ")\n" +
               "[StartupHook]::InvokePowerShellGetTestModuleInstalled($Name, $RequiredVersion)";
    }

    private static PowerShell CreatePowerShell(Runspace? runspace)
    {
        PowerShell powerShell = PowerShell.Create();
        if (runspace is not null)
        {
            powerShell.Runspace = runspace;
        }

        return powerShell;
    }

    private static void AddStringArrayParameter(PowerShell powerShell, string name, string[]? value)
    {
        if (value is { Length: > 0 })
        {
            powerShell.AddParameter(name, value);
        }
    }

    private static void AddStringParameter(PowerShell powerShell, string name, string? value)
    {
        if (!string.IsNullOrWhiteSpace(value))
        {
            powerShell.AddParameter(name, value);
        }
    }

    private static void AddObjectParameter(PowerShell powerShell, string name, object? value)
    {
        if (value is not null)
        {
            powerShell.AddParameter(name, value);
        }
    }

    private static bool ModuleVersionMatches(
        PSModuleInfo module,
        string? minimumVersion,
        string? requiredVersion,
        string? maximumVersion)
    {
        string moduleVersion = module.Version.ToString();
        if (!string.IsNullOrWhiteSpace(requiredVersion) && !string.Equals(moduleVersion, requiredVersion, StringComparison.Ordinal))
        {
            return false;
        }

        if (!VersionSatisfiesMinimum(module.Version, minimumVersion))
        {
            return false;
        }

        if (!VersionSatisfiesMaximum(module.Version, maximumVersion))
        {
            return false;
        }

        return true;
    }

    private static bool VersionSatisfiesMinimum(Version version, string? minimumVersion)
    {
        if (string.IsNullOrWhiteSpace(minimumVersion))
        {
            return true;
        }

        return Version.TryParse(minimumVersion, out Version? minimum) && version >= minimum;
    }

    private static bool VersionSatisfiesMaximum(Version version, string? maximumVersion)
    {
        if (string.IsNullOrWhiteSpace(maximumVersion))
        {
            return true;
        }

        return Version.TryParse(maximumVersion, out Version? maximum) && version <= maximum;
    }

    private static bool IsPathUnderRoot(string? candidatePath, string rootPath)
    {
        if (string.IsNullOrWhiteSpace(candidatePath) || string.IsNullOrWhiteSpace(rootPath))
        {
            return false;
        }

        string normalizedRoot = NormalizePathForPrefix(rootPath);
        string normalizedCandidate = NormalizePathForPrefix(candidatePath);

        return string.Equals(normalizedCandidate, normalizedRoot, StringComparison.OrdinalIgnoreCase)
            || normalizedCandidate.StartsWith(normalizedRoot + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)
            || normalizedCandidate.StartsWith(normalizedRoot + Path.AltDirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);
    }

    private static string NormalizePathForPrefix(string path)
    {
        return Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
    }
}