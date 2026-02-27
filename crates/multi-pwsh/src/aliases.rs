use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{MultiPwshError, Result};
use crate::layout::InstallLayout;
use crate::platform::HostOs;
use crate::versions::MajorMinor;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AliasSelector {
    Major(u64),
    MajorMinor(MajorMinor),
    Exact(Version),
}

pub fn create_or_update_alias(
    layout: &InstallLayout,
    os: HostOs,
    line: MajorMinor,
    version: &Version,
    target: &Path,
) -> Result<PathBuf> {
    create_or_update_alias_with_selector(layout, os, AliasSelector::MajorMinor(line), version, target)
}

pub fn create_or_update_major_alias(
    layout: &InstallLayout,
    os: HostOs,
    major: u64,
    version: &Version,
    target: &Path,
) -> Result<PathBuf> {
    create_or_update_alias_with_selector(layout, os, AliasSelector::Major(major), version, target)
}

pub fn create_or_update_patch_alias(
    layout: &InstallLayout,
    os: HostOs,
    version: &Version,
    target: &Path,
) -> Result<PathBuf> {
    create_or_update_alias_with_selector(layout, os, AliasSelector::Exact(version.clone()), version, target)
}

fn create_or_update_alias_with_selector(
    layout: &InstallLayout,
    os: HostOs,
    selector: AliasSelector,
    version: &Version,
    target: &Path,
) -> Result<PathBuf> {
    fs::create_dir_all(layout.bin_dir())?;

    let alias_command = alias_command_name(&selector);
    let alias_file = alias_file_name(&selector, os);
    let alias_path = layout.bin_dir().join(alias_file);

    if os == HostOs::Windows {
        let legacy_exe_alias = layout.bin_dir().join(format!("{}.exe", alias_command));
        if legacy_exe_alias != alias_path && legacy_exe_alias.exists() {
            fs::remove_file(&legacy_exe_alias)?;
        }
    }

    if alias_path.exists() {
        fs::remove_file(&alias_path)?;
    }

    match os {
        HostOs::Windows => {
            create_windows_cmd_alias(target, &alias_path)?;
        }
        HostOs::Linux | HostOs::Macos => {
            create_symlink(target, &alias_path)?;
        }
    }

    let mut metadata = read_alias_metadata(layout)?;
    metadata.insert(alias_command, version.to_string());
    write_alias_metadata(layout, metadata)?;

    Ok(alias_path)
}

pub fn remove_alias(layout: &InstallLayout, os: HostOs, alias_command: &str) -> Result<bool> {
    let alias_path = layout.bin_dir().join(alias_file_name_from_command(alias_command, os));
    let mut removed = false;

    if alias_path.exists() {
        fs::remove_file(&alias_path)?;
        removed = true;
    }

    let mut document = read_alias_document(layout)?;
    if document.aliases.remove(alias_command).is_some() {
        write_alias_document(layout, &document)?;
        removed = true;
    }

    Ok(removed)
}

pub fn parse_alias_command_selector(alias_command: &str) -> Option<AliasSelector> {
    let selector = alias_command.strip_prefix("pwsh-")?;

    if let Ok(version) = Version::parse(selector) {
        return Some(AliasSelector::Exact(version));
    }

    let parts: Vec<&str> = selector.split('.').collect();
    if parts.len() == 1 {
        let major = parts[0].parse::<u64>().ok()?;
        return Some(AliasSelector::Major(major));
    }

    if parts.len() == 2 {
        let major = parts[0].parse::<u64>().ok()?;
        let minor = parts[1].parse::<u64>().ok()?;
        return Some(AliasSelector::MajorMinor(MajorMinor { major, minor }));
    }

    None
}

pub fn read_alias_metadata(layout: &InstallLayout) -> Result<HashMap<String, String>> {
    Ok(read_alias_document(layout)?.aliases)
}

fn write_alias_metadata(layout: &InstallLayout, aliases: HashMap<String, String>) -> Result<()> {
    let mut document = read_alias_document(layout)?;
    document.aliases = aliases;
    write_alias_document(layout, &document)?;
    Ok(())
}

pub fn read_minor_pin(layout: &InstallLayout, line: MajorMinor) -> Result<Option<Version>> {
    let document = read_alias_document(layout)?;
    match document.pins.get(&line_pin_key(line)) {
        Some(value) => Ok(Some(Version::parse(value)?)),
        None => Ok(None),
    }
}

pub fn read_minor_pins(layout: &InstallLayout) -> Result<HashMap<String, String>> {
    Ok(read_alias_document(layout)?.pins)
}

pub fn set_minor_pin(layout: &InstallLayout, line: MajorMinor, version: Option<Version>) -> Result<()> {
    let mut document = read_alias_document(layout)?;
    let key = line_pin_key(line);

    match version {
        Some(version) => {
            document.pins.insert(key, version.to_string());
        }
        None => {
            document.pins.remove(&key);
        }
    }

    write_alias_document(layout, &document)
}

fn line_pin_key(line: MajorMinor) -> String {
    format!("{}.{}", line.major, line.minor)
}

fn read_alias_document(layout: &InstallLayout) -> Result<AliasMetadata> {
    let path = layout.aliases_file();
    if !path.exists() {
        return Ok(AliasMetadata::default());
    }

    let content = fs::read_to_string(path)?;
    let metadata: AliasMetadata = serde_json::from_str(&content)?;
    Ok(metadata)
}

fn write_alias_document(layout: &InstallLayout, metadata: &AliasMetadata) -> Result<()> {
    let path = layout.aliases_file();
    let payload = serde_json::to_string_pretty(metadata)?;
    fs::write(path, payload)?;
    Ok(())
}

fn alias_file_name(selector: &AliasSelector, os: HostOs) -> String {
    alias_file_name_from_command(&alias_command_name(selector), os)
}

fn alias_file_name_from_command(alias_command: &str, os: HostOs) -> String {
    match os {
        HostOs::Windows => format!("{}.cmd", alias_command),
        HostOs::Linux | HostOs::Macos => alias_command.to_string(),
    }
}

fn alias_command_name(selector: &AliasSelector) -> String {
    match selector {
        AliasSelector::Major(major) => format!("pwsh-{}", major),
        AliasSelector::MajorMinor(line) => format!("pwsh-{}.{}", line.major, line.minor),
        AliasSelector::Exact(version) => format!("pwsh-{}", version),
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link_path: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    symlink(target, link_path)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_symlink(_target: &Path, _link_path: &Path) -> Result<()> {
    Err(MultiPwshError::AliasCreation(
        "symlink is not available on this platform".to_string(),
    ))
}

#[cfg(windows)]
fn create_windows_cmd_alias(target: &Path, alias_path: &Path) -> Result<()> {
    let target_string = target
        .to_str()
        .ok_or_else(|| MultiPwshError::AliasCreation("target path is not valid UTF-8".to_string()))?;

    let script = format!("@echo off\r\n\"{}\" %*\r\nexit /b %ERRORLEVEL%\r\n", target_string);

    fs::write(alias_path, script).map_err(|error| {
        MultiPwshError::AliasCreation(format!(
            "failed to write windows command alias '{}' -> '{}': {}",
            alias_path.display(),
            target.display(),
            error
        ))
    })?;

    Ok(())
}

#[cfg(not(windows))]
fn create_windows_cmd_alias(_target: &Path, _alias_path: &Path) -> Result<()> {
    Err(MultiPwshError::AliasCreation(
        "windows command alias is not available on this platform".to_string(),
    ))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AliasMetadata {
    #[serde(default)]
    aliases: HashMap<String, String>,
    #[serde(default)]
    pins: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_name_uses_major_minor() {
        let line = MajorMinor { major: 7, minor: 4 };
        assert_eq!(alias_command_name(&AliasSelector::MajorMinor(line)), "pwsh-7.4");
        assert_eq!(
            alias_file_name(&AliasSelector::MajorMinor(line), HostOs::Linux),
            "pwsh-7.4"
        );
        assert_eq!(
            alias_file_name(&AliasSelector::MajorMinor(line), HostOs::Windows),
            "pwsh-7.4.cmd"
        );
    }

    #[test]
    fn alias_name_supports_major() {
        assert_eq!(alias_command_name(&AliasSelector::Major(7)), "pwsh-7");
        assert_eq!(alias_file_name(&AliasSelector::Major(7), HostOs::Linux), "pwsh-7");
        assert_eq!(alias_file_name(&AliasSelector::Major(7), HostOs::Windows), "pwsh-7.cmd");
    }

    #[test]
    fn parses_alias_major_minor_selector() {
        let selector = parse_alias_command_selector("pwsh-7.5").unwrap();
        assert_eq!(selector, AliasSelector::MajorMinor(MajorMinor { major: 7, minor: 5 }));
    }

    #[test]
    fn parses_alias_major_selector() {
        let selector = parse_alias_command_selector("pwsh-7").unwrap();
        assert_eq!(selector, AliasSelector::Major(7));
    }

    #[test]
    fn parses_alias_exact_selector() {
        let selector = parse_alias_command_selector("pwsh-7.4.11").unwrap();
        assert_eq!(selector, AliasSelector::Exact(Version::parse("7.4.11").unwrap()));
    }

    #[test]
    fn rejects_invalid_alias_selector() {
        assert!(parse_alias_command_selector("pwsh").is_none());
        assert!(parse_alias_command_selector("not-pwsh-7.5").is_none());
    }
}
