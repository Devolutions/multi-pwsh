use std::fs;
use std::path::PathBuf;

use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error, PartialEq)]
pub enum EnvError {
    #[error("PATH undefined or unset in the environment.")]
    UndefOrUnset,
    #[error("PowerShell install dir not found in PATH")]
    Missing,
}

pub fn find_pwsh_exe() -> Option<PathBuf> {
    if let Ok(pwsh_exe) = which::which("pwsh") {
        if let Ok(pwsh_link) = fs::read_link(&pwsh_exe) {
            return Some(pwsh_link);
        } else {
            return Some(pwsh_exe);
        }
    }
    None
}

pub fn find_pwsh_dir() -> Option<PathBuf> {
    if let Some(mut pwsh_exe) = find_pwsh_exe() {
        pwsh_exe.pop();
        return Some(pwsh_exe);
    }
    None
}

#[allow(dead_code)]
pub fn pwsh_host_detect() -> Result<PathBuf, EnvError> {
    find_pwsh_dir().ok_or(EnvError::Missing)
}
