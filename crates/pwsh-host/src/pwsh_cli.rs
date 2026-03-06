use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;

use crate::host_detect::pwsh_host_detect;
use crate::hostfxr::load_hostfxr_from_pwsh_dir;
use crate::pdcstr;
use crate::pdcstring::PdCString;
use crate::startup_hook::{PROVIDER_UNIFY_STARTUP_HOOK_ASSEMBLY_NAME, STARTUP_HOOK_DLL};

const STARTUP_HOOKS_ENV_VAR: &str = "PWSH_HOST_STARTUP_HOOKS";
const FORCE_MODULE_PATH_ENV_VAR: &str = "PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH";
const LOG_PATH_ENV_VAR: &str = "PWSH_STARTUP_HOOK_LOG_PATH";
const STRATEGY_ENV_VAR: &str = "PWSH_STARTUP_HOOK_STRATEGY";

fn take_env_var(name: &str) -> Option<std::ffi::OsString> {
    let value = std::env::var_os(name);
    if value.is_some() {
        unsafe {
            std::env::remove_var(name);
        }
    }

    value
}

enum StartupHooksTarget {
    None,
    Path(OsString),
    EmbeddedAssemblyName,
}

fn resolve_startup_hooks(
    startup_hooks: Option<OsString>,
    forced_module_path: Option<&OsString>,
    log_path: Option<&OsString>,
    strategy: Option<&OsString>,
) -> StartupHooksTarget {
    match startup_hooks {
        Some(path) => StartupHooksTarget::Path(path),
        None if forced_module_path.is_some() || log_path.is_some() || strategy.is_some() => {
            StartupHooksTarget::EmbeddedAssemblyName
        }
        None => StartupHooksTarget::None,
    }
}

pub fn run_pwsh_command_line<I, A>(args: I) -> Result<i32, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let pwsh_dir = pwsh_host_detect()?;
    run_pwsh_command_line_for_pwsh_dir(&pwsh_dir, args)
}

pub fn run_pwsh_command_line_for_pwsh_exe<I, A>(
    pwsh_exe_path: impl AsRef<Path>,
    args: I,
) -> Result<i32, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let pwsh_dir = pwsh_exe_path.as_ref().parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pwsh executable has no parent directory",
        )
    })?;
    run_pwsh_command_line_for_pwsh_dir(pwsh_dir, args)
}

pub fn run_pwsh_command_line_for_pwsh_dir<I, A>(
    pwsh_dir: impl AsRef<Path>,
    args: I,
) -> Result<i32, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let pwsh_dll = pwsh_dir.as_ref().join("pwsh.dll");

    let mut host_args = vec![PdCString::from_os_str(pwsh_dll)?];
    for arg in args {
        host_args.push(PdCString::from_os_str(arg)?);
    }

    let hostfxr = load_hostfxr_from_pwsh_dir(pwsh_dir)?;
    let context = hostfxr.initialize_for_dotnet_command_line_args(&host_args)?;

    let startup_hooks = take_env_var(STARTUP_HOOKS_ENV_VAR);
    let forced_module_path = take_env_var(FORCE_MODULE_PATH_ENV_VAR);
    let log_path = take_env_var(LOG_PATH_ENV_VAR);
    let strategy = take_env_var(STRATEGY_ENV_VAR);
    let startup_hooks = resolve_startup_hooks(
        startup_hooks,
        forced_module_path.as_ref(),
        log_path.as_ref(),
        strategy.as_ref(),
    );

    match &startup_hooks {
        StartupHooksTarget::None => {}
        StartupHooksTarget::Path(startup_hooks) => {
            let startup_hooks_pd = PdCString::from_os_str(startup_hooks)?;
            context.set_runtime_property_value(pdcstr!("STARTUP_HOOKS"), &startup_hooks_pd)?;
        }
        StartupHooksTarget::EmbeddedAssemblyName => {
            let startup_hooks_pd = PdCString::from_os_str(PROVIDER_UNIFY_STARTUP_HOOK_ASSEMBLY_NAME)?;
            context.set_runtime_property_value(pdcstr!("STARTUP_HOOKS"), &startup_hooks_pd)?;
        }
    }

    if let Some(forced_module_path) = forced_module_path {
        let forced_module_path_pd = PdCString::from_os_str(&forced_module_path)?;
        context.set_runtime_property_value(pdcstr!("PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH"), &forced_module_path_pd)?;
    }

    if let Some(log_path) = log_path {
        let log_path_pd = PdCString::from_os_str(&log_path)?;
        context.set_runtime_property_value(pdcstr!("PWSH_STARTUP_HOOK_LOG_PATH"), &log_path_pd)?;
    }

    if let Some(strategy) = strategy {
        let strategy_pd = PdCString::from_os_str(&strategy)?;
        context.set_runtime_property_value(pdcstr!("PWSH_STARTUP_HOOK_STRATEGY"), &strategy_pd)?;
    }

    if let StartupHooksTarget::EmbeddedAssemblyName = startup_hooks {
        // STARTUP_HOOKS must be set before the runtime is materialized; after that,
        // we can preload the embedded hook bytes into the default load context so
        // StartupHookProvider resolves the simple assembly name without a temp DLL.
        context.load_assembly_bytes_in_default_context(STARTUP_HOOK_DLL, None)?;
    }

    Ok(context.run_app())
}
