use crate::error::{MultiPwshError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostOs {
    Windows,
    Macos,
    Linux,
}

impl HostOs {
    pub fn detect() -> Result<Self> {
        match std::env::consts::OS {
            "windows" => Ok(HostOs::Windows),
            "macos" => Ok(HostOs::Macos),
            "linux" => Ok(HostOs::Linux),
            value => Err(MultiPwshError::UnsupportedPlatform(format!(
                "operating system '{}' is not supported",
                value
            ))),
        }
    }

    pub fn executable_name(self) -> &'static str {
        match self {
            HostOs::Windows => "pwsh.exe",
            HostOs::Macos | HostOs::Linux => "pwsh",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostArch {
    X64,
    X86,
    Arm64,
    Arm32,
}

impl HostArch {
    pub fn detect() -> Self {
        match std::env::consts::ARCH {
            "x86_64" => HostArch::X64,
            "x86" | "i686" => HostArch::X86,
            "aarch64" => HostArch::Arm64,
            "arm" | "armv7" | "armv7l" => HostArch::Arm32,
            _ => HostArch::X64,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "x64" => Some(HostArch::X64),
            "x86" => Some(HostArch::X86),
            "arm64" => Some(HostArch::Arm64),
            "arm32" => Some(HostArch::Arm32),
            _ => None,
        }
    }
}
