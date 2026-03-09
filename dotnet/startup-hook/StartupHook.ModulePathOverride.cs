using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;

public static partial class StartupHook
{
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string GetModuleVenvPathReplacement()
    {
        return GetModuleVenvPath() ?? string.Empty;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string GetEffectiveModulePathReplacement()
    {
        string effectiveModulePath = GetEffectivePsModulePath();
        Environment.SetEnvironmentVariable("PSModulePath", effectiveModulePath);
        return effectiveModulePath;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string GetConfigModulePathReplacement(object powerShellConfig, int scope)
    {
        return GetModuleVenvPath() ?? string.Empty;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static IEnumerable<string> GetEnumeratedModulePathReplacement(bool includeSystemModulePath, object context)
    {
        var yieldedPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        string? moduleVenvPath = GetModuleVenvPath();

        if (!string.IsNullOrWhiteSpace(moduleVenvPath) && yieldedPaths.Add(moduleVenvPath))
        {
            yield return moduleVenvPath;
        }

        string? psHomeModulePath = GetPsHomeModulesPath();

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
        MethodInfo setModulePath = moduleIntrinsics.GetMethod(
            "SetModulePath",
            BindingFlags.Static | BindingFlags.NonPublic,
            binder: null,
            types: Type.EmptyTypes,
            modifiers: null)!;
        MethodInfo getComposedModulePath = moduleIntrinsics.GetMethod(
            "GetModulePath",
            BindingFlags.Static | BindingFlags.Public,
            binder: null,
            types: new[] { typeof(string), typeof(string), typeof(string) },
            modifiers: null)!;
        MethodInfo pathReplacement = typeof(StartupHook).GetMethod(
            nameof(GetModuleVenvPathReplacement),
            BindingFlags.NonPublic | BindingFlags.Static)!;
        MethodInfo enumeratedPathReplacement = typeof(StartupHook).GetMethod(
            nameof(GetEnumeratedModulePathReplacement),
            BindingFlags.NonPublic | BindingFlags.Static)!;
        MethodInfo effectiveModulePathReplacement = typeof(StartupHook).GetMethod(
            nameof(GetEffectiveModulePathReplacement),
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
        PatchMethod(setModulePath, effectiveModulePathReplacement);
        PatchMethod(getComposedModulePath, effectiveModulePathReplacement);
        PatchMethod(getConfigModulePath, configReplacement);
    }
}