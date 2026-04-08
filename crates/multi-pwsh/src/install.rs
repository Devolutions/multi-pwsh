use std::fs;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
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

    validate_archive_checksum(http, release, &archive_path)?;

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

fn download_text_with_retry(http: &Agent, url: &str, retries: usize) -> Result<String> {
    let mut last_error = None;

    for attempt in 1..=retries {
        let result = (|| -> Result<String> {
            let response = http.get(url).set("User-Agent", "multi-pwsh").call()?;
            Ok(response.into_string()?)
        })();

        match result {
            Ok(body) => return Ok(body),
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

fn validate_archive_checksum(http: &Agent, release: &ResolvedRelease, archive_path: &Path) -> Result<()> {
    let checksums = download_text_with_retry(http, &release.checksum_asset_url, 8)?;
    let expected = find_expected_checksum(&checksums, &release.asset_name)?;
    let actual = sha256_file(archive_path)?;

    if expected.eq_ignore_ascii_case(&actual) {
        return Ok(());
    }

    let _ = fs::remove_file(archive_path);
    Err(MultiPwshError::Archive(format!(
        "checksum mismatch for '{}' using '{}': expected {}, got {}",
        release.asset_name, release.checksum_asset_name, expected, actual
    )))
}

fn find_expected_checksum(checksums: &str, asset_name: &str) -> Result<String> {
    for (index, line) in checksums.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let checksum = trimmed
            .split_ascii_whitespace()
            .next()
            .ok_or_else(|| MultiPwshError::Archive(format!("malformed checksum line {}", index + 1)))?;

        if !is_valid_sha256_hex(checksum) {
            return Err(MultiPwshError::Archive(format!(
                "invalid sha256 checksum on line {}",
                index + 1
            )));
        }

        let file_name = trimmed[checksum.len()..].trim_start().trim_start_matches('*');
        if file_name.is_empty() {
            return Err(MultiPwshError::Archive(format!(
                "missing file name in checksum line {}",
                index + 1
            )));
        }

        if file_name == asset_name {
            return Ok(checksum.to_ascii_lowercase());
        }
    }

    Err(MultiPwshError::Archive(format!(
        "checksum entry for '{}' not found in checksum file",
        asset_name
    )))
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.as_bytes().iter().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{:02x}", byte));
    }
    Ok(output)
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

    #[test]
    fn find_expected_checksum_accepts_common_sha256_formats() {
        let checksum = find_expected_checksum(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  PowerShell-7.4.13-win-x64.zip\n\
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *other.zip",
            "PowerShell-7.4.13-win-x64.zip",
        )
        .unwrap();

        assert_eq!(
            checksum,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn find_expected_checksum_rejects_invalid_lines() {
        let error = find_expected_checksum(
            "not-a-hash  PowerShell-7.4.13-win-x64.zip",
            "PowerShell-7.4.13-win-x64.zip",
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid sha256 checksum"));
    }

    #[test]
    fn find_expected_checksum_requires_matching_asset() {
        let error = find_expected_checksum(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  other.zip",
            "PowerShell-7.4.13-win-x64.zip",
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("checksum entry for 'PowerShell-7.4.13-win-x64.zip' not found"));
    }

    #[test]
    fn sha256_file_hashes_file_contents() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("sample.txt");
        fs::write(&path, b"abc").unwrap();

        let digest = sha256_file(&path).unwrap();

        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
