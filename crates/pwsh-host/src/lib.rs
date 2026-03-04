mod bindings;
mod cli_xml;
mod context;
mod delegate_loader;
mod error;
mod host_detect;
mod host_exit_code;
mod hostfxr;
mod loader;
mod named_pipe_command;
mod pwsh_cli;
mod tests;
mod time;

extern crate libc;
#[macro_use]
extern crate dlopen_derive;
extern crate dlopen;
#[macro_use]
extern crate quick_error;

/// Module for a platform dependent c-like string type.
#[macro_use]
mod pdcstring;

pub use bindings::PowerShell;
pub use named_pipe_command::{preprocess_named_pipe_command_args, NamedPipeCommandError};
pub use pwsh_cli::{run_pwsh_command_line, run_pwsh_command_line_for_pwsh_dir, run_pwsh_command_line_for_pwsh_exe};
