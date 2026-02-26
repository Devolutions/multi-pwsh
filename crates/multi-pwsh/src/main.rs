mod aliases;
mod error;
mod install;
mod layout;
mod platform;
mod release;
mod versions;

use std::env;
use std::fs;
use std::process;

use semver::Version;

use aliases::{create_or_update_alias, parse_alias_command_line, remove_alias};
use error::{MultiPwshError, Result};
use install::ensure_installed;
use layout::InstallLayout;
use platform::{HostArch, HostOs};
use release::ReleaseClient;
use versions::{parse_exact_version, parse_install_selector, parse_major_minor_selector, MajorMinor};

fn print_usage() {
    eprintln!(
        "Usage:\n  multi-pwsh install <version|major.minor> [--arch <auto|x64|x86|arm64|arm32>]\n  multi-pwsh update <major.minor> [--arch <auto|x64|x86|arm64|arm32>]\n  multi-pwsh uninstall <version> [--force]\n  multi-pwsh list\n  multi-pwsh doctor --repair-aliases"
    );
}

fn parse_arch_option(args: &[String]) -> Result<Option<HostArch>> {
    if args.is_empty() {
        return Ok(None);
    }

    if args.len() != 2 || (args[0] != "--arch" && args[0] != "-a") {
        return Err(MultiPwshError::InvalidArguments(
            "expected optional --arch <value>".to_string(),
        ));
    }

    if args[1] == "auto" {
        return Ok(None);
    }

    HostArch::parse(&args[1]).map(Some).ok_or_else(|| {
        MultiPwshError::InvalidArguments(format!(
            "unsupported architecture '{}', expected one of: auto, x64, x86, arm64, arm32",
            args[1]
        ))
    })
}

fn run_install(selector_input: &str, arch: Option<HostArch>) -> Result<()> {
    let selector = parse_install_selector(selector_input)?;
    let os = HostOs::detect()?;
    let arch = arch.unwrap_or_else(HostArch::detect);

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let token = env::var("GITHUB_TOKEN").ok();
    let release_client = ReleaseClient::new(token)?;
    let release = release_client.resolve_selector(selector, os, arch)?;
    let executable_path = ensure_installed(&layout, release_client.http_client(), os, &release)?;

    let line = release.version_line();
    let alias_path = create_or_update_alias(&layout, os, line, &release.version, &executable_path)?;

    println!("Installed PowerShell {}", release.version);
    println!("Version path: {}", layout.version_dir(&release.version).display());
    println!("Updated alias: {}", alias_path.display());
    println!("Add to PATH once: {}", layout.bin_dir().display());

    Ok(())
}

fn run_update(line_input: &str, arch: Option<HostArch>) -> Result<()> {
    let line = parse_major_minor_selector(line_input)?;
    let os = HostOs::detect()?;
    let arch = arch.unwrap_or_else(HostArch::detect);

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let token = env::var("GITHUB_TOKEN").ok();
    let release_client = ReleaseClient::new(token)?;
    let release = release_client.resolve_latest_in_line(line, os, arch)?;
    let executable_path = ensure_installed(&layout, release_client.http_client(), os, &release)?;

    let alias_path = create_or_update_alias(&layout, os, line, &release.version, &executable_path)?;

    println!("Updated line {} to {}", line, release.version);
    println!("Version path: {}", layout.version_dir(&release.version).display());
    println!("Updated alias: {}", alias_path.display());
    println!("Add to PATH once: {}", layout.bin_dir().display());

    Ok(())
}

fn parse_force_option(args: &[String]) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }

    if args.len() == 1 && args[0] == "--force" {
        return Ok(true);
    }

    Err(MultiPwshError::InvalidArguments(
        "expected optional --force".to_string(),
    ))
}

fn run_uninstall(version_input: &str, force: bool) -> Result<()> {
    let version = parse_exact_version(version_input)?;
    let os = HostOs::detect()?;

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let version_dir = layout.version_dir(&version);
    if version_dir.exists() {
        fs::remove_dir_all(&version_dir)?;
        println!("Removed PowerShell {}", version);
    } else if force {
        println!(
            "PowerShell {} is not installed; continuing because --force was provided",
            version
        );
    } else {
        return Err(MultiPwshError::InvalidArguments(format!(
            "version {} is not installed (use --force to ignore)",
            version
        )));
    }

    let aliases = aliases::read_alias_metadata(&layout)?;
    let removed_version_text = version.to_string();
    let mut affected_aliases: Vec<String> = aliases
        .into_iter()
        .filter_map(|(alias_name, alias_version)| {
            if alias_version == removed_version_text {
                Some(alias_name)
            } else {
                None
            }
        })
        .collect();

    if affected_aliases.is_empty() {
        println!("No aliases referenced version {}", version);
        return Ok(());
    }

    affected_aliases.sort();
    let installed_versions = layout.installed_versions()?;

    let mut updated_aliases = 0usize;
    let mut removed_aliases = 0usize;

    for alias_name in affected_aliases {
        let fallback_version = parse_alias_command_line(&alias_name).and_then(|line| {
            installed_versions
                .iter()
                .find(|candidate| MajorMinor::from_version(candidate) == line)
                .cloned()
        });

        if let Some(fallback_version) = fallback_version {
            let line = MajorMinor::from_version(&fallback_version);
            let target = layout.version_executable(&fallback_version);
            let alias_path = create_or_update_alias(&layout, os, line, &fallback_version, &target)?;
            println!("Updated alias: {} -> {}", alias_name, fallback_version);
            println!("Alias path: {}", alias_path.display());
            updated_aliases += 1;
            continue;
        }

        if remove_alias(&layout, os, &alias_name)? {
            println!("Removed alias: {}", alias_name);
        }
        removed_aliases += 1;
    }

    println!(
        "Alias cleanup complete: {} updated, {} removed",
        updated_aliases, removed_aliases
    );

    Ok(())
}

fn run_list() -> Result<()> {
    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;
    let versions = layout.installed_versions()?;
    let aliases = aliases::read_alias_metadata(&layout)?;

    println!("Install root: {}", layout.root().display());
    println!("Alias bin: {}", layout.bin_dir().display());
    println!();

    if versions.is_empty() {
        println!("Installed versions: (none)");
    } else {
        println!("Installed versions:");
        for version in versions {
            println!("  - {}", version);
        }
    }

    println!();
    if aliases.is_empty() {
        println!("Aliases: (none)");
    } else {
        println!("Aliases:");
        let mut items: Vec<_> = aliases.into_iter().collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        for (alias, version) in items {
            println!("  - {} -> {}", alias, version);
        }
    }

    Ok(())
}

fn run_doctor(args: &[String]) -> Result<()> {
    if args.len() != 1 || args[0] != "--repair-aliases" {
        return Err(MultiPwshError::InvalidArguments(
            "doctor currently supports only: --repair-aliases".to_string(),
        ));
    }

    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let aliases = aliases::read_alias_metadata(&layout)?;
    if aliases.is_empty() {
        println!("No aliases found in metadata.");
        return Ok(());
    }

    let mut repaired = 0usize;
    let mut skipped = 0usize;

    let mut items: Vec<_> = aliases.into_iter().collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    for (alias_name, version_text) in items {
        let version = match Version::parse(&version_text) {
            Ok(version) => version,
            Err(_) => {
                eprintln!("Skipping alias {}: invalid version '{}'", alias_name, version_text);
                skipped += 1;
                continue;
            }
        };

        let target = layout.version_executable(&version);
        if !target.exists() {
            eprintln!(
                "Skipping alias {}: target executable not found at {}",
                alias_name,
                target.display()
            );
            skipped += 1;
            continue;
        }

        let line = MajorMinor::from_version(&version);
        let alias_path = create_or_update_alias(&layout, os, line, &version, &target)?;
        println!("Repaired alias: {}", alias_path.display());
        repaired += 1;
    }

    println!("Repair complete: {} repaired, {} skipped", repaired, skipped);
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Err(MultiPwshError::InvalidArguments("missing command".to_string()));
    }

    match args[0].as_str() {
        "install" => {
            if args.len() < 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "install requires <version|major.minor>".to_string(),
                ));
            }
            let arch = parse_arch_option(&args[2..])?;
            run_install(&args[1], arch)
        }
        "update" => {
            if args.len() < 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "update requires <major.minor>".to_string(),
                ));
            }
            let arch = parse_arch_option(&args[2..])?;
            run_update(&args[1], arch)
        }
        "uninstall" => {
            if args.len() < 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "uninstall requires <version>".to_string(),
                ));
            }
            let force = parse_force_option(&args[2..])?;
            run_uninstall(&args[1], force)
        }
        "list" => {
            if args.len() != 1 {
                return Err(MultiPwshError::InvalidArguments(
                    "list does not accept additional arguments".to_string(),
                ));
            }
            run_list()
        }
        "doctor" => run_doctor(&args[1..]),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        command => Err(MultiPwshError::InvalidArguments(format!(
            "unknown command '{}'. expected: install, update, uninstall, list, doctor",
            command
        ))),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {}", error);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_force_option_defaults_to_false() {
        let args: Vec<String> = Vec::new();
        assert!(!parse_force_option(&args).unwrap());
    }

    #[test]
    fn parse_force_option_accepts_force_flag() {
        let args = vec!["--force".to_string()];
        assert!(parse_force_option(&args).unwrap());
    }

    #[test]
    fn parse_force_option_rejects_unexpected_args() {
        let args = vec!["--arch".to_string(), "x64".to_string()];
        assert!(parse_force_option(&args).is_err());
    }
}
