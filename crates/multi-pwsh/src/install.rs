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

    let install_dir = layout.version_dir(&release.version);
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir)?;
    }
    fs::create_dir_all(&install_dir)?;

    let temp_dir = tempfile::tempdir()?;
    let archive_path = temp_dir.path().join(&release.asset_name);

    download_with_retry(http, &release.asset_url, &archive_path, 8)?;
    extract_archive(&archive_path, &install_dir)?;

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
