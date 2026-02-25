use core::ptr;
use std::borrow::BorrowMut;

use crate::error::Error;
use crate::host_exit_code::HostExitCode;
use crate::hostfxr::{
    char_t, GetFunctionPointerFn, LoadAssemblyAndGetFunctionPointerFn, UNMANAGED_CALLERS_ONLY_METHOD,
};
use crate::pdcstring::PdCStr;

#[derive(Copy, Clone)]
pub struct DelegateLoader {
    pub get_load_assembly_and_get_function_pointer: LoadAssemblyAndGetFunctionPointerFn,
    pub get_function_pointer: GetFunctionPointerFn,
}

pub type MethodWithUnknownSignature = *mut libc::c_void;

impl DelegateLoader {
    #[allow(dead_code)]
    pub fn load_assembly_and_get_function_pointer(
        &self,
        assembly_path: impl AsRef<PdCStr>,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
        delegate_type_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        unsafe {
            self._load_assembly_and_get_function_pointer(
                assembly_path.as_ref().as_ptr(),
                type_name.as_ref().as_ptr(),
                method_name.as_ref().as_ptr(),
                delegate_type_name.as_ref().as_ptr(),
            )
        }
    }

    unsafe fn _load_assembly_and_get_function_pointer(
        &self,
        assembly_path: *const char_t,
        type_name: *const char_t,
        method_name: *const char_t,
        delegate_type_name: *const char_t,
    ) -> Result<MethodWithUnknownSignature, Error> {
        let mut delegate = ptr::null::<*mut libc::c_void>() as *mut libc::c_void;

        let result = (self.get_load_assembly_and_get_function_pointer)(
            assembly_path,
            type_name,
            method_name,
            delegate_type_name,
            ptr::null(),
            delegate.borrow_mut() as *mut _ as *mut libc::c_void, //Initialise nullptr
        );
        HostExitCode::from(result).into_result()?;
        Ok(delegate)
    }

    #[allow(dead_code)]
    pub fn get_function_pointer(
        &self,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
        delegate_type_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        unsafe {
            self._get_function_pointer(
                type_name.as_ref().as_ptr(),
                method_name.as_ref().as_ptr(),
                delegate_type_name.as_ref().as_ptr(),
            )
        }
    }

    unsafe fn _get_function_pointer(
        &self,
        type_name: *const char_t,
        method_name: *const char_t,
        delegate_type_name: *const char_t,
    ) -> Result<MethodWithUnknownSignature, Error> {
        let mut delegate = ptr::null::<*mut libc::c_void>() as *mut libc::c_void;

        let result = (self.get_function_pointer)(
            type_name,
            method_name,
            delegate_type_name,
            ptr::null(),
            ptr::null(),
            delegate.borrow_mut() as *mut _ as *mut libc::c_void, //Initialise nullptr
        );
        HostExitCode::from(result).into_result()?;
        Ok(delegate)
    }

    #[allow(dead_code)]
    pub fn load_assembly_and_get_function_pointer_for_unmanaged_callers_only_method(
        &self,
        assembly_path: impl AsRef<PdCStr>,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        unsafe {
            self._load_assembly_and_get_function_pointer(
                assembly_path.as_ref().as_ptr(),
                type_name.as_ref().as_ptr(),
                method_name.as_ref().as_ptr(),
                UNMANAGED_CALLERS_ONLY_METHOD,
            )
        }
    }

    #[allow(dead_code)]
    pub fn get_function_pointer_for_unmanaged_callers_only_method(
        &self,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        unsafe {
            self._get_function_pointer(
                type_name.as_ref().as_ptr(),
                method_name.as_ref().as_ptr(),
                UNMANAGED_CALLERS_ONLY_METHOD,
            )
        }
    }
}

pub struct AssemblyDelegateLoader<A: AsRef<PdCStr>> {
    loader: DelegateLoader,
    assembly_path: A,
}

impl<A: AsRef<PdCStr>> AssemblyDelegateLoader<A> {
    pub fn new(loader: DelegateLoader, assembly_path: A) -> Self {
        Self { loader, assembly_path }
    }

    #[allow(dead_code)]
    pub fn get_function_pointer(
        &self,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
        delegate_type_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        self.loader.load_assembly_and_get_function_pointer(
            self.assembly_path.as_ref(),
            type_name,
            method_name,
            delegate_type_name,
        )
    }

    #[allow(dead_code)]
    pub fn get_function_pointer_for_unmanaged_callers_only_method(
        &self,
        type_name: impl AsRef<PdCStr>,
        method_name: impl AsRef<PdCStr>,
    ) -> Result<MethodWithUnknownSignature, Error> {
        self.loader
            .load_assembly_and_get_function_pointer_for_unmanaged_callers_only_method(
                self.assembly_path.as_ref(),
                type_name,
                method_name,
            )
    }
}
