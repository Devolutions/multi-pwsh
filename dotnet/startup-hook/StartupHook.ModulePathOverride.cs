using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;

public static partial class StartupHook
{
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
    private static IEnumerable<string> GetEnumeratedModulePathReplacement(bool includeSystemModulePath, object context)
    {
        var yieldedPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
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
}