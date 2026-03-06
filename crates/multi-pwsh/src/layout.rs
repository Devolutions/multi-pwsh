use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;

use crate::error::{MultiPwshError, Result};
use crate::platform::HostOs;

pub struct InstallLayout {
    home: PathBuf,
    bin_dir: PathBuf,
    cache_dir: PathBuf,
    os: HostOs,
}

impl InstallLayout {
    pub fn new(os: HostOs) -> Result<Self> {
        let user_home = home::home_dir().ok_or(MultiPwshError::HomeDirectoryNotFound)?;
        let home = env::var_os("MULTI_PWSH_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_home.join(".pwsh"));
        let bin_dir = env::var_os("MULTI_PWSH_BIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("bin"));
        let cache_dir = env::var_os("MULTI_PWSH_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("cache"));

        Ok(InstallLayout {
            home,
            bin_dir,
            cache_dir,
            os,
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.bin_dir.clone()
    }

    pub fn aliases_file(&self) -> PathBuf {
        self.home.join("aliases.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir.clone()
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.home.join("multi")
    }

    pub fn preferred_version_dir(&self, version: &Version) -> PathBuf {
        self.versions_dir().join(version.to_string())
    }

    fn legacy_version_dir(&self, version: &Version) -> PathBuf {
        self.home.join(version.to_string())
    }

    pub fn executable_name(&self) -> &'static str {
        self.os.executable_name()
    }

    pub fn version_dir(&self, version: &Version) -> PathBuf {
        let preferred = self.preferred_version_dir(version);
        if preferred.exists() {
            return preferred;
        }

        let legacy = self.legacy_version_dir(version);
        if legacy.exists() {
            return legacy;
        }

        preferred
    }

    pub fn version_executable(&self, version: &Version) -> PathBuf {
        self.version_dir(version).join(self.executable_name())
    }

    pub fn version_install_dir(&self, version: &Version) -> PathBuf {
        self.preferred_version_dir(version)
    }

    pub fn remove_version_dirs(&self, version: &Version) -> Result<bool> {
        let mut removed = false;

        for path in [self.preferred_version_dir(version), self.legacy_version_dir(version)] {
            if path.exists() {
                fs::remove_dir_all(path)?;
                removed = true;
            }
        }

        Ok(removed)
    }

    pub fn ensure_base_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.home)?;
        fs::create_dir_all(self.bin_dir())?;
        fs::create_dir_all(self.cache_dir())?;
        fs::create_dir_all(self.versions_dir())?;
        Ok(())
    }

    pub fn installed_versions(&self) -> Result<Vec<Version>> {
        if !self.home.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        collect_versions_from_dir(&self.versions_dir(), &[], self.executable_name(), &mut versions)?;
        collect_versions_from_dir(
            self.home(),
            &["bin", "cache", "multi"],
            self.executable_name(),
            &mut versions,
        )?;

        versions.sort_by(|a, b| b.cmp(a));
        Ok(versions)
    }
}

fn collect_versions_from_dir(
    base_dir: &Path,
    excluded_names: &[&str],
    executable_name: &str,
    versions: &mut Vec<Version>,
) -> Result<()> {
    if !base_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(base_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if excluded_names.iter().any(|name| *name == file_name) {
            continue;
        }

        let version = match Version::parse(file_name.as_ref()) {
            Ok(version) => version,
            Err(_) => continue,
        };

        let executable = path.join(executable_name);
        if executable.exists() && !versions.contains(&version) {
            versions.push(version);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_var<T>(key: &str, value: Option<&Path>, action: impl FnOnce() -> T) -> T {
        let previous = env::var_os(key);

        match value {
            Some(value) => unsafe { env::set_var(key, value) },
            None => unsafe { env::remove_var(key) },
        }

        let result = action();

        match previous {
            Some(value) => unsafe { env::set_var(key, value) },
            None => unsafe { env::remove_var(key) },
        }

        result
    }

    fn with_layout_env<T>(
        home: Option<&Path>,
        bin_dir: Option<&Path>,
        cache_dir: Option<&Path>,
        action: impl FnOnce() -> T,
    ) -> T {
        let _guard = ENV_LOCK.lock().unwrap();

        with_env_var("MULTI_PWSH_HOME", home, || {
            with_env_var("MULTI_PWSH_BIN_DIR", bin_dir, || {
                with_env_var("MULTI_PWSH_CACHE_DIR", cache_dir, action)
            })
        })
    }

    #[test]
    fn layout_uses_home_override_and_derived_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");

        with_layout_env(Some(&expected_home), None, None, || {
            let layout = InstallLayout::new(HostOs::Windows).unwrap();
            assert_eq!(layout.home(), expected_home.as_path());
            assert_eq!(layout.bin_dir(), expected_home.join("bin"));
            assert_eq!(layout.cache_dir(), expected_home.join("cache"));
            assert_eq!(layout.versions_dir(), expected_home.join("multi"));
            assert_eq!(layout.aliases_file(), expected_home.join("aliases.json"));
        });
    }

    #[test]
    fn layout_uses_explicit_bin_and_cache_overrides() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");
        let expected_bin = temp_dir.path().join("shims");
        let expected_cache = temp_dir.path().join("cache-root");

        with_layout_env(Some(&expected_home), Some(&expected_bin), Some(&expected_cache), || {
            let layout = InstallLayout::new(HostOs::Linux).unwrap();
            assert_eq!(layout.home(), expected_home.as_path());
            assert_eq!(layout.bin_dir(), expected_bin);
            assert_eq!(layout.cache_dir(), expected_cache);
            assert_eq!(layout.versions_dir(), expected_home.join("multi"));
        });
    }

    #[test]
    fn version_dir_falls_back_to_legacy_location() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");
        let version = Version::parse("7.4.13").unwrap();
        let legacy_dir = expected_home.join(version.to_string());
        fs::create_dir_all(&legacy_dir).unwrap();

        with_layout_env(Some(&expected_home), None, None, || {
            let layout = InstallLayout::new(HostOs::Linux).unwrap();
            assert_eq!(layout.version_dir(&version), legacy_dir);
            assert_eq!(
                layout.version_install_dir(&version),
                expected_home.join("multi").join("7.4.13")
            );
        });
    }

    #[test]
    fn installed_versions_include_new_and_legacy_locations() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");
        let new_version = Version::parse("7.5.0").unwrap();
        let legacy_version = Version::parse("7.4.13").unwrap();

        let new_dir = expected_home.join("multi").join(new_version.to_string());
        let legacy_dir = expected_home.join(legacy_version.to_string());
        fs::create_dir_all(&new_dir).unwrap();
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(new_dir.join("pwsh"), "").unwrap();
        fs::write(legacy_dir.join("pwsh"), "").unwrap();

        with_layout_env(Some(&expected_home), None, None, || {
            let layout = InstallLayout::new(HostOs::Linux).unwrap();
            assert_eq!(layout.installed_versions().unwrap(), vec![new_version, legacy_version]);
        });
    }
}
