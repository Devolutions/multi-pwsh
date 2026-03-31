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
    venvs_dir: PathBuf,
    versions_dir: PathBuf,
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
            .unwrap_or_else(|| join_layout_path(&home, "bin"));
        let cache_dir = env::var_os("MULTI_PWSH_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| join_layout_path(&home, "cache"));
        let venvs_dir = env::var_os("MULTI_PWSH_VENV_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| join_layout_path(&home, "venv"));
        let versions_dir = join_layout_path(&home, "multi");

        Ok(InstallLayout {
            home,
            bin_dir,
            cache_dir,
            venvs_dir,
            versions_dir,
            os,
        })
    }

    pub fn from_root(os: HostOs, home: PathBuf) -> Result<Self> {
        Self::from_root_with_versions_dir(os, home.clone(), home.join("multi"))
    }

    pub fn from_root_with_versions_dir(os: HostOs, home: PathBuf, versions_dir: PathBuf) -> Result<Self> {
        Self::from_parts(
            os,
            home.clone(),
            join_layout_path(&home, "bin"),
            join_layout_path(&home, "cache"),
            join_layout_path(&home, "venv"),
            versions_dir,
        )
    }

    pub fn from_parts(
        os: HostOs,
        home: PathBuf,
        bin_dir: PathBuf,
        cache_dir: PathBuf,
        venvs_dir: PathBuf,
        versions_dir: PathBuf,
    ) -> Result<Self> {
        Ok(InstallLayout {
            home,
            bin_dir,
            cache_dir,
            venvs_dir,
            versions_dir,
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
        join_layout_path(&self.home, "aliases.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir.clone()
    }

    pub fn venvs_dir(&self) -> PathBuf {
        self.venvs_dir.clone()
    }

    pub fn venv_dir(&self, name: &str) -> PathBuf {
        join_layout_path(&self.venvs_dir(), name)
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.versions_dir.clone()
    }

    pub fn preferred_version_dir(&self, version: &Version) -> PathBuf {
        join_layout_path(&self.versions_dir(), &version.to_string())
    }

    fn legacy_version_dir(&self, version: &Version) -> PathBuf {
        join_layout_path(&self.home, &version.to_string())
    }

    pub fn executable_name(&self) -> &'static str {
        self.os.executable_name()
    }

    pub fn os(&self) -> HostOs {
        self.os
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
        fs::create_dir_all(self.venvs_dir())?;
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
            &["bin", "cache", "multi", "venv"],
            self.executable_name(),
            &mut versions,
        )?;

        versions.sort_by(|a, b| b.cmp(a));
        Ok(versions)
    }
}

fn join_layout_path(base: &Path, child: &str) -> PathBuf {
    let base_text = base.to_string_lossy();
    if !looks_like_windows_path(base_text.as_ref()) {
        return base.join(child);
    }

    let separator = if base_text.contains('\\') { '\\' } else { '/' };
    let mut path = base_text.to_string();
    if !path.ends_with('\\') && !path.ends_with('/') {
        path.push(separator);
    }
    path.push_str(child);
    PathBuf::from(path)
}

fn looks_like_windows_path(path: &str) -> bool {
    path.starts_with(r"\\") || path.starts_with("//") || (path.len() >= 2 && path.as_bytes()[1] == b':')
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
        venv_dir: Option<&Path>,
        action: impl FnOnce() -> T,
    ) -> T {
        let _guard = ENV_LOCK.lock().unwrap();

        with_env_var("MULTI_PWSH_HOME", home, || {
            with_env_var("MULTI_PWSH_BIN_DIR", bin_dir, || {
                with_env_var("MULTI_PWSH_CACHE_DIR", cache_dir, || {
                    with_env_var("MULTI_PWSH_VENV_DIR", venv_dir, action)
                })
            })
        })
    }

    #[test]
    fn layout_uses_home_override_and_derived_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");

        with_layout_env(Some(&expected_home), None, None, None, || {
            let layout = InstallLayout::new(HostOs::Windows).unwrap();
            assert_eq!(layout.home(), expected_home.as_path());
            assert_eq!(layout.bin_dir(), expected_home.join("bin"));
            assert_eq!(layout.cache_dir(), expected_home.join("cache"));
            assert_eq!(layout.venvs_dir(), expected_home.join("venv"));
            assert_eq!(layout.venv_dir("msgraph"), expected_home.join("venv").join("msgraph"));
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

        with_layout_env(
            Some(&expected_home),
            Some(&expected_bin),
            Some(&expected_cache),
            None,
            || {
                let layout = InstallLayout::new(HostOs::Linux).unwrap();
                assert_eq!(layout.home(), expected_home.as_path());
                assert_eq!(layout.bin_dir(), expected_bin);
                assert_eq!(layout.cache_dir(), expected_cache);
                assert_eq!(layout.venvs_dir(), expected_home.join("venv"));
                assert_eq!(layout.versions_dir(), expected_home.join("multi"));
            },
        );
    }

    #[test]
    fn layout_uses_explicit_venv_override() {
        let temp_dir = TempDir::new().unwrap();
        let expected_home = temp_dir.path().join("pwsh-home");
        let expected_venv = temp_dir.path().join("venvs-root");

        with_layout_env(Some(&expected_home), None, None, Some(&expected_venv), || {
            let layout = InstallLayout::new(HostOs::Linux).unwrap();
            assert_eq!(layout.home(), expected_home.as_path());
            assert_eq!(layout.venvs_dir(), expected_venv);
            assert_eq!(layout.venv_dir("msgraph"), expected_venv.join("msgraph"));
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

        with_layout_env(Some(&expected_home), None, None, None, || {
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

        with_layout_env(Some(&expected_home), None, None, None, || {
            let layout = InstallLayout::new(HostOs::Linux).unwrap();
            assert_eq!(layout.installed_versions().unwrap(), vec![new_version, legacy_version]);
        });
    }

    #[test]
    fn from_root_ignores_multi_pwsh_env_overrides() {
        let temp_dir = TempDir::new().unwrap();
        let explicit_home = temp_dir.path().join("package-root");
        let ignored_home = temp_dir.path().join("ignored-home");
        let overridden_bin = temp_dir.path().join("override-bin");
        let overridden_cache = temp_dir.path().join("override-cache");
        let overridden_venv = temp_dir.path().join("override-venv");

        with_layout_env(
            Some(&ignored_home),
            Some(&overridden_bin),
            Some(&overridden_cache),
            Some(&overridden_venv),
            || {
                let layout = InstallLayout::from_root(HostOs::Windows, explicit_home.clone()).unwrap();
                assert_eq!(layout.home(), explicit_home.as_path());
                assert_eq!(layout.bin_dir(), explicit_home.join("bin"));
                assert_eq!(layout.cache_dir(), explicit_home.join("cache"));
                assert_eq!(layout.venvs_dir(), explicit_home.join("venv"));
                assert_eq!(layout.versions_dir(), explicit_home.join("multi"));
            },
        );
    }

    #[test]
    fn from_root_with_versions_dir_supports_direct_version_roots() {
        let temp_dir = TempDir::new().unwrap();
        let explicit_home = temp_dir.path().join("package-root");

        let layout =
            InstallLayout::from_root_with_versions_dir(HostOs::Windows, explicit_home.clone(), explicit_home.clone())
                .unwrap();

        assert_eq!(layout.home(), explicit_home.as_path());
        assert_eq!(layout.bin_dir(), explicit_home.join("bin"));
        assert_eq!(layout.cache_dir(), explicit_home.join("cache"));
        assert_eq!(layout.venvs_dir(), explicit_home.join("venv"));
        assert_eq!(layout.versions_dir(), explicit_home);
    }

    #[test]
    fn from_parts_supports_custom_bin_and_versions_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let home = temp_dir.path().join("state-root");
        let bin_dir = temp_dir.path().join("shared-bin");
        let cache_dir = temp_dir.path().join("cache-root");
        let venvs_dir = temp_dir.path().join("venv-root");
        let versions_dir = temp_dir.path().join("payload-root");

        let layout = InstallLayout::from_parts(
            HostOs::Linux,
            home.clone(),
            bin_dir.clone(),
            cache_dir.clone(),
            venvs_dir.clone(),
            versions_dir.clone(),
        )
        .unwrap();

        assert_eq!(layout.home(), home.as_path());
        assert_eq!(layout.bin_dir(), bin_dir);
        assert_eq!(layout.cache_dir(), cache_dir);
        assert_eq!(layout.venvs_dir(), venvs_dir);
        assert_eq!(layout.versions_dir(), versions_dir);
    }
}
