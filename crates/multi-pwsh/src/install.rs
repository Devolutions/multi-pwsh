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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChecksumSource {
    ReleaseAsset,
    Url(String),
    File(PathBuf),
    Skip,
}

pub fn ensure_installed(
    layout: &InstallLayout,
    http: &Agent,
    os: HostOs,
    release: &ResolvedRelease,
    checksum_source: &ChecksumSource,
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

    validate_archive_checksum(http, release, checksum_source, &archive_path)?;

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
            let mut reader = response.into_reader();
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            decode_checksum_text(&bytes)
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

fn decode_checksum_text(bytes: &[u8]) -> Result<String> {
    match detect_text_encoding(bytes) {
        TextEncoding::Utf8 => String::from_utf8(bytes.to_vec()).map_err(invalid_checksum_encoding),
        TextEncoding::Utf8Bom => String::from_utf8(bytes[3..].to_vec()).map_err(invalid_checksum_encoding),
        TextEncoding::Utf16Le => decode_utf16_text(&bytes[2..], true),
        TextEncoding::Utf16Be => decode_utf16_text(&bytes[2..], false),
        TextEncoding::Utf16LeNoBom => decode_utf16_text(bytes, true),
        TextEncoding::Utf16BeNoBom => decode_utf16_text(bytes, false),
    }
}

fn decode_utf16_text(bytes: &[u8], little_endian: bool) -> Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(MultiPwshError::Archive(
            "invalid checksum file encoding: odd-length utf-16 payload".to_string(),
        ));
    }

    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();

    String::from_utf16(&units)
        .map_err(|error| MultiPwshError::Archive(format!("invalid checksum file encoding: {}", error)))
}

fn invalid_checksum_encoding(error: std::string::FromUtf8Error) -> MultiPwshError {
    MultiPwshError::Archive(format!("invalid checksum file encoding: {}", error))
}

fn detect_text_encoding(bytes: &[u8]) -> TextEncoding {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return TextEncoding::Utf8Bom;
    }

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return TextEncoding::Utf16Le;
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        return TextEncoding::Utf16Be;
    }

    if looks_like_utf16_le(bytes) {
        return TextEncoding::Utf16LeNoBom;
    }

    if looks_like_utf16_be(bytes) {
        return TextEncoding::Utf16BeNoBom;
    }

    TextEncoding::Utf8
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0
}

fn looks_like_utf16_be(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0 && bytes[2] == 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Utf16LeNoBom,
    Utf16BeNoBom,
}

fn validate_archive_checksum(
    http: &Agent,
    release: &ResolvedRelease,
    checksum_source: &ChecksumSource,
    archive_path: &Path,
) -> Result<()> {
    if matches!(checksum_source, ChecksumSource::Skip) {
        return Ok(());
    }

    let (checksums, checksum_source_name) = load_checksum_text(http, release, checksum_source)?;
    let expected = find_expected_checksum(&checksums, &release.asset_name)?;
    let actual = sha256_file(archive_path)?;

    if expected.eq_ignore_ascii_case(&actual) {
        return Ok(());
    }

    let _ = fs::remove_file(archive_path);
    Err(MultiPwshError::Archive(format!(
        "checksum mismatch for '{}' using '{}': expected {}, got {}",
        release.asset_name, checksum_source_name, expected, actual
    )))
}

fn load_checksum_text(
    http: &Agent,
    release: &ResolvedRelease,
    checksum_source: &ChecksumSource,
) -> Result<(String, String)> {
    match checksum_source {
        ChecksumSource::ReleaseAsset => {
            let checksum_url = release.checksum_asset_url.as_deref().ok_or_else(|| {
                MultiPwshError::Archive(format!(
                    "release '{}' is missing checksum asset metadata; provide --hash-file <url-or-path> or use --skip-hash-verification to bypass verification",
                    release.asset_name
                ))
            })?;
            let checksum_name = release
                .checksum_asset_name
                .clone()
                .unwrap_or_else(|| "hashes.sha256".to_string());
            Ok((download_text_with_retry(http, checksum_url, 8)?, checksum_name))
        }
        ChecksumSource::Url(url) => Ok((download_text_with_retry(http, url, 8)?, url.clone())),
        ChecksumSource::File(path) => {
            let bytes = fs::read(path)?;
            Ok((decode_checksum_text(&bytes)?, path.display().to_string()))
        }
        ChecksumSource::Skip => Ok((String::new(), "checksum verification disabled".to_string())),
    }
}

fn find_expected_checksum(checksums: &str, asset_name: &str) -> Result<String> {
    for (index, line) in checksums.lines().enumerate() {
        if let Some((checksum, file_name)) = parse_checksum_line(line, index + 1, asset_name)? {
            if file_name == asset_name {
                return Ok(checksum);
            }
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

fn parse_checksum_line<'a>(
    line: &'a str,
    line_number: usize,
    target_asset_name: &str,
) -> Result<Option<(String, &'a str)>> {
    let trimmed = line.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }

    if let Some(parsed) = parse_bsd_checksum_line(trimmed, line_number)? {
        return Ok(Some(parsed));
    }

    parse_gnu_checksum_line(trimmed, line_number, target_asset_name)
}

fn parse_bsd_checksum_line<'a>(line: &'a str, line_number: usize) -> Result<Option<(String, &'a str)>> {
    let Some((left, right)) = line.split_once('=') else {
        return Ok(None);
    };

    let Some(file_name) = left
        .trim()
        .strip_prefix("SHA256 (")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Ok(None);
    };

    let checksum = right.trim();
    if !is_valid_sha256_hex(checksum) {
        return Err(MultiPwshError::Archive(format!(
            "invalid sha256 checksum on line {}",
            line_number
        )));
    }

    Ok(Some((checksum.to_ascii_lowercase(), file_name)))
}

fn parse_gnu_checksum_line<'a>(
    line: &'a str,
    line_number: usize,
    target_asset_name: &str,
) -> Result<Option<(String, &'a str)>> {
    let mut parts = line.split_ascii_whitespace();
    let Some(checksum) = parts.next() else {
        return Ok(None);
    };

    let remainder = line[checksum.len()..].trim_start();
    if remainder.is_empty() {
        return Ok(None);
    }

    let file_name = remainder.trim_start_matches('*').trim();
    if file_name.is_empty() {
        return Ok(None);
    }

    if !is_valid_sha256_hex(checksum) {
        if file_name == target_asset_name {
            return Err(MultiPwshError::Archive(format!(
                "invalid sha256 checksum on line {}",
                line_number
            )));
        }

        return Ok(None);
    }

    Ok(Some((checksum.to_ascii_lowercase(), file_name)))
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
    fn load_checksum_text_reads_local_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let checksum_path = temp_dir.path().join("hashes.sha256");
        fs::write(
            &checksum_path,
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  PowerShell-7.4.13-win-x64.zip\n",
        )
        .unwrap();

        let http = ureq::AgentBuilder::new().build();
        let release = ResolvedRelease {
            version: semver::Version::parse("7.4.13").unwrap(),
            asset_name: "PowerShell-7.4.13-win-x64.zip".to_string(),
            asset_url: "https://example.invalid/PowerShell-7.4.13-win-x64.zip".to_string(),
            checksum_asset_name: None,
            checksum_asset_url: None,
        };

        let (content, source_name) =
            load_checksum_text(&http, &release, &ChecksumSource::File(checksum_path.clone())).unwrap();

        assert!(content.contains("PowerShell-7.4.13-win-x64.zip"));
        assert_eq!(source_name, checksum_path.display().to_string());
    }

    #[test]
    fn load_checksum_text_requires_release_asset_for_default_source() {
        let http = ureq::AgentBuilder::new().build();
        let release = ResolvedRelease {
            version: semver::Version::parse("7.4.13").unwrap(),
            asset_name: "PowerShell-7.4.13-win-x64.zip".to_string(),
            asset_url: "https://example.invalid/PowerShell-7.4.13-win-x64.zip".to_string(),
            checksum_asset_name: None,
            checksum_asset_url: None,
        };

        let error = load_checksum_text(&http, &release, &ChecksumSource::ReleaseAsset).unwrap_err();

        assert!(error.to_string().contains("missing checksum asset metadata"));
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
    fn find_expected_checksum_accepts_bsd_format() {
        let checksum = find_expected_checksum(
            "SHA256 (PowerShell-7.4.13-win-x64.zip) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "PowerShell-7.4.13-win-x64.zip",
        )
        .unwrap();

        assert_eq!(
            checksum,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn find_expected_checksum_ignores_unrelated_non_checksum_lines() {
        let checksum = find_expected_checksum(
            "Checksums for release assets\n\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  PowerShell-7.4.13-win-x64.zip",
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
    fn decode_checksum_text_accepts_utf16le_bom() {
        let content =
            "73601859461b130ee1e6624f0683000a794cbe86db0f4ff9f2ce2a7d4f5f6a01 *powershell-7.4.13-1.cm.aarch64.rpm\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in content.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        let decoded = decode_checksum_text(&bytes).unwrap();

        assert_eq!(decoded, content);
    }

    #[test]
    fn find_expected_checksum_accepts_real_powershell_utf16le_line() {
        let content =
            "0aa943342ddd5ff5cd5bbb964e6594b7af3e10758ff59874cd26420bebb3c755 *PowerShell-7.4.13-win-arm64.exe\n\
1820febe6f9567c8bab21be601dacb902777c1185e1beb81843c3a6f902d6b9d *PowerShell-7.4.13-win-arm64.zip\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in content.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        let decoded = decode_checksum_text(&bytes).unwrap();
        let checksum = find_expected_checksum(&decoded, "PowerShell-7.4.13-win-arm64.zip").unwrap();

        assert_eq!(
            checksum,
            "1820febe6f9567c8bab21be601dacb902777c1185e1beb81843c3a6f902d6b9d"
        );
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
