using System;
using System.IO;
using System.Reflection;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading;

public static partial class StartupHook
{
    private const string ForceModulePathProperty = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
    private const string LogPathProperty = "PWSH_STARTUP_HOOK_LOG_PATH";
    private const string StrategyProperty = "PWSH_STARTUP_HOOK_STRATEGY";
    private const string PowerShellGetPatchHelperName = "__PWSH_HOST_PATCH_POWERSHELLGET_VENV";
    private const string VenvInstalledModuleHelperName = "__PWSH_HOST_GET_VENV_INSTALLED_MODULE";
    private const string VenvInstalledPSResourceHelperName = "__PWSH_HOST_GET_VENV_INSTALLED_PSRESOURCE";
    private const string InstallModuleWrapperName = "Install-Module";
    private const string GetInstalledModuleWrapperName = "Get-InstalledModule";
    private const string InstallPSResourceWrapperName = "Install-PSResource";
    private const string GetInstalledPSResourceWrapperName = "Get-InstalledPSResource";

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
            BeginModuleManagementCommandOverrides();

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

    private static void BeginModuleManagementCommandOverrides()
    {
        Thread thread = new(() =>
        {
            try
            {
                WaitForPrimaryRunspaceAndInstallModuleManagementOverrides();
            }
            catch (Exception ex)
            {
                WriteLog($"failed to install module-management overrides: {ex}");
            }
        })
        {
            IsBackground = true,
            Name = "pwsh-host-module-management-overrides",
        };

        thread.Start();
    }

    private static void WaitForPrimaryRunspaceAndInstallModuleManagementOverrides()
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
                InstallModuleManagementOverrides(executionContext);
                WriteLog("installed module-management command overrides");
                return;
            }

            Thread.Sleep(1);
        }

        WriteLog("timed out waiting for primary runspace to install module-management overrides");
    }

    private static void InstallModuleManagementOverrides(object executionContext)
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
        InstallFunction(setFunction, sessionState, VenvInstalledPSResourceHelperName, BuildVenvInstalledPSResourceHelperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, InstallModuleWrapperName, BuildInstallModuleWrapperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, GetInstalledModuleWrapperName, BuildGetInstalledModuleWrapperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, InstallPSResourceWrapperName, BuildInstallPSResourceWrapperScript(escapedForcedModulePath));
        InstallFunction(setFunction, sessionState, GetInstalledPSResourceWrapperName, BuildGetInstalledPSResourceWrapperScript(escapedForcedModulePath));
    }

    private static void InstallFunction(MethodInfo setFunction, object sessionState, string functionName, string script)
    {
        ScriptBlock scriptBlock = ScriptBlock.Create(script);
        _ = setFunction.Invoke(sessionState, new object[] { functionName, scriptBlock, true });
    }
}