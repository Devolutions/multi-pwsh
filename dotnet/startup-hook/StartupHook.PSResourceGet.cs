using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;

public static partial class StartupHook
{
    public static IEnumerable<PSObject> InvokeGetInstalledPSResource(
        string[]? name,
        string? version,
        string? scope,
        string? path)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            yield break;
        }

        string targetPath = string.IsNullOrWhiteSpace(path) ? moduleVenvPath : path;
        if (!string.Equals(targetPath, moduleVenvPath, StringComparison.OrdinalIgnoreCase))
        {
            foreach (PSObject nativeResult in InvokeNativeGetInstalledPSResource(name, version, scope, targetPath, silenceErrors: false))
            {
                yield return nativeResult;
            }

            yield break;
        }

        bool hasManifestInVenv = Directory.Exists(moduleVenvPath)
            && Directory.EnumerateFiles(moduleVenvPath, "*.psd1", SearchOption.AllDirectories).Any();

        List<PSObject> nativeResults = hasManifestInVenv
            ? InvokeNativeGetInstalledPSResource(name, version, scope: null, moduleVenvPath, silenceErrors: true)
                .Where(result => IsPathUnderRoot(result.Properties["InstalledLocation"]?.Value?.ToString(), moduleVenvPath))
                .ToList()
            : new List<PSObject>();
        HashSet<string> reportedKeys = new(StringComparer.OrdinalIgnoreCase);

        foreach (PSObject nativeResult in nativeResults)
        {
            _ = reportedKeys.Add(GetPsResourceResultKey(nativeResult));
            yield return nativeResult;
        }

        foreach (PSObject fallbackResult in GetFallbackInstalledPsResources(name, version, moduleVenvPath))
        {
            if (reportedKeys.Add(GetPsResourceResultKey(fallbackResult)))
            {
                yield return fallbackResult;
            }
        }
    }

    public static IEnumerable<PSObject> CompleteInstallPSResourceForVenv(
        PSObject[]? nativeResults,
        string[]? name,
        PSObject[]? inputObject,
        string? version,
        string[]? repository,
        PSCredential? credential,
        bool trustRepository,
        bool quiet,
        bool prerelease,
        bool acceptLicense,
        bool skipDependencyCheck,
        bool authenticodeCheck,
        string? temporaryPath)
    {
        string? moduleVenvPath = GetModuleVenvPath();
        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            yield break;
        }

        bool savePerformed = false;
        string[] savedNames = Array.Empty<string>();

        if (name is { Length: > 0 })
        {
            string[] moduleNamesToSave = name
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .Where(resourceName => !GetFallbackInstalledPsResources(new[] { resourceName }, version, moduleVenvPath).Any())
                .ToArray();

            if (moduleNamesToSave.Length > 0)
            {
                InvokeSavePsResourceByName(
                    moduleNamesToSave,
                    version,
                    repository,
                    credential,
                    trustRepository,
                    quiet,
                    prerelease,
                    acceptLicense,
                    skipDependencyCheck,
                    authenticodeCheck,
                    temporaryPath,
                    moduleVenvPath);
                savePerformed = true;
                savedNames = moduleNamesToSave;
            }
        }
        else if (inputObject is { Length: > 0 })
        {
            List<PSObject> moduleInputsToSave = new();
            List<string> savedNameList = new();

            foreach (PSObject resource in inputObject)
            {
                string? resourceName = resource.Properties["Name"]?.Value?.ToString();
                if (string.IsNullOrWhiteSpace(resourceName))
                {
                    continue;
                }

                if (GetFallbackInstalledPsResources(new[] { resourceName }, version, moduleVenvPath).Any())
                {
                    continue;
                }

                if (!IsModuleResourceObject(resource))
                {
                    continue;
                }

                moduleInputsToSave.Add(resource);
                savedNameList.Add(resourceName);
            }

            if (moduleInputsToSave.Count > 0)
            {
                InvokeSavePsResourceByInputObject(
                    moduleInputsToSave.ToArray(),
                    repository,
                    credential,
                    trustRepository,
                    quiet,
                    prerelease,
                    acceptLicense,
                    skipDependencyCheck,
                    authenticodeCheck,
                    temporaryPath,
                    moduleVenvPath);
                savePerformed = true;
                savedNames = savedNameList.Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
            }
        }

        List<PSObject> filteredNativeResults = (nativeResults ?? Array.Empty<PSObject>())
            .Where(result =>
            {
                string? installedLocation = result.Properties["InstalledLocation"]?.Value?.ToString();
                return string.IsNullOrWhiteSpace(installedLocation) || IsPathUnderRoot(installedLocation, moduleVenvPath);
            })
            .ToList();
        HashSet<string> reportedKeys = new(StringComparer.OrdinalIgnoreCase);

        foreach (PSObject nativeResult in filteredNativeResults)
        {
            _ = reportedKeys.Add(GetPsResourceResultKey(nativeResult));
            yield return nativeResult;
        }

        if (!savePerformed)
        {
            yield break;
        }

        foreach (PSObject fallbackResult in GetFallbackInstalledPsResources(savedNames, version, moduleVenvPath))
        {
            if (reportedKeys.Add(GetPsResourceResultKey(fallbackResult)))
            {
                yield return fallbackResult;
            }
        }
    }

    private static List<PSObject> InvokeNativeGetInstalledPSResource(
        string[]? name,
        string? version,
        string? scope,
        string path,
        bool silenceErrors)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("Microsoft.PowerShell.PSResourceGet\\Get-InstalledPSResource");
        AddStringArrayParameter(powerShell, "Name", name);
        AddStringParameter(powerShell, "Version", version);
        AddStringParameter(powerShell, "Scope", scope);
        powerShell.AddParameter("Path", path);

        if (silenceErrors)
        {
            powerShell.AddParameter("ErrorAction", ActionPreference.SilentlyContinue);
        }

        return powerShell.Invoke().ToList();
    }

    private static IEnumerable<PSObject> GetFallbackInstalledPsResources(
        string[]? name,
        string? version,
        string targetPath)
    {
        List<PSModuleInfo> modules = new();

        if (!Directory.Exists(targetPath))
        {
            yield break;
        }

        foreach (string manifestPath in Directory.EnumerateFiles(targetPath, "*.psd1", SearchOption.AllDirectories)
                     .Distinct(StringComparer.OrdinalIgnoreCase))
        {
            PSModuleInfo? module = TryLoadModuleManifest(manifestPath);
            if (module is null || !IsPathUnderRoot(module.ModuleBase, targetPath))
            {
                continue;
            }

            if (name is { Length: > 0 } && !name.Contains(module.Name, StringComparer.OrdinalIgnoreCase))
            {
                continue;
            }

            if (!string.IsNullOrWhiteSpace(version) && !string.Equals(module.Version.ToString(), version, StringComparison.Ordinal))
            {
                continue;
            }

            modules.Add(module);
        }

        foreach (PSModuleInfo module in modules
                     .GroupBy(module => module.Path, StringComparer.OrdinalIgnoreCase)
                     .Select(group => group.First())
                     .OrderByDescending(module => module.Name, StringComparer.OrdinalIgnoreCase)
                     .ThenByDescending(module => module.Version))
        {
            yield return CreateInstalledPsResourceResult(module);
        }
    }

    private static PSModuleInfo? TryLoadModuleManifest(string manifestPath)
    {
        try
        {
            using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
            powerShell.AddCommand("Microsoft.PowerShell.Core\\Test-ModuleManifest");
            powerShell.AddParameter("Path", manifestPath);
            powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
            return powerShell.Invoke<PSModuleInfo>().FirstOrDefault();
        }
        catch
        {
            return null;
        }
    }

    private static PSObject CreateInstalledPsResourceResult(PSModuleInfo module)
    {
        object? privateData = null;
        object? psData = null;
        if (module.PrivateData is not null)
        {
            privateData = module.PrivateData;
            psData = PSObject.AsPSObject(privateData).Properties["PSData"]?.Value;
        }

        DateTime? installedDate = null;
        try
        {
            installedDate = Directory.GetLastWriteTime(module.ModuleBase);
        }
        catch
        {
        }

        string prerelease = GetPsObjectPropertyString(psData, "Prerelease") ?? string.Empty;
        object typeValue = GetPsResourceModuleTypeValue();

        PSObject result = new();
        result.Properties.Add(new PSNoteProperty("AdditionalMetadata", null));
        result.Properties.Add(new PSNoteProperty("Author", module.Author));
        result.Properties.Add(new PSNoteProperty("CompanyName", module.CompanyName));
        result.Properties.Add(new PSNoteProperty("Copyright", module.Copyright));
        result.Properties.Add(new PSNoteProperty("Dependencies", module.RequiredModules));
        result.Properties.Add(new PSNoteProperty("Description", module.Description));
        result.Properties.Add(new PSNoteProperty("IconUri", GetPsObjectPropertyValue(psData, "IconUri")));
        result.Properties.Add(new PSNoteProperty("Includes", null));
        result.Properties.Add(new PSNoteProperty("InstalledDate", installedDate));
        result.Properties.Add(new PSNoteProperty("InstalledLocation", module.ModuleBase));
        result.Properties.Add(new PSNoteProperty("IsPrerelease", !string.IsNullOrEmpty(prerelease)));
        result.Properties.Add(new PSNoteProperty("LicenseUri", GetPsObjectPropertyValue(psData, "LicenseUri")));
        result.Properties.Add(new PSNoteProperty("Name", module.Name));
        result.Properties.Add(new PSNoteProperty("Prerelease", prerelease));
        result.Properties.Add(new PSNoteProperty("ProjectUri", GetPsObjectPropertyValue(psData, "ProjectUri") ?? module.ProjectUri));
        result.Properties.Add(new PSNoteProperty("PublishedDate", null));
        result.Properties.Add(new PSNoteProperty("ReleaseNotes", GetPsObjectPropertyValue(psData, "ReleaseNotes")));
        result.Properties.Add(new PSNoteProperty("Repository", null));
        result.Properties.Add(new PSNoteProperty("RepositorySourceLocation", null));
        result.Properties.Add(new PSNoteProperty("Tags", GetModuleTags(module, psData)));
        result.Properties.Add(new PSNoteProperty("Type", typeValue));
        result.Properties.Add(new PSNoteProperty("UpdatedDate", null));
        result.Properties.Add(new PSNoteProperty("Version", module.Version));
        result.TypeNames.Insert(0, "Microsoft.PowerShell.PSResourceGet.UtilClasses.PSResourceInfo");
        return result;
    }

    private static object[] GetModuleTags(PSModuleInfo module, object? psData)
    {
        object? tagValue = GetPsObjectPropertyValue(psData, "Tags");
        if (tagValue is IEnumerable<object> objectEnumerable)
        {
            return objectEnumerable.Select(value => value?.ToString()).Where(value => !string.IsNullOrWhiteSpace(value)).Cast<object>().ToArray();
        }

        if (tagValue is IEnumerable<string> stringEnumerable)
        {
            return stringEnumerable.Where(value => !string.IsNullOrWhiteSpace(value)).Cast<object>().ToArray();
        }

        if (module.Tags is not null)
        {
            return module.Tags.Where(value => !string.IsNullOrWhiteSpace(value)).Cast<object>().ToArray();
        }

        return Array.Empty<object>();
    }

    private static object GetPsResourceModuleTypeValue()
    {
        Type? resourceType = AppDomain.CurrentDomain
            .GetAssemblies()
            .Select(assembly => assembly.GetType("Microsoft.PowerShell.PSResourceGet.UtilClasses.ResourceType", throwOnError: false))
            .FirstOrDefault(type => type is not null);

        if (resourceType is not null)
        {
            return Enum.Parse(resourceType, "Module");
        }

        return 1;
    }

    private static bool IsModuleResourceObject(PSObject resource)
    {
        object? typeValue = resource.Properties["Type"]?.Value;
        string? typeString = typeValue?.ToString();
        return string.Equals(typeString, "Module", StringComparison.OrdinalIgnoreCase)
            || string.Equals(typeString, "1", StringComparison.OrdinalIgnoreCase);
    }

    private static void InvokeSavePsResourceByName(
        string[] name,
        string? version,
        string[]? repository,
        PSCredential? credential,
        bool trustRepository,
        bool quiet,
        bool prerelease,
        bool acceptLicense,
        bool skipDependencyCheck,
        bool authenticodeCheck,
        string? temporaryPath,
        string moduleVenvPath)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("Microsoft.PowerShell.PSResourceGet\\Save-PSResource");
        powerShell.AddParameter("Name", name);
        powerShell.AddParameter("Path", moduleVenvPath);
        powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
        AddStringParameter(powerShell, "Version", version);
        AddStringArrayParameter(powerShell, "Repository", repository);
        AddObjectParameter(powerShell, "Credential", credential);
        AddStringParameter(powerShell, "TemporaryPath", temporaryPath);
        AddSwitchParameter(powerShell, "TrustRepository", trustRepository);
        AddSwitchParameterIfSupported(
            powerShell,
            "Microsoft.PowerShell.PSResourceGet\\Save-PSResource",
            "Quiet",
            quiet);
        AddSwitchParameter(powerShell, "Prerelease", prerelease);
        AddSwitchParameter(powerShell, "AcceptLicense", acceptLicense);
        AddSwitchParameter(powerShell, "SkipDependencyCheck", skipDependencyCheck);
        AddSwitchParameter(powerShell, "AuthenticodeCheck", authenticodeCheck);
        _ = powerShell.Invoke();
    }

    private static void InvokeSavePsResourceByInputObject(
        PSObject[] inputObject,
        string[]? repository,
        PSCredential? credential,
        bool trustRepository,
        bool quiet,
        bool prerelease,
        bool acceptLicense,
        bool skipDependencyCheck,
        bool authenticodeCheck,
        string? temporaryPath,
        string moduleVenvPath)
    {
        using PowerShell powerShell = PowerShell.Create(RunspaceMode.CurrentRunspace);
        powerShell.AddCommand("Microsoft.PowerShell.PSResourceGet\\Save-PSResource");
        powerShell.AddParameter("InputObject", inputObject);
        powerShell.AddParameter("Path", moduleVenvPath);
        powerShell.AddParameter("ErrorAction", ActionPreference.Stop);
        AddStringArrayParameter(powerShell, "Repository", repository);
        AddObjectParameter(powerShell, "Credential", credential);
        AddStringParameter(powerShell, "TemporaryPath", temporaryPath);
        AddSwitchParameter(powerShell, "TrustRepository", trustRepository);
        AddSwitchParameterIfSupported(
            powerShell,
            "Microsoft.PowerShell.PSResourceGet\\Save-PSResource",
            "Quiet",
            quiet);
        AddSwitchParameter(powerShell, "Prerelease", prerelease);
        AddSwitchParameter(powerShell, "AcceptLicense", acceptLicense);
        AddSwitchParameter(powerShell, "SkipDependencyCheck", skipDependencyCheck);
        AddSwitchParameter(powerShell, "AuthenticodeCheck", authenticodeCheck);
        _ = powerShell.Invoke();
    }

    private static string GetPsResourceResultKey(PSObject resource)
    {
        string name = resource.Properties["Name"]?.Value?.ToString() ?? string.Empty;
        string version = resource.Properties["Version"]?.Value?.ToString() ?? string.Empty;
        string installedLocation = resource.Properties["InstalledLocation"]?.Value?.ToString() ?? string.Empty;
        return string.Join("|", name, version, installedLocation);
    }

    private static object? GetPsObjectPropertyValue(object? target, string propertyName)
    {
        return target is null ? null : PSObject.AsPSObject(target).Properties[propertyName]?.Value;
    }

    private static string? GetPsObjectPropertyString(object? target, string propertyName)
    {
        return GetPsObjectPropertyValue(target, propertyName)?.ToString();
    }

    private static void AddSwitchParameter(PowerShell powerShell, string name, bool value)
    {
        if (value)
        {
            powerShell.AddParameter(name);
        }
    }

    private static void AddSwitchParameterIfSupported(
        PowerShell powerShell,
        string commandName,
        string parameterName,
        bool value)
    {
        if (!value)
        {
            return;
        }

        if (CommandSupportsParameter(commandName, parameterName))
        {
            powerShell.AddParameter(parameterName);
        }
    }

    private static bool CommandSupportsParameter(string commandName, string parameterName)
    {
        using PowerShell commandLookup = PowerShell.Create(RunspaceMode.CurrentRunspace);
        commandLookup.AddCommand("Microsoft.PowerShell.Core\\Get-Command");
        commandLookup.AddParameter("Name", commandName);

        CommandInfo? commandInfo = commandLookup.Invoke<CommandInfo>().FirstOrDefault();
        if (commandInfo is null)
        {
            return false;
        }

        return commandInfo.Parameters.ContainsKey(parameterName);
    }
}