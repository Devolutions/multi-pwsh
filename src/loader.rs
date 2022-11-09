use crate::delegate_loader::AssemblyDelegateLoader;
use crate::host_detect::pwsh_host_detect;
use crate::host_exit_code::HostExitCode;
use crate::hostfxr::load_hostfxr;
use crate::pdcstr;
use crate::pdcstring::PdCString;

pub const BINDINGS_DLL: &[u8] = include_bytes!("../dotnet/bin/Release/net6.0/Bindings.dll");

pub fn get_assembly_delegate_loader() -> AssemblyDelegateLoader<PdCString> {
    let pwsh_path = pwsh_host_detect();
    assert!(pwsh_path.is_ok());
    let pwsh_path = pwsh_path.unwrap();

    let hostfxr = load_hostfxr();
    assert!(hostfxr.is_ok());
    let hostfxr = hostfxr.unwrap();

    let ctx = hostfxr.initialize_for_dotnet_command_line(pwsh_path.join("pwsh.dll"));
    assert!(ctx.is_ok());
    let ctx = ctx.unwrap();

    let assembly_path = PdCString::from_os_str(
        pwsh_path
            .join("System.Management.Automation.dll")
            .into_os_string(),
    );
    assert!(assembly_path.is_ok());
    let assembly_path = assembly_path.unwrap();

    let fn_loader = ctx.get_delegate_loader_for_assembly(assembly_path);
    assert!(fn_loader.is_ok());
    let fn_loader = fn_loader.unwrap();

    let load_assembly_from_native_memory = fn_loader.get_function_pointer_for_unmanaged_callers_only_method(
        pdcstr!("System.Management.Automation.PowerShellUnsafeAssemblyLoad, System.Management.Automation"),
    pdcstr!("LoadAssemblyFromNativeMemory"));
    assert!(load_assembly_from_native_memory.is_ok());
    let load_assembly_from_native_memory = load_assembly_from_native_memory.unwrap();

    let load_assembly_from_native_memory: extern "system" fn(
        bytes: *const libc::c_uchar,
        size: libc::c_uint,
    ) -> i32 = unsafe { std::mem::transmute(load_assembly_from_native_memory) };
    let result =
        (load_assembly_from_native_memory)(BINDINGS_DLL.as_ptr(), BINDINGS_DLL.len() as u32);
    HostExitCode::from(result).into_result().unwrap();

    fn_loader
}
