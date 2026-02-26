#![allow(dead_code)]

mod bindings_generated;

use std::ffi::{CStr, CString};

use self::bindings_generated::{Bindings, PowerShellHandle};

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

    pub fn invoke_member_json(&self, member_name: &str, arguments_json: &str) -> String {
        unsafe {
            let member_name_cstr = CString::new(member_name).unwrap();
            let arguments_json_cstr = CString::new(arguments_json).unwrap();
            let cstr_ptr = (self.inner.invoke_member_json_fn)(
                self.handle,
                member_name_cstr.as_ptr(),
                arguments_json_cstr.as_ptr(),
            );
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn get_property_json(&self, property_name: &str) -> String {
        unsafe {
            let property_name_cstr = CString::new(property_name).unwrap();
            let cstr_ptr = (self.inner.get_property_json_fn)(self.handle, property_name_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn set_property_json(&self, property_name: &str, value_json: &str) -> String {
        unsafe {
            let property_name_cstr = CString::new(property_name).unwrap();
            let value_json_cstr = CString::new(value_json).unwrap();
            let cstr_ptr =
                (self.inner.set_property_json_fn)(self.handle, property_name_cstr.as_ptr(), value_json_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn invoke_static_member_json(&self, member_name: &str, arguments_json: &str) -> String {
        unsafe {
            let member_name_cstr = CString::new(member_name).unwrap();
            let arguments_json_cstr = CString::new(arguments_json).unwrap();
            let cstr_ptr =
                (self.inner.invoke_static_member_json_fn)(member_name_cstr.as_ptr(), arguments_json_cstr.as_ptr());
            let cstr = CStr::from_ptr(cstr_ptr);
            let rstr = String::from_utf8_lossy(cstr.to_bytes()).to_string();
            self.marshal_free_co_task_mem(cstr_ptr as *mut libc::c_void);
            rstr
        }
    }

    pub fn free_handle(&self, handle: PowerShellHandle) {
        unsafe {
            (self.inner.gc_handle_free_fn)(handle);
        }
    }

    fn marshal_free_co_task_mem(&self, ptr: *mut libc::c_void) {
        unsafe {
            (self.inner.marshal_free_co_task_mem_fn)(ptr);
        }
    }
}
