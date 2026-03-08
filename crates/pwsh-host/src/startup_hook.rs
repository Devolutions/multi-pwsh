pub const STARTUP_HOOK_FORCE_MODULE_PATH_ENV_VAR: &str = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
pub const STARTUP_HOOK_STRATEGY_ENV_VAR: &str = "PWSH_STARTUP_HOOK_STRATEGY";
pub const MODULE_PATH_STRATEGY: &str = "module-path";
pub(crate) const STARTUP_HOOK_ASSEMBLY_NAME: &str = "Devolutions.PowerShell.SDK.StartupHook";

pub(crate) const STARTUP_HOOK_DLL: &[u8] =
    include_bytes!("../../../dotnet/startup-hook/bin/Release/net8.0/Devolutions.PowerShell.SDK.StartupHook.dll");
