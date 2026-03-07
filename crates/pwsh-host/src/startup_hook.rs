pub const PROVIDER_UNIFY_FORCE_MODULE_PATH_ENV_VAR: &str = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
pub const PROVIDER_UNIFY_STRATEGY_ENV_VAR: &str = "PWSH_STARTUP_HOOK_STRATEGY";
pub const PROVIDER_UNIFY_STRATEGY: &str = "provider-unify";
pub(crate) const PROVIDER_UNIFY_STARTUP_HOOK_ASSEMBLY_NAME: &str = "PwshModulePathStartupHook";

pub(crate) const STARTUP_HOOK_DLL: &[u8] =
    include_bytes!("../../../dotnet/startup-hook/bin/Release/net8.0/PwshModulePathStartupHook.dll");
