use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;

use crate::context::HostfxrContext;
use crate::host_detect::pwsh_host_detect;
use crate::hostfxr::load_hostfxr_from_pwsh_dir;
use crate::pdcstr;
use crate::pdcstring::PdCString;
use crate::startup_hook::{STARTUP_HOOK_ASSEMBLY_NAME, STARTUP_HOOK_DLL};

const STARTUP_HOOKS_ENV_VAR: &str = "PWSH_HOST_STARTUP_HOOKS";
const MODULE_VENV_PATH_ENV_VAR: &str = "PSMODULE_VENV_PATH";
const LOG_PATH_ENV_VAR: &str = "PWSH_STARTUP_HOOK_LOG_PATH";
const STRATEGY_ENV_VAR: &str = "PWSH_STARTUP_HOOK_STRATEGY";

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
/// # Safety
/// `target` must point to a writable JIT mapping, and `bytes` must point to
/// `length` readable bytes that do not overlap the target.
unsafe extern "C" fn patch_macos_jit(target: *mut libc::c_void, bytes: *const libc::c_void, length: usize) {
    extern "C" {
        fn pthread_jit_write_protect_np(enabled: libc::c_int);
        fn sys_icache_invalidate(start: *mut libc::c_void, length: libc::size_t);
    }

    // SAFETY: The managed hook supplies valid, non-overlapping patch buffers.
    // The write-protection transition and copy must both execute in native code.
    unsafe {
        pthread_jit_write_protect_np(0);
        libc::memcpy(target, bytes, length);
        pthread_jit_write_protect_np(1);
        sys_icache_invalidate(target, length);
    }
}

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
    module_venv_path: Option<&OsString>,
    log_path: Option<&OsString>,
    strategy: Option<&OsString>,
) -> StartupHooksTarget {
    match startup_hooks {
        Some(path) => StartupHooksTarget::Path(path),
        None if module_venv_path.is_some() || log_path.is_some() || strategy.is_some() => {
            StartupHooksTarget::EmbeddedAssemblyName
        }
        None => StartupHooksTarget::None,
    }
}

pub(crate) fn configure_startup_hooks_for_context<I>(
    context: &HostfxrContext<'_, I>,
) -> Result<(), Box<dyn std::error::Error>> {
    let startup_hooks = take_env_var(STARTUP_HOOKS_ENV_VAR);
    let module_venv_path = take_env_var(MODULE_VENV_PATH_ENV_VAR);
    let log_path = take_env_var(LOG_PATH_ENV_VAR);
    let strategy = take_env_var(STRATEGY_ENV_VAR);
    let startup_hooks = resolve_startup_hooks(
        startup_hooks,
        module_venv_path.as_ref(),
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
            let startup_hooks_pd = PdCString::from_os_str(STARTUP_HOOK_ASSEMBLY_NAME)?;
            context.set_runtime_property_value(pdcstr!("STARTUP_HOOKS"), &startup_hooks_pd)?;
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if module_venv_path.is_some() {
        let address = PdCString::from_os_str(format!("{:x}", patch_macos_jit as *const () as usize))?;
        context.set_runtime_property_value(pdcstr!("PWSH_HOST_NATIVE_JIT_PATCH"), &address)?;
    }

    if let Some(module_venv_path) = module_venv_path {
        let module_venv_path_pd = PdCString::from_os_str(&module_venv_path)?;
        context.set_runtime_property_value(pdcstr!("PSMODULE_VENV_PATH"), &module_venv_path_pd)?;
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
        context.load_assembly_bytes_in_default_context(STARTUP_HOOK_DLL, None)?;
    }

    Ok(())
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
    configure_startup_hooks_for_context(&context)?;

    Ok(context.run_app())
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod tests {
    use super::patch_macos_jit;

    #[test]
    fn macos_jit_patch_rewrites_executable_code() {
        let initial: [u32; 2] = [0x528000e0, 0xd65f03c0];
        let replacement: [u32; 2] = [0x52800540, 0xd65f03c0];
        let length = std::mem::size_of_val(&initial);

        // SAFETY: The mapping is a private JIT page, and both patches are valid
        // arm64 functions with the declared signature.
        unsafe {
            let memory = libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            );
            assert_ne!(memory, libc::MAP_FAILED, "failed to allocate a JIT page");

            patch_macos_jit(memory, initial.as_ptr().cast(), length);
            let execute: unsafe extern "C" fn() -> u32 = std::mem::transmute(memory);
            let first = execute();

            patch_macos_jit(memory, replacement.as_ptr().cast(), length);
            let second = execute();
            let unmapped = libc::munmap(memory, length);

            assert_eq!(unmapped, 0, "failed to release the JIT page");
            assert_eq!((first, second), (7, 42), "JIT patch did not update executable code");
        }
    }
}
