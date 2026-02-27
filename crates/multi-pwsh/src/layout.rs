use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;

use crate::error::{MultiPwshError, Result};
use crate::platform::HostOs;

pub struct InstallLayout {
    root: PathBuf,
    os: HostOs,
}

impl InstallLayout {
    pub fn new(os: HostOs) -> Result<Self> {
        let home = home::home_dir().ok_or(MultiPwshError::HomeDirectoryNotFound)?;
        Ok(InstallLayout {
            root: home.join(".pwsh"),
            os,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn aliases_file(&self) -> PathBuf {
        self.root.join("aliases.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        match env::var_os("MULTI_PWSH_CACHE_DIR") {
            Some(value) => PathBuf::from(value),
            None => self.root.join("cache"),
        }
    }

    pub fn executable_name(&self) -> &'static str {
        self.os.executable_name()
    }

    pub fn version_dir(&self, version: &Version) -> PathBuf {
        self.root.join(version.to_string())
    }

    pub fn version_executable(&self, version: &Version) -> PathBuf {
        self.version_dir(version).join(self.executable_name())
    }

    pub fn ensure_base_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(self.bin_dir())?;
        fs::create_dir_all(self.cache_dir())?;
        Ok(())
    }

    pub fn installed_versions(&self) -> Result<Vec<Version>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name == "bin" || file_name == "cache" {
                continue;
            }

            let version = match Version::parse(file_name.as_ref()) {
                Ok(version) => version,
                Err(_) => continue,
            };

            let executable = path.join(self.executable_name());
            if executable.exists() {
                versions.push(version);
            }
        }

        versions.sort_by(|a, b| b.cmp(a));
        Ok(versions)
    }
}
