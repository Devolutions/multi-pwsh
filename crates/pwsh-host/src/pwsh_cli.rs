use std::ffi::OsStr;
use std::path::Path;

use crate::host_detect::pwsh_host_detect;
use crate::hostfxr::load_hostfxr_from_pwsh_dir;
use crate::pdcstring::PdCString;

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
    Ok(context.run_app())
}
