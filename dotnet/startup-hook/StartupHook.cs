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
    private const string ImportModuleCmdletHelperName = "Import-PWSHHostModule";
    private const string InstallModuleCmdletHelperName = "Install-PWSHHostModule";
    private const string GetInstalledModuleCmdletHelperName = "Get-PWSHHostInstalledModule";
    private const string GetPSRepositoryCmdletHelperName = "Get-PWSHHostPSRepository";
    private const string SetPSRepositoryCmdletHelperName = "Set-PWSHHostPSRepository";
    private const string RegisterPSRepositoryCmdletHelperName = "Register-PWSHHostPSRepository";
    private const string UnregisterPSRepositoryCmdletHelperName = "Unregister-PWSHHostPSRepository";
    private const string InstallPSResourceCmdletHelperName = "Install-PWSHHostPSResource";
    private const string GetInstalledPSResourceCmdletHelperName = "Get-PWSHHostInstalledPSResource";
    private const string ImportModuleCommandName = "Import-Module";
    private const string InstallModuleCommandName = "Install-Module";
    private const string GetInstalledModuleCommandName = "Get-InstalledModule";
    private const string GetPSRepositoryCommandName = "Get-PSRepository";
    private const string SetPSRepositoryCommandName = "Set-PSRepository";
    private const string RegisterPSRepositoryCommandName = "Register-PSRepository";
    private const string UnregisterPSRepositoryCommandName = "Unregister-PSRepository";
    private const string InstallPSResourceCommandName = "Install-PSResource";
    private const string GetInstalledPSResourceCommandName = "Get-InstalledPSResource";

    private static string? s_moduleVenvPath;
    private static string? s_logPath;
    private static string? s_strategy;

    private static string? GetModuleVenvPath()
    {
        return string.IsNullOrWhiteSpace(s_moduleVenvPath) ? null : s_moduleVenvPath;
    }

    private static string? GetModuleVenvModulesPath()
    {
        string? moduleVenvPath = GetModuleVenvPath();
        return string.IsNullOrWhiteSpace(moduleVenvPath) ? null : Path.Combine(moduleVenvPath, "Modules");
    }

    private static string? GetModuleVenvScriptsPath()
    {
        string? moduleVenvPath = GetModuleVenvPath();
        return string.IsNullOrWhiteSpace(moduleVenvPath) ? null : Path.Combine(moduleVenvPath, "Scripts");
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
        string? moduleVenvPath = GetModuleVenvModulesPath();
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

            Environment.SetEnvironmentVariable(ModuleVenvPathProperty, s_moduleVenvPath);

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
        MethodInfo addSessionStateEntry = sessionState.GetType().GetMethod(
            "AddSessionStateEntry",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(SessionStateCmdletEntry) },
            modifiers: null)!;
        MethodInfo setAliasValue = sessionState.GetType().GetMethod(
            "SetAliasValue",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            binder: null,
            types: new[] { typeof(string), typeof(string), typeof(ScopedItemOptions), typeof(bool), typeof(CommandOrigin) },
            modifiers: null)!;

        InstallCmdlet(addSessionStateEntry, sessionState, ImportModuleCmdletHelperName, typeof(StartupHookImportModuleCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, InstallModuleCmdletHelperName, typeof(StartupHookInstallModuleCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, GetInstalledModuleCmdletHelperName, typeof(StartupHookGetInstalledModuleCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, GetPSRepositoryCmdletHelperName, typeof(StartupHookGetPSRepositoryCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, SetPSRepositoryCmdletHelperName, typeof(StartupHookSetPSRepositoryCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, RegisterPSRepositoryCmdletHelperName, typeof(StartupHookRegisterPSRepositoryCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, UnregisterPSRepositoryCmdletHelperName, typeof(StartupHookUnregisterPSRepositoryCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, InstallPSResourceCmdletHelperName, typeof(StartupHookInstallPSResourceCommand));
        InstallCmdlet(addSessionStateEntry, sessionState, GetInstalledPSResourceCmdletHelperName, typeof(StartupHookGetInstalledPSResourceCommand));
        InstallAlias(setAliasValue, sessionState, ImportModuleCommandName, ImportModuleCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, InstallModuleCommandName, InstallModuleCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, GetInstalledModuleCommandName, GetInstalledModuleCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, GetPSRepositoryCommandName, GetPSRepositoryCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, SetPSRepositoryCommandName, SetPSRepositoryCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, RegisterPSRepositoryCommandName, RegisterPSRepositoryCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, UnregisterPSRepositoryCommandName, UnregisterPSRepositoryCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, InstallPSResourceCommandName, InstallPSResourceCmdletHelperName);
        InstallAlias(setAliasValue, sessionState, GetInstalledPSResourceCommandName, GetInstalledPSResourceCmdletHelperName);
    }

    private static void InstallCmdlet(MethodInfo addSessionStateEntry, object sessionState, string commandName, Type implementingType)
    {
        SessionStateCmdletEntry entry = new(commandName, implementingType, helpFileName: null);
        _ = addSessionStateEntry.Invoke(sessionState, new object[] { entry });
    }

    private static void InstallAlias(MethodInfo setAliasValue, object sessionState, string aliasName, string targetName)
    {
        _ = setAliasValue.Invoke(
            sessionState,
            new object[] { aliasName, targetName, ScopedItemOptions.AllScope, true, CommandOrigin.Internal }
        );
    }
}
