mod aliases;
mod error;
mod install;
mod layout;
mod platform;
mod release;
mod versions;

use std::env;
use std::process;

use semver::Version;

use aliases::create_or_update_alias;
use error::{MultiPwshError, Result};
use install::ensure_installed;
use layout::InstallLayout;
use platform::{HostArch, HostOs};
use release::ReleaseClient;
use versions::{parse_install_selector, parse_major_minor_selector, MajorMinor};

fn print_usage() {
    eprintln!(
        "Usage:\n  multi-pwsh install <version|major.minor> [--arch <auto|x64|x86|arm64|arm32>]\n  multi-pwsh update <major.minor> [--arch <auto|x64|x86|arm64|arm32>]\n  multi-pwsh list\n  multi-pwsh doctor --repair-aliases"
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
            "unknown command '{}'. expected: install, update, list, doctor",
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
