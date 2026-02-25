use std::borrow::BorrowMut;
use std::ffi::OsStr;

use dlopen::wrapper::{Container, WrapperApi};

use crate::context::{HostfxrContext, HostfxrHandle, InitializedForCommandLine};
use crate::host_detect::pwsh_host_detect;
use crate::pdcstring::{PdCStr, PdCString};

#[cfg(windows)]
#[allow(non_camel_case_types)]
pub type char_t = u16;
/// The char type used in nethost and hostfxr. Either u8 on unix systems or u16 on windows.
#[allow(non_camel_case_types)]
#[cfg(not(windows))]
pub type char_t = i8;

/// [`UnmanagedCallersOnlyAttribute`]: https://docs.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.unmanagedcallersonlyattribute
pub const UNMANAGED_CALLERS_ONLY_METHOD: *const char_t = usize::MAX as *const _;

#[repr(i32)]
#[allow(dead_code)]
pub enum HostfxrDelegateType {
    ComActivation,
    LoadInMemoryAssembly,
    WinrtActivation,
    ComRegister,
    ComUnregister,
    LoadAssemblyAndGetFunctionPointer,
    GetFunctionPointer,
}

#[repr(C)]
pub struct HostfxrInitializeParameters {
    pub size: usize,
    pub host_path: Box<PdCStr>,   //*const char_t,
    pub dotnet_root: Box<PdCStr>, //*const char_t,
}

pub type Hostfxrhandle = *mut libc::c_void;

#[derive(WrapperApi)]
pub struct HostfxrLib {
    hostfxr_initialize_for_dotnet_command_line: unsafe extern "C" fn(
        argc: i32,
        argv: *const *const char_t,
        parameters: *const HostfxrInitializeParameters,
        host_context_handle: Hostfxrhandle,
    ) -> i32,
    hostfxr_initialize_for_runtime_config: unsafe extern "C" fn(
        runtime_config_path: *const char_t,
        parameters: *const HostfxrInitializeParameters,
        host_context_handle: *mut Hostfxrhandle,
    ) -> i32,
    hostfxr_get_runtime_property_value:
        unsafe extern "C" fn(host_context_handle: Hostfxrhandle, name: *const char_t, value: *mut *const char_t) -> i32,
    hostfxr_set_runtime_property_value:
        unsafe extern "C" fn(host_context_handle: Hostfxrhandle, name: *const char_t, value: *const char_t) -> i32,
    hostfxr_get_runtime_properties: unsafe extern "C" fn(
        host_context_handle: Hostfxrhandle,
        count: *mut libc::size_t,
        keys: *mut *const char_t,
        values: *mut *const char_t,
    ) -> i32,
    hostfxr_run_app: unsafe extern "C" fn(host_context_handle: Hostfxrhandle) -> i32,
    hostfxr_get_runtime_delegate: unsafe extern "C" fn(
        host_context_handle: Hostfxrhandle,
        delegate_type: HostfxrDelegateType,
        delegate: *mut libc::c_void,
    ) -> i32,
    hostfxr_close: unsafe extern "C" fn(host_context_handle: Hostfxrhandle) -> i32,
}

impl HostfxrLib {
    #[allow(dead_code)]
    fn load_lib(path: impl AsRef<OsStr>) -> Result<Container<Self>, Box<dyn std::error::Error>> {
        Ok(unsafe { Container::load(path)? })
    }
}

pub type LoadAssemblyAndGetFunctionPointerFn = unsafe extern "system" fn(
    assembly_path: *const char_t,
    type_name: *const char_t,
    method_name: *const char_t,
    delegate_type_name: *const char_t,
    reserved: *const (),
    delegate: *mut libc::c_void,
) -> i32;

pub type GetFunctionPointerFn = unsafe extern "system" fn(
    type_name: *const char_t,
    method_name: *const char_t,
    delegate_type_name: *const char_t,
    load_context: *const (),
    reserved: *const (),
    delegate: *mut libc::c_void,
) -> i32;

pub struct Hostfxr {
    pub lib: Container<HostfxrLib>,
}

impl Hostfxr {
    #[allow(dead_code)]
    pub fn load_from_path(path: impl AsRef<OsStr>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            lib: HostfxrLib::load_lib(path)?,
        })
    }

    #[allow(dead_code)]
    pub fn initialize_for_dotnet_command_line(
        &self,
        pwsh_path: impl AsRef<OsStr>,
    ) -> Result<HostfxrContext<'_, InitializedForCommandLine>, Box<dyn std::error::Error>> {
        use std::ptr;

        use crate::host_exit_code::HostExitCode;

        let args = &[PdCString::from_os_str(pwsh_path)?];
        let mut host_context_handle = ptr::null::<Hostfxrhandle>() as Hostfxrhandle;

        let result = unsafe {
            self.lib.hostfxr_initialize_for_dotnet_command_line(
                args.len() as i32,
                args.as_ptr() as *const *const char_t,
                ptr::null(),
                host_context_handle.borrow_mut() as *mut _ as Hostfxrhandle, //Initialise nullptr
            )
        };

        HostExitCode::from(result).into_result()?;

        Ok(HostfxrContext::new(
            unsafe { HostfxrHandle::new_unchecked(host_context_handle) },
            self,
        ))
    }

    #[allow(dead_code)]
    pub fn initialize_for_runtime_config(
        &self,
        runtime_config_path: impl AsRef<PdCStr>,
        parameters: Box<HostfxrInitializeParameters>, //*const HostfxrInitializeParameters,
        host_context_handle: *mut Hostfxrhandle,
    ) -> i32 {
        unsafe {
            self.lib.hostfxr_initialize_for_runtime_config(
                runtime_config_path.as_ref().as_ptr(),
                parameters.as_ref(),
                host_context_handle,
            )
        }
    }

    #[allow(dead_code)]
    pub fn get_runtime_property_value(
        &self,
        host_context_handle: Hostfxrhandle,
        name: impl AsRef<PdCStr>,  //*const char_t,
        value: impl AsRef<PdCStr>, //*mut *const char_t,
    ) -> i32 {
        unsafe {
            self.lib.hostfxr_get_runtime_property_value(
                host_context_handle,
                name.as_ref().as_ptr(),
                value.as_ref().as_ptr().borrow_mut(),
            )
        }
    }

    #[allow(dead_code)]
    pub fn set_runtime_property_value(
        &self,
        host_context_handle: Hostfxrhandle,
        name: impl AsRef<PdCStr>,  //*const char_t,
        value: impl AsRef<PdCStr>, //*const char_t,
    ) -> i32 {
        unsafe {
            self.lib.hostfxr_set_runtime_property_value(
                host_context_handle,
                name.as_ref().as_ptr(),
                value.as_ref().as_ptr(),
            )
        }
    }

    #[allow(dead_code)]
    pub fn get_runtime_properties(
        &self,
        host_context_handle: Hostfxrhandle,
        count: &mut usize,          //*mut libc::size_t,
        keys: impl AsRef<PdCStr>,   //*mut *const char_t,
        values: impl AsRef<PdCStr>, //*mut *const char_t,
    ) -> i32 {
        unsafe {
            self.lib.hostfxr_get_runtime_properties(
                host_context_handle,
                count,
                keys.as_ref().as_ptr().borrow_mut(),
                values.as_ref().as_ptr().borrow_mut(),
            )
        }
    }

    #[allow(dead_code)]
    pub fn run_app(&self, host_context_handle: Hostfxrhandle) -> i32 {
        unsafe { self.lib.hostfxr_run_app(host_context_handle) }
    }

    #[allow(dead_code)]
    pub fn get_runtime_delegate(
        &self,
        host_context_handle: Hostfxrhandle,
        delegate_type: HostfxrDelegateType,
        delegate: &mut libc::c_void, //*mut libc::c_void,
    ) -> i32 {
        unsafe {
            self.lib
                .hostfxr_get_runtime_delegate(host_context_handle, delegate_type, delegate)
        }
    }

    #[allow(dead_code)]
    pub fn close(&self, host_context_handle: Hostfxrhandle) -> i32 {
        unsafe { self.lib.hostfxr_close(host_context_handle) }
    }
}

#[allow(dead_code)]
pub fn load_hostfxr() -> Result<Hostfxr, Box<dyn std::error::Error>> {
    let pwsh_path = pwsh_host_detect()?;
    Hostfxr::load_from_path(pwsh_path.join(if cfg!(target_os = "windows") {
        "hostfxr.dll"
    } else if cfg!(target_os = "linux") {
        "libhostfxr.so"
    } else {
        "libhostfxr.dylib"
    }))
}
