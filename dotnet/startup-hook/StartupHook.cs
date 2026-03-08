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
    private const string ModuleVenvPathProperty = "PSMODULE_VENV_PATH";
    private const string LegacyForceModulePathProperty = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
    private const string LogPathProperty = "PWSH_STARTUP_HOOK_LOG_PATH";
    private const string StrategyProperty = "PWSH_STARTUP_HOOK_STRATEGY";
    private const string PowerShellGetPatchHelperName = "__PWSH_HOST_PATCH_POWERSHELLGET_VENV";
    private const string VenvInstalledModuleHelperName = "__PWSH_HOST_GET_VENV_INSTALLED_MODULE";
    private const string VenvInstalledPSResourceHelperName = "__PWSH_HOST_GET_VENV_INSTALLED_PSRESOURCE";
    private const string InstallModuleWrapperHelperName = "__PWSH_HOST_INSTALL_MODULE_WRAPPER";
    private const string GetInstalledModuleWrapperHelperName = "__PWSH_HOST_GET_INSTALLED_MODULE_WRAPPER";
    private const string InstallPSResourceWrapperHelperName = "__PWSH_HOST_INSTALL_PSRESOURCE_WRAPPER";
    private const string GetInstalledPSResourceWrapperHelperName = "__PWSH_HOST_GET_INSTALLED_PSRESOURCE_WRAPPER";
    private const string InstallModuleWrapperName = "Install-Module";
    private const string GetInstalledModuleWrapperName = "Get-InstalledModule";
    private const string InstallPSResourceWrapperName = "Install-PSResource";
    private const string GetInstalledPSResourceWrapperName = "Get-InstalledPSResource";

    private static string? s_moduleVenvPath;
    private static string? s_logPath;
    private static string? s_strategy;

    private static string? GetModuleVenvPath()
    {
        return string.IsNullOrWhiteSpace(s_moduleVenvPath) ? null : s_moduleVenvPath;
    }

    private static string GetPsHomeModulesPath()
    {
        string psHomeModulesPath;
        try
        {
            string smaLocation = Assembly.Load("System.Management.Automation").Location;
            string? smaDirectory = Path.GetDirectoryName(smaLocation);
            psHomeModulesPath = string.IsNullOrWhiteSpace(smaDirectory)
                ? Path.Combine(AppContext.BaseDirectory, "Modules")
                : Path.Combine(smaDirectory, "Modules");
        }
        catch
        {
            psHomeModulesPath = Path.Combine(AppContext.BaseDirectory, "Modules");
        }

        return psHomeModulesPath;
    }

    private static string GetEffectivePsModulePath()
    {
        string? moduleVenvPath = GetModuleVenvPath();
        string psHomeModulesPath = GetPsHomeModulesPath();

        if (string.IsNullOrWhiteSpace(moduleVenvPath))
        {
            return psHomeModulesPath;
        }

        if (!Directory.Exists(psHomeModulesPath))
        {
            return moduleVenvPath;
        }

        return string.Join(Path.PathSeparator, new[] { moduleVenvPath, psHomeModulesPath });
    }

    public static void Initialize()
    {
        s_moduleVenvPath = ReadConfigurationValue(ModuleVenvPathProperty)
            ?? ReadConfigurationValue(LegacyForceModulePathProperty);
        s_logPath = ReadConfigurationValue(LogPathProperty);
        s_strategy = ReadConfigurationValue(StrategyProperty);

        Environment.SetEnvironmentVariable("DOTNET_STARTUP_HOOKS", null);
        Environment.SetEnvironmentVariable("PSMODULE_VENV_PATH", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_LOG_PATH", null);
        Environment.SetEnvironmentVariable("PWSH_STARTUP_HOOK_STRATEGY", null);

        try
        {
            WriteLog($"startup hook entered; initial PSModulePath={Environment.GetEnvironmentVariable("PSModulePath")}");

            if (string.IsNullOrWhiteSpace(s_moduleVenvPath))
            {
                WriteLog("no module venv path provided");
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
            Environment.SetEnvironmentVariable("PSModulePath", GetEffectivePsModulePath());
            RuntimeHelpers.RunClassConstructor(moduleIntrinsics.TypeHandle);
            WriteLog($"configured module-path override; rewritten PSModulePath={Environment.GetEnvironmentVariable("PSModulePath")}");
            BeginModuleManagementCommandOverrides();

            Environment.SetEnvironmentVariable("PSModulePath", GetEffectivePsModulePath());
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
        MethodInfo setAliasValue = sessionState.GetType().GetMethod(
            "SetAliasValue",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(string), typeof(ScopedItemOptions), typeof(bool), typeof(CommandOrigin) },
            modifiers: null)!;

        string escapedModuleVenvPath = (s_moduleVenvPath ?? string.Empty).Replace("'", "''", StringComparison.Ordinal);
        InstallFunction(setFunction, sessionState, PowerShellGetPatchHelperName, BuildPowerShellGetPatchHelperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, VenvInstalledModuleHelperName, BuildVenvInstalledModuleHelperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, VenvInstalledPSResourceHelperName, BuildVenvInstalledPSResourceHelperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, InstallModuleWrapperHelperName, BuildInstallModuleWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, GetInstalledModuleWrapperHelperName, BuildGetInstalledModuleWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, InstallPSResourceWrapperHelperName, BuildInstallPSResourceWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, GetInstalledPSResourceWrapperHelperName, BuildGetInstalledPSResourceWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, InstallModuleWrapperName, BuildInstallModuleWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, GetInstalledModuleWrapperName, BuildGetInstalledModuleWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, InstallPSResourceWrapperName, BuildInstallPSResourceWrapperScript(escapedModuleVenvPath));
        InstallFunction(setFunction, sessionState, GetInstalledPSResourceWrapperName, BuildGetInstalledPSResourceWrapperScript(escapedModuleVenvPath));
        InstallAlias(setAliasValue, sessionState, InstallModuleWrapperName, InstallModuleWrapperHelperName);
        InstallAlias(setAliasValue, sessionState, GetInstalledModuleWrapperName, GetInstalledModuleWrapperHelperName);
        InstallAlias(setAliasValue, sessionState, InstallPSResourceWrapperName, InstallPSResourceWrapperHelperName);
        InstallAlias(setAliasValue, sessionState, GetInstalledPSResourceWrapperName, GetInstalledPSResourceWrapperHelperName);
    }

    private static void InstallFunction(MethodInfo setFunction, object sessionState, string functionName, string script)
    {
        ScriptBlock scriptBlock = ScriptBlock.Create(script);
        _ = setFunction.Invoke(sessionState, new object[] { functionName, scriptBlock, true });
    }

    private static void InstallAlias(MethodInfo setAliasValue, object sessionState, string aliasName, string targetName)
    {
        _ = setAliasValue.Invoke(
            sessionState,
            new object[] { aliasName, targetName, ScopedItemOptions.AllScope, true, CommandOrigin.Internal }
        );
    }
}