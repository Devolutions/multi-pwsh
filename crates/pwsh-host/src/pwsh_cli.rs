use std::ffi::OsStr;

use crate::host_detect::pwsh_host_detect;
use crate::hostfxr::load_hostfxr;
use crate::pdcstring::PdCString;

pub fn run_pwsh_command_line<I, A>(args: I) -> Result<i32, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let pwsh_dir = pwsh_host_detect()?;
    let pwsh_dll = pwsh_dir.join("pwsh.dll");

    let mut host_args = vec![PdCString::from_os_str(pwsh_dll)?];
    for arg in args {
        host_args.push(PdCString::from_os_str(arg)?);
    }

    let hostfxr = load_hostfxr()?;
    let context = hostfxr.initialize_for_dotnet_command_line_args(&host_args)?;
    Ok(context.run_app())
}
