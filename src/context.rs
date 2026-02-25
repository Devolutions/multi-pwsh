use core::{mem, ptr};
use std::borrow::BorrowMut;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::delegate_loader::{AssemblyDelegateLoader, DelegateLoader, MethodWithUnknownSignature};
use crate::error::Error;
use crate::host_exit_code::HostExitCode;
use crate::hostfxr::{
    GetFunctionPointerFn, Hostfxr, HostfxrDelegateType, Hostfxrhandle, LoadAssemblyAndGetFunctionPointerFn,
};
use crate::pdcstring::PdCStr;

/// A marker struct indicating that the context was initialized for the dotnet command line.
/// This means that it is possible to run the application associated with the context.
#[allow(dead_code)]
pub struct InitializedForCommandLine;

#[derive(Debug, Clone, Copy)]
pub struct HostfxrHandle(NonNull<()>);

impl HostfxrHandle {
    #[allow(dead_code)]
    pub unsafe fn new_unchecked(ptr: Hostfxrhandle) -> Self {
        Self(NonNull::new_unchecked(ptr as *mut _))
    }

    #[allow(dead_code)]
    pub fn as_raw(&self) -> Hostfxrhandle {
        self.0.as_ptr() as Hostfxrhandle
    }
}

#[derive(Clone)]
pub struct HostfxrContext<'a, I> {
    handle: HostfxrHandle,
    hostfxr: &'a Hostfxr,
    context_type: PhantomData<&'a I>,
}

impl<'a, I> HostfxrContext<'a, I> {
    #[allow(dead_code)]
    pub fn new(handle: HostfxrHandle, hostfxr: &'a Hostfxr) -> Self {
        Self {
            handle,
            hostfxr,
            context_type: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn get_runtime_delegate(
        &self,
        delegate_type: HostfxrDelegateType,
    ) -> Result<MethodWithUnknownSignature, Error> {
        let mut delegate = ptr::null::<*mut libc::c_void>() as *mut libc::c_void;
        let result = unsafe {
            self.hostfxr.lib.hostfxr_get_runtime_delegate(
                self.handle.as_raw(),
                delegate_type,
                delegate.borrow_mut() as *mut _ as *mut libc::c_void, //Initialise nullptr
            )
        };
        HostExitCode::from(result).into_result()?;
        Ok(delegate)
    }

    #[allow(dead_code)]
    fn get_load_assembly_and_get_function_pointer_delegate(
        &self,
    ) -> Result<LoadAssemblyAndGetFunctionPointerFn, Error> {
        unsafe {
            self.get_runtime_delegate(HostfxrDelegateType::LoadAssemblyAndGetFunctionPointer)
                .map(|ptr| mem::transmute(ptr))
        }
    }

    #[allow(dead_code)]
    fn get_get_function_pointer_delegate(&self) -> Result<GetFunctionPointerFn, Error> {
        unsafe {
            self.get_runtime_delegate(HostfxrDelegateType::GetFunctionPointer)
                .map(|ptr| mem::transmute(ptr))
        }
    }

    #[allow(dead_code)]
    pub fn get_delegate_loader(&self) -> Result<DelegateLoader, Error> {
        Ok(DelegateLoader {
            get_load_assembly_and_get_function_pointer: self.get_load_assembly_and_get_function_pointer_delegate()?,
            get_function_pointer: self.get_get_function_pointer_delegate()?,
        })
    }

    #[allow(dead_code)]
    pub fn get_delegate_loader_for_assembly<A: AsRef<PdCStr>>(
        &self,
        assembly_path: A,
    ) -> Result<AssemblyDelegateLoader<A>, Error> {
        self.get_delegate_loader()
            .map(|loader| AssemblyDelegateLoader::new(loader, assembly_path))
    }
}
