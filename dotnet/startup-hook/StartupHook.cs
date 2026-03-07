using System;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

public static class StartupHook
{
    private const string ForceModulePathProperty = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
    private const string LogPathProperty = "PWSH_STARTUP_HOOK_LOG_PATH";
    private const string StrategyProperty = "PWSH_STARTUP_HOOK_STRATEGY";

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