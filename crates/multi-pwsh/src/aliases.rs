use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{MultiPwshError, Result};
use crate::layout::InstallLayout;
use crate::platform::HostOs;
use crate::versions::MajorMinor;

pub fn create_or_update_alias(
    layout: &InstallLayout,
    os: HostOs,
    line: MajorMinor,
    version: &Version,
    target: &Path,
) -> Result<PathBuf> {
    fs::create_dir_all(layout.bin_dir())?;

    let alias_command = alias_command_name(line);
    let alias_file = alias_file_name(line, os);
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

    let mut metadata = read_alias_metadata(layout)?;
    if metadata.remove(alias_command).is_some() {
        write_alias_metadata(layout, metadata)?;
        removed = true;
    }

    Ok(removed)
}

pub fn parse_alias_command_line(alias_command: &str) -> Option<MajorMinor> {
    let line = alias_command.strip_prefix("pwsh-")?;
    let parts: Vec<&str> = line.split('.').collect();
    if parts.len() != 2 {
        return None;
    }

    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    Some(MajorMinor { major, minor })
}

pub fn read_alias_metadata(layout: &InstallLayout) -> Result<HashMap<String, String>> {
    let path = layout.aliases_file();
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(path)?;
    let metadata: AliasMetadata = serde_json::from_str(&content)?;
    Ok(metadata.aliases)
}

fn write_alias_metadata(layout: &InstallLayout, aliases: HashMap<String, String>) -> Result<()> {
    let path = layout.aliases_file();
    let metadata = AliasMetadata { aliases };
    let payload = serde_json::to_string_pretty(&metadata)?;
    fs::write(path, payload)?;
    Ok(())
}

fn alias_file_name(line: MajorMinor, os: HostOs) -> String {
    alias_file_name_from_command(&alias_command_name(line), os)
}

fn alias_file_name_from_command(alias_command: &str, os: HostOs) -> String {
    match os {
        HostOs::Windows => format!("{}.cmd", alias_command),
        HostOs::Linux | HostOs::Macos => alias_command.to_string(),
    }
}

fn alias_command_name(line: MajorMinor) -> String {
    format!("pwsh-{}.{}", line.major, line.minor)
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

#[derive(Debug, Serialize, Deserialize)]
struct AliasMetadata {
    aliases: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_name_uses_major_minor() {
        let line = MajorMinor { major: 7, minor: 4 };
        assert_eq!(alias_command_name(line), "pwsh-7.4");
        assert_eq!(alias_file_name(line, HostOs::Linux), "pwsh-7.4");
        assert_eq!(alias_file_name(line, HostOs::Windows), "pwsh-7.4.cmd");
    }

    #[test]
    fn parses_alias_line() {
        let line = parse_alias_command_line("pwsh-7.5").unwrap();
        assert_eq!(line.major, 7);
        assert_eq!(line.minor, 5);
    }

    #[test]
    fn rejects_invalid_alias_line() {
        assert!(parse_alias_command_line("pwsh").is_none());
        assert!(parse_alias_command_line("pwsh-7").is_none());
        assert!(parse_alias_command_line("pwsh-7.5.1").is_none());
        assert!(parse_alias_command_line("not-pwsh-7.5").is_none());
    }
}
