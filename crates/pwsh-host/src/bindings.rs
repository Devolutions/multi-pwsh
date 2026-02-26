#![allow(dead_code)]

use std::ffi::{CStr, CString};

use crate::delegate_loader::{AssemblyDelegateLoader, MethodWithUnknownSignature};
use crate::error::Error;
use crate::loader::get_assembly_delegate_loader;
use crate::pdcstr;
use crate::pdcstring::{PdCStr, PdCString};

pub type PowerShellHandle = *mut libc::c_void;

pub type FnPowerShellCreate = unsafe extern "system" fn() -> PowerShellHandle;

pub type FnPowerShellAddArgumentString =
    unsafe extern "system" fn(handle: PowerShellHandle, argument: *const libc::c_char);

pub type FnPowerShellAddParameterString =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: *const libc::c_char);

pub type FnPowerShellAddParameterInt =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: i32);

pub type FnPowerShellAddParameterLong =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: i64);

pub type FnPowerShellAddCommand = unsafe extern "system" fn(handle: PowerShellHandle, command: *const libc::c_char);

pub type FnPowerShellAddScript = unsafe extern "system" fn(handle: PowerShellHandle, script: *const libc::c_char);

pub type FnPowerShellAddStatement = unsafe extern "system" fn(handle: PowerShellHandle);

pub type FnPowerShellInvoke = unsafe extern "system" fn(handle: PowerShellHandle);

pub type FnPowerShellClear = unsafe extern "system" fn(handle: PowerShellHandle);

pub type FnPowerShellExportToXml =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char;

pub type FnPowerShellExportToJson =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char;

pub type FnPowerShellExportToString =
    unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char;

pub type FnMarshalFreeCoTaskMem = unsafe extern "system" fn(ptr: *mut libc::c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiPs74 {
    pub create_fn: *const libc::c_void,
    pub add_argument_string_fn: *const libc::c_void,
    pub add_parameter_string_fn: *const libc::c_void,
    pub add_parameter_int_fn: *const libc::c_void,
    pub add_parameter_long_fn: *const libc::c_void,
    pub add_command_fn: *const libc::c_void,
    pub add_script_fn: *const libc::c_void,
    pub add_statement_fn: *const libc::c_void,
    pub invoke_fn: *const libc::c_void,
    pub clear_fn: *const libc::c_void,
    pub export_to_xml_fn: *const libc::c_void,
    pub export_to_json_fn: *const libc::c_void,
    pub export_to_string_fn: *const libc::c_void,
    pub marshal_free_co_task_mem_fn: *const libc::c_void,
}

pub type FnBindingsGetApiPs74 = unsafe extern "system" fn() -> *const ApiPs74;

struct Bindings {
    create_fn: FnPowerShellCreate,
    add_argument_string_fn: FnPowerShellAddArgumentString,
    add_parameter_string_fn: FnPowerShellAddParameterString,
    add_parameter_int_fn: FnPowerShellAddParameterInt,
    add_parameter_long_fn: FnPowerShellAddParameterLong,
    add_command_fn: FnPowerShellAddCommand,
    add_script_fn: FnPowerShellAddScript,
    add_statement_fn: FnPowerShellAddStatement,
    invoke_fn: FnPowerShellInvoke,
    clear_fn: FnPowerShellClear,
    export_to_xml_fn: FnPowerShellExportToXml,
    export_to_json_fn: FnPowerShellExportToJson,
    export_to_string_fn: FnPowerShellExportToString,
    marshal_free_co_task_mem_fn: FnMarshalFreeCoTaskMem,
}

impl Bindings {
    pub fn new() -> Result<Self, Error> {
        let fn_loader = get_assembly_delegate_loader();
        Self::new_with_loader(&fn_loader)
    }

    pub fn new_with_loader(fn_loader: &AssemblyDelegateLoader<PdCString>) -> Result<Self, Error> {
        fn get_function_pointer(
            fn_loader: &AssemblyDelegateLoader<PdCString>,
            type_name: impl AsRef<PdCStr>,
            method_name: impl AsRef<PdCStr>,
        ) -> Result<MethodWithUnknownSignature, Error> {
            fn_loader.get_function_pointer_for_unmanaged_callers_only_method(type_name, method_name)
        }

        let get_api_ps74_fn: FnBindingsGetApiPs74 = {
            let fn_ptr = get_function_pointer(
                fn_loader,
                pdcstr!("NativeHost.Bindings, Bindings"),
                pdcstr!("Bindings_GetApiPS74"),
            )?;
            unsafe { std::mem::transmute(fn_ptr) }
        };

        let api = unsafe {
            let api_ptr = get_api_ps74_fn();
            assert!(!api_ptr.is_null());
            *api_ptr
        };

        let pwsh = Self {
            create_fn: unsafe { std::mem::transmute(api.create_fn) },
            add_argument_string_fn: unsafe { std::mem::transmute(api.add_argument_string_fn) },
            add_parameter_string_fn: unsafe { std::mem::transmute(api.add_parameter_string_fn) },
            add_parameter_int_fn: unsafe { std::mem::transmute(api.add_parameter_int_fn) },
            add_parameter_long_fn: unsafe { std::mem::transmute(api.add_parameter_long_fn) },
            add_command_fn: unsafe { std::mem::transmute(api.add_command_fn) },
            add_script_fn: unsafe { std::mem::transmute(api.add_script_fn) },
            add_statement_fn: unsafe { std::mem::transmute(api.add_statement_fn) },
            invoke_fn: unsafe { std::mem::transmute(api.invoke_fn) },
            clear_fn: unsafe { std::mem::transmute(api.clear_fn) },
            export_to_xml_fn: unsafe { std::mem::transmute(api.export_to_xml_fn) },
            export_to_json_fn: unsafe { std::mem::transmute(api.export_to_json_fn) },
            export_to_string_fn: unsafe { std::mem::transmute(api.export_to_string_fn) },
            marshal_free_co_task_mem_fn: unsafe { std::mem::transmute(api.marshal_free_co_task_mem_fn) },
        };
        Ok(pwsh)
    }
}

pub struct PowerShell {
    inner: Bindings,
    handle: PowerShellHandle,
}

impl PowerShell {
    pub fn new() -> Option<Self> {
        let bindings = Bindings::new().ok()?;
        let handle = unsafe { (bindings.create_fn)() };
        Some(Self {
            inner: bindings,
            handle: handle,
        })
    }

    pub fn add_argument_string(&self, argument: &str) {
        let argument_cstr = CString::new(argument).unwrap();
        unsafe {
            (self.inner.add_argument_string_fn)(self.handle, argument_cstr.as_ptr());
        }
    }

    pub fn add_parameter_string(&self, name: &str, value: &str) {
        let name_cstr = CString::new(name).unwrap();
        let value_cstr = CString::new(value).unwrap();
        unsafe {
            (self.inner.add_parameter_string_fn)(self.handle, name_cstr.as_ptr(), value_cstr.as_ptr());
        }
    }

    pub fn add_parameter_int(&self, name: &str, value: i32) {
        let name_cstr = CString::new(name).unwrap();
        unsafe {
            (self.inner.add_parameter_int_fn)(self.handle, name_cstr.as_ptr(), value);
        }
    }

    pub fn add_parameter_long(&self, name: &str, value: i64) {
        let name_cstr = CString::new(name).unwrap();
        unsafe {
            (self.inner.add_parameter_long_fn)(self.handle, name_cstr.as_ptr(), value);
        }
    }

    pub fn add_command(&self, command: &str) {
        let command_cstr = CString::new(command).unwrap();
        unsafe {
            (self.inner.add_command_fn)(self.handle, command_cstr.as_ptr());
        }
    }

    pub fn add_script(&self, script: &str) {
        let script_cstr = CString::new(script).unwrap();
        unsafe {
            (self.inner.add_script_fn)(self.handle, script_cstr.as_ptr());
        }
    }

    pub fn add_statement(&self) {
        unsafe {
            (self.inner.add_statement_fn)(self.handle);
        }
    }

    pub fn invoke(&self, clear: bool) {
        unsafe {
            (self.inner.invoke_fn)(self.handle);
            if clear {
                (self.inner.clear_fn)(self.handle);
            }
        }
    }

    pub fn clear(&self) {
        unsafe {
            (self.inner.clear_fn)(self.handle);
        }
    }

    pub fn export_to_xml(&self, name: &str) -> String {
        unsafe {
            let name_cstr = CString::new(name).unwrap();
            let cstr_ptr = (self.inner.export_to_xml_fn)(self.handle, name_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn export_to_json(&self, name: &str) -> String {
        unsafe {
            let name_cstr = CString::new(name).unwrap();
            let cstr_ptr = (self.inner.export_to_json_fn)(self.handle, name_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn export_to_string(&self, name: &str) -> String {
        unsafe {
            let name_cstr = CString::new(name).unwrap();
            let cstr_ptr = (self.inner.export_to_string_fn)(self.handle, name_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    fn marshal_free_co_task_mem(&self, ptr: *mut libc::c_void) {
        unsafe {
            (self.inner.marshal_free_co_task_mem_fn)(ptr);
        }
    }
}
