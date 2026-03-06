use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use flate2::read::GzDecoder;
use ureq::Agent;

use crate::error::{MultiPwshError, Result};
use crate::layout::InstallLayout;
use crate::platform::HostOs;
use crate::release::ResolvedRelease;

pub fn ensure_installed(
    layout: &InstallLayout,
    http: &Agent,
    os: HostOs,
    release: &ResolvedRelease,
) -> Result<PathBuf> {
    let executable = layout.version_executable(&release.version);
    if executable.exists() {
        return Ok(executable);
    }

    let install_dir = layout.version_install_dir(&release.version);
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)?;
    }
    fs::create_dir_all(&install_dir)?;

    let cache_dir = layout.cache_dir();
    fs::create_dir_all(&cache_dir)?;
    let archive_path = cache_dir.join(&release.asset_name);

    if !archive_path.exists() {
        download_with_retry(http, &release.asset_url, &archive_path, 8)?;
    }

    extract_archive(&archive_path, &install_dir)?;

    if !cache_keep_archives() {
        let _ = fs::remove_file(&archive_path);
    }

    let executable = layout.version_executable(&release.version);
    if !executable.exists() {
        return Err(MultiPwshError::Archive(format!(
            "installation completed but executable '{}' was not found",
            executable.display()
        )));
    }

    if os != HostOs::Windows {
        ensure_executable_bit(&executable)?;
    }

    Ok(executable)
}

fn download_with_retry(http: &Agent, url: &str, destination: &Path, retries: usize) -> Result<()> {
    let mut last_error = None;

    for attempt in 1..=retries {
        let result = (|| -> Result<()> {
            let response = http.get(url).set("User-Agent", "multi-pwsh").call()?;
            let mut response_reader = response.into_reader();
            let mut file = File::create(destination)?;
            io::copy(&mut response_reader, &mut file)?;
            file.flush()?;
            Ok(())
        })();

        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < retries {
                    let delay_seconds = 2u64.pow(attempt as u32);
                    thread::sleep(Duration::from_secs(delay_seconds.min(30)));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| MultiPwshError::Archive("download failed without detailed error".to_string())))
}

fn cache_keep_archives() -> bool {
    match std::env::var("MULTI_PWSH_CACHE_KEEP") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn extract_archive(archive_path: &Path, install_dir: &Path) -> Result<()> {
    let name = archive_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    if name.ends_with(".zip") {
        extract_zip(archive_path, install_dir)?;
        return Ok(());
    }

    if name.ends_with(".tar.gz") {
        extract_tar_gz(archive_path, install_dir)?;
        return Ok(());
    }

    Err(MultiPwshError::Archive(format!(
        "unsupported archive format '{}'",
        name
    )))
}

fn extract_zip(archive_path: &Path, install_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut zipped_file = archive.by_index(index)?;
        let relative_path = match zipped_file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => continue,
        };
        let out_path = install_dir.join(relative_path);

        if zipped_file.name().ends_with('/') {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = File::create(&out_path)?;
        io::copy(&mut zipped_file, &mut output)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = zipped_file.unix_mode() {
                fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

fn extract_tar_gz(archive_path: &Path, install_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let decompressed = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decompressed);
    archive.unpack(install_dir)?;
    Ok(())
}

#[cfg(unix)]
fn ensure_executable_bit(executable: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(executable)?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();
    if mode & 0o111 == 0 {
        permissions.set_mode(mode | 0o755);
        fs::set_permissions(executable, permissions)?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn ensure_executable_bit(_executable: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_cache_keep<T>(value: Option<&str>, action: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os("MULTI_PWSH_CACHE_KEEP");

        match value {
            Some(value) => unsafe { std::env::set_var("MULTI_PWSH_CACHE_KEEP", value) },
            None => unsafe { std::env::remove_var("MULTI_PWSH_CACHE_KEEP") },
        }

        let result = action();

        match previous {
            Some(value) => unsafe { std::env::set_var("MULTI_PWSH_CACHE_KEEP", value) },
            None => unsafe { std::env::remove_var("MULTI_PWSH_CACHE_KEEP") },
        }

        result
    }

    #[test]
    fn cache_keep_defaults_to_false() {
        with_cache_keep(None, || {
            assert!(!cache_keep_archives());
        });
    }

    #[test]
    fn cache_keep_parses_truthy_values() {
        with_cache_keep(Some("true"), || {
            assert!(cache_keep_archives());
        });

        with_cache_keep(Some("1"), || {
            assert!(cache_keep_archives());
        });

        with_cache_keep(Some("on"), || {
            assert!(cache_keep_archives());
        });
    }

    #[test]
    fn cache_keep_parses_falsey_values() {
        with_cache_keep(Some("false"), || {
            assert!(!cache_keep_archives());
        });
    }
}
