mod aliases;
mod error;
mod install;
mod layout;
mod platform;
mod release;
mod versions;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process;

use semver::Version;

use aliases::{
    create_or_update_alias, create_or_update_major_alias, create_or_update_patch_alias, parse_alias_command_selector,
    read_minor_pin, read_minor_pins, remove_alias, set_minor_pin, AliasSelector,
};
use error::{MultiPwshError, Result};
use install::ensure_installed;
use layout::InstallLayout;
use platform::{HostArch, HostOs};
use release::ReleaseClient;
use versions::{
    parse_exact_version, parse_install_selector, parse_major_minor_selector, parse_major_selector, MajorMinor,
    VersionSelector,
};

const POWERSHELL_UPDATECHECK_ENV_VAR: &str = "POWERSHELL_UPDATECHECK";
const POWERSHELL_UPDATECHECK_OFF: &str = "Off";
const PSMODULEPATH_ENV_VAR: &str = "PSModulePath";
const VIRTUAL_ENVIRONMENT_FLAG: &str = "-virtualenvironment";
const VIRTUAL_ENVIRONMENT_SHORT_FLAG: &str = "-venv";

fn print_usage() {
    eprintln!(
        "Usage:\n  multi-pwsh install <version|major|major.minor|major.minor.x> [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease]\n  multi-pwsh update <major.minor> [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease]\n  multi-pwsh uninstall <version> [--force]\n  multi-pwsh list [--available] [--include-prerelease]\n  multi-pwsh venv create <name>\n  multi-pwsh venv delete <name>\n  multi-pwsh venv export <name> <archive.zip>\n  multi-pwsh venv import <name> <archive.zip>\n  multi-pwsh venv list\n  multi-pwsh alias set <major.minor> <version|latest>\n  multi-pwsh alias unset <major.minor>\n  multi-pwsh host <version|major|major.minor|pwsh-alias> [-VirtualEnvironment <name>|-venv <name>] [pwsh arguments...]\n  multi-pwsh doctor --repair-aliases"
    );
}

struct ReleaseSelectionOptions {
    arch: Option<HostArch>,
    include_prerelease: bool,
}

enum ListOption {
    Installed,
    Available { include_prerelease: bool },
}

#[derive(Debug, Default, Eq, PartialEq)]
struct HostLaunchOptions {
    pwsh_args: Vec<OsString>,
    virtual_environment: Option<String>,
}

struct ProcessEnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl ProcessEnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let previous = env::var_os(key);
        unsafe { env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for ProcessEnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { env::set_var(self.key, value) },
            None => unsafe { env::remove_var(self.key) },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostSelector {
    Major(u64),
    MajorMinor(MajorMinor),
    Exact(Version),
}

fn parse_host_selector(value: &str) -> Result<HostSelector> {
    if let Some(selector) = parse_alias_command_selector(value) {
        return Ok(match selector {
            AliasSelector::Major(major) => HostSelector::Major(major),
            AliasSelector::MajorMinor(line) => HostSelector::MajorMinor(line),
            AliasSelector::Exact(version) => HostSelector::Exact(version),
        });
    }

    if let Ok(version) = parse_exact_version(value) {
        return Ok(HostSelector::Exact(version));
    }

    if let Ok(line) = parse_major_minor_selector(value) {
        return Ok(HostSelector::MajorMinor(line));
    }

    if let Ok(major) = parse_major_selector(value) {
        return Ok(HostSelector::Major(major));
    }

    Err(MultiPwshError::InvalidArguments(format!(
        "host selector '{}' is invalid; expected one of: <major>, <major.minor>, <major.minor.patch>, or pwsh-<selector>",
        value
    )))
}

fn resolve_host_version(layout: &InstallLayout, selector: &HostSelector) -> Result<Version> {
    match selector {
        HostSelector::Exact(version) => Ok(version.clone()),
        HostSelector::Major(major) => latest_installed_in_major(layout, *major)?.ok_or_else(|| {
            MultiPwshError::InvalidArguments(format!(
                "no installed PowerShell version found for major {}; install one with: multi-pwsh install {}",
                major, major
            ))
        }),
        HostSelector::MajorMinor(line) => {
            let pinned = read_minor_pin(layout, *line)?;
            if let Some(version) = pinned {
                return Ok(version);
            }

            latest_installed_in_line(layout, *line)?.ok_or_else(|| {
                MultiPwshError::InvalidArguments(format!(
                    "no installed PowerShell version found for line {}; install one with: multi-pwsh install {}",
                    line, line
                ))
            })
        }
    }
}

fn resolve_host_executable(layout: &InstallLayout, selector_input: &str) -> Result<(Version, PathBuf)> {
    let selector = parse_host_selector(selector_input)?;
    let version = resolve_host_version(layout, &selector)?;
    let executable = layout.version_executable(&version);

    if !executable.exists() {
        return Err(MultiPwshError::InvalidArguments(format!(
            "resolved host selector '{}' to {}, but executable was not found at {}",
            selector_input,
            version,
            executable.display()
        )));
    }

    Ok((version, executable))
}

fn validate_venv_name(value: &str) -> Result<&str> {
    if value.is_empty() {
        return Err(MultiPwshError::InvalidArguments(
            "virtual environment name cannot be empty".to_string(),
        ));
    }

    if value == "." || value == ".." {
        return Err(MultiPwshError::InvalidArguments(format!(
            "virtual environment name '{}' is reserved",
            value
        )));
    }

    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Ok(value);
    }

    Err(MultiPwshError::InvalidArguments(format!(
        "virtual environment name '{}' is invalid; expected only ASCII letters, digits, '.', '-', or '_'",
        value
    )))
}

fn normalize_host_flag(arg: &OsStr) -> String {
    arg.to_string_lossy().to_ascii_lowercase()
}

fn is_virtual_environment_flag(arg: &OsStr) -> bool {
    matches!(
        normalize_host_flag(arg).as_str(),
        VIRTUAL_ENVIRONMENT_FLAG | VIRTUAL_ENVIRONMENT_SHORT_FLAG
    )
}

fn is_option_like(arg: &OsStr) -> bool {
    let text = arg.to_string_lossy();
    text.starts_with('-') || text.starts_with('/')
}

fn extract_virtual_environment_arg(args: Vec<OsString>) -> Result<(Vec<OsString>, Option<String>)> {
    let mut virtual_environment_index = None;
    let mut virtual_environment_name = None;

    for (index, arg) in args.iter().enumerate() {
        if !is_virtual_environment_flag(arg.as_os_str()) {
            continue;
        }

        if virtual_environment_index.is_some() {
            return Err(MultiPwshError::InvalidArguments(
                "-VirtualEnvironment can only be specified once".to_string(),
            ));
        }

        let value = args.get(index + 1).ok_or_else(|| {
            MultiPwshError::InvalidArguments("-VirtualEnvironment requires a virtual environment name".to_string())
        })?;

        if is_option_like(value.as_os_str()) {
            return Err(MultiPwshError::InvalidArguments(
                "-VirtualEnvironment requires a virtual environment name".to_string(),
            ));
        }

        let value = value.to_string_lossy().into_owned();
        validate_venv_name(&value)?;

        virtual_environment_index = Some(index);
        virtual_environment_name = Some(value);
    }

    let Some(index) = virtual_environment_index else {
        return Ok((args, None));
    };

    let mut rewritten = Vec::with_capacity(args.len().saturating_sub(2));
    rewritten.extend_from_slice(&args[..index]);
    rewritten.extend_from_slice(&args[index + 2..]);

    Ok((rewritten, virtual_environment_name))
}

fn preprocess_host_args(args: Vec<OsString>) -> Result<HostLaunchOptions> {
    let (args, virtual_environment) = extract_virtual_environment_arg(args)?;
    let pwsh_args = pwsh_host::preprocess_named_pipe_command_args(args)
        .map_err(|error| MultiPwshError::Host(format!("invalid host arguments: {}", error)))?;

    Ok(HostLaunchOptions {
        pwsh_args,
        virtual_environment,
    })
}

fn disable_powershell_update_notifications() {
    unsafe { env::set_var(POWERSHELL_UPDATECHECK_ENV_VAR, POWERSHELL_UPDATECHECK_OFF) };
}

fn resolve_virtual_environment_dir(layout: &InstallLayout, name: &str) -> Result<PathBuf> {
    let name = validate_venv_name(name)?;
    let venv_dir = layout.venv_dir(name);

    if !venv_dir.is_dir() {
        return Err(MultiPwshError::InvalidArguments(format!(
            "virtual environment '{}' was not found at {}; create it with: multi-pwsh venv create {}",
            name,
            venv_dir.display(),
            name
        )));
    }

    Ok(venv_dir)
}

fn zip_file_options() -> zip::write::FileOptions {
    zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn format_archive_entry_name(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn append_directory_to_zip<W: io::Write + io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    root_dir: &Path,
    current_dir: &Path,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current_dir)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let entry_path = entry.path();
        let relative_path = entry_path.strip_prefix(root_dir).map_err(|error| {
            MultiPwshError::Archive(format!(
                "failed to strip archive root '{}' from '{}': {}",
                root_dir.display(),
                entry_path.display(),
                error
            ))
        })?;
        let archive_name = format_archive_entry_name(relative_path);

        if entry_path.is_dir() {
            writer.add_directory(format!("{}/", archive_name), zip_file_options())?;
            append_directory_to_zip(writer, root_dir, &entry_path)?;
            continue;
        }

        writer.start_file(archive_name, zip_file_options())?;
        let mut source = fs::File::open(&entry_path)?;
        io::copy(&mut source, writer)?;
    }

    Ok(())
}

fn export_virtual_environment_to_archive(venv_dir: &Path, archive_path: &Path) -> Result<()> {
    if let Some(parent) = archive_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let archive_file = fs::File::create(archive_path)?;
    let mut writer = zip::ZipWriter::new(archive_file);
    append_directory_to_zip(&mut writer, venv_dir, venv_dir)?;
    writer.finish()?;
    Ok(())
}

fn sanitize_archive_entry_path(name: &str) -> Result<PathBuf> {
    let mut sanitized = PathBuf::new();

    for component in Path::new(name).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => sanitized.push(value),
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(MultiPwshError::Archive(format!(
                    "archive entry '{}' contains an invalid path",
                    name
                )));
            }
        }
    }

    Ok(sanitized)
}

fn import_virtual_environment_from_archive(venv_dir: &Path, archive_path: &Path) -> Result<()> {
    if !archive_path.is_file() {
        return Err(MultiPwshError::InvalidArguments(format!(
            "archive '{}' was not found",
            archive_path.display()
        )));
    }

    if venv_dir.exists() {
        return Err(MultiPwshError::InvalidArguments(format!(
            "virtual environment destination '{}' already exists",
            venv_dir.display()
        )));
    }

    fs::create_dir_all(venv_dir)?;

    let import_result = (|| -> Result<()> {
        let archive_file = fs::File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(archive_file)?;

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let relative_path = sanitize_archive_entry_path(entry.name())?;

            if relative_path.as_os_str().is_empty() {
                continue;
            }

            let destination_path = venv_dir.join(relative_path);

            if entry.is_dir() {
                fs::create_dir_all(&destination_path)?;
                continue;
            }

            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut destination = fs::File::create(&destination_path)?;
            io::copy(&mut entry, &mut destination)?;
        }

        Ok(())
    })();

    if let Err(error) = import_result {
        let _ = fs::remove_dir_all(venv_dir);
        return Err(error);
    }

    Ok(())
}

fn run_host_mode(selector_input: &str, pwsh_args: Vec<OsString>) -> Result<i32> {
    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let (_version, executable) = resolve_host_executable(&layout, selector_input)?;
    let HostLaunchOptions {
        pwsh_args,
        virtual_environment,
    } = preprocess_host_args(pwsh_args)?;
    disable_powershell_update_notifications();

    let _virtual_environment_guard = virtual_environment
        .as_deref()
        .map(|name| resolve_virtual_environment_dir(&layout, name))
        .transpose()?
        .map(|venv_dir| ProcessEnvVarGuard::set(PSMODULEPATH_ENV_VAR, venv_dir.as_os_str()));

    pwsh_host::run_pwsh_command_line_for_pwsh_exe(&executable, pwsh_args).map_err(|error| {
        MultiPwshError::Host(format!(
            "failed to start native host for selector '{}': {}",
            selector_input, error
        ))
    })
}

fn run_host_command(args: &[String]) -> Result<i32> {
    if args.is_empty() {
        return Err(MultiPwshError::InvalidArguments(
            "host requires: <version|major|major.minor|pwsh-alias> [-VirtualEnvironment <name>|-venv <name>] [pwsh arguments...]"
                .to_string(),
        ));
    }

    let selector = &args[0];
    let pwsh_args: Vec<OsString> = args[1..].iter().map(OsString::from).collect();
    run_host_mode(selector, pwsh_args)
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn executable_selector_name(executable_path: &Path) -> Option<String> {
    let file_name = executable_path.file_name()?.to_str()?;

    if file_name.len() > 4 && file_name.to_ascii_lowercase().ends_with(".exe") {
        return Some(file_name[..file_name.len() - 4].to_string());
    }

    Some(file_name.to_string())
}

fn detect_implicit_host_selector(bin_dir: &Path, executable_path: &Path) -> Option<String> {
    let selector = executable_selector_name(executable_path)?;
    if selector.eq_ignore_ascii_case("multi-pwsh") {
        return None;
    }

    if parse_alias_command_selector(&selector).is_none() {
        return None;
    }

    let parent = executable_path.parent()?;
    if !paths_refer_to_same_location(parent, bin_dir) {
        return None;
    }

    Some(selector)
}

fn run_implicit_host_mode_if_needed() -> Result<Option<i32>> {
    let executable_path = env::current_exe()?;

    let selector_name = match executable_selector_name(&executable_path) {
        Some(selector_name) => selector_name,
        None => return Ok(None),
    };
    if selector_name.eq_ignore_ascii_case("multi-pwsh") || parse_alias_command_selector(&selector_name).is_none() {
        return Ok(None);
    }

    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;

    let bin_dir = layout.bin_dir();
    let Some(selector) = detect_implicit_host_selector(&bin_dir, &executable_path) else {
        return Ok(None);
    };

    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let exit_code = run_host_mode(&selector, args)?;
    Ok(Some(exit_code))
}

fn latest_installed_in_major(layout: &InstallLayout, major: u64) -> Result<Option<Version>> {
    let versions = layout.installed_versions()?;
    Ok(versions.into_iter().find(|version| version.major == major))
}

fn latest_installed_in_line(layout: &InstallLayout, line: MajorMinor) -> Result<Option<Version>> {
    let versions = layout.installed_versions()?;
    Ok(versions
        .into_iter()
        .find(|version| version.major == line.major && version.minor == line.minor))
}

fn sync_minor_alias(layout: &InstallLayout, os: HostOs, line: MajorMinor) -> Result<Option<PathBuf>> {
    let pinned = read_minor_pin(layout, line)?;
    let target_version = match pinned {
        Some(version) => Some(version),
        None => latest_installed_in_line(layout, line)?,
    };

    let Some(target_version) = target_version else {
        let alias_name = format!("pwsh-{}.{}", line.major, line.minor);
        remove_alias(layout, os, &alias_name)?;
        return Ok(None);
    };

    let target = layout.version_executable(&target_version);
    if !target.exists() {
        return Ok(None);
    }

    let path = create_or_update_alias(layout, os, line, &target_version, &target)?;
    Ok(Some(path))
}

fn parse_alias_set_target(target: &str) -> Result<Option<Version>> {
    if target.eq_ignore_ascii_case("latest") {
        return Ok(None);
    }

    let version = parse_exact_version(target)?;
    Ok(Some(version))
}

fn run_venv(args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(MultiPwshError::InvalidArguments(
            "venv requires: create <name>, delete <name>, export <name> <archive.zip>, import <name> <archive.zip>, or list"
                .to_string(),
        ));
    }

    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    match args[0].as_str() {
        "create" => {
            if args.len() != 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "venv create requires: <name>".to_string(),
                ));
            }

            let name = validate_venv_name(&args[1])?;
            let venv_dir = layout.venv_dir(name);
            fs::create_dir_all(&venv_dir)?;

            println!("Virtual environment: {}", name);
            println!("Path: {}", venv_dir.display());
            Ok(())
        }
        "delete" => {
            if args.len() != 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "venv delete requires: <name>".to_string(),
                ));
            }

            let name = validate_venv_name(&args[1])?;
            let venv_dir = layout.venv_dir(name);

            if !venv_dir.is_dir() {
                return Err(MultiPwshError::InvalidArguments(format!(
                    "virtual environment '{}' was not found at {}",
                    name,
                    venv_dir.display()
                )));
            }

            fs::remove_dir_all(&venv_dir)?;

            println!("Deleted virtual environment: {}", name);
            println!("Path: {}", venv_dir.display());
            Ok(())
        }
        "export" => {
            if args.len() != 3 {
                return Err(MultiPwshError::InvalidArguments(
                    "venv export requires: <name> <archive.zip>".to_string(),
                ));
            }

            let name = validate_venv_name(&args[1])?;
            let venv_dir = resolve_virtual_environment_dir(&layout, name)?;
            let archive_path = PathBuf::from(&args[2]);

            export_virtual_environment_to_archive(&venv_dir, &archive_path)?;

            println!("Exported virtual environment: {}", name);
            println!("Archive: {}", archive_path.display());
            Ok(())
        }
        "import" => {
            if args.len() != 3 {
                return Err(MultiPwshError::InvalidArguments(
                    "venv import requires: <name> <archive.zip>".to_string(),
                ));
            }

            let name = validate_venv_name(&args[1])?;
            let venv_dir = layout.venv_dir(name);
            let archive_path = PathBuf::from(&args[2]);

            import_virtual_environment_from_archive(&venv_dir, &archive_path)?;

            println!("Imported virtual environment: {}", name);
            println!("Path: {}", venv_dir.display());
            println!("Archive: {}", archive_path.display());
            Ok(())
        }
        "list" => {
            if args.len() != 1 {
                return Err(MultiPwshError::InvalidArguments(
                    "venv list does not accept additional arguments".to_string(),
                ));
            }

            let venvs_dir = layout.venvs_dir();
            println!("Venv root: {}", venvs_dir.display());

            let mut entries: Vec<String> = fs::read_dir(&venvs_dir)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    if !entry.path().is_dir() {
                        return None;
                    }

                    entry.file_name().into_string().ok()
                })
                .collect();
            entries.sort();

            if entries.is_empty() {
                println!("Virtual environments: (none)");
            } else {
                println!("Virtual environments:");
                for entry in entries {
                    println!("  - {}", entry);
                }
            }

            Ok(())
        }
        _ => Err(MultiPwshError::InvalidArguments(
            "venv requires: create <name>, delete <name>, export <name> <archive.zip>, import <name> <archive.zip>, or list"
                .to_string(),
        )),
    }
}

fn run_alias(args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(MultiPwshError::InvalidArguments(
            "alias requires: set <major.minor> <version|latest> or unset <major.minor>".to_string(),
        ));
    }

    let os = HostOs::detect()?;
    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    match args[0].as_str() {
        "set" => {
            if args.len() != 3 {
                return Err(MultiPwshError::InvalidArguments(
                    "alias set requires: <major.minor> <version|latest>".to_string(),
                ));
            }

            let line = parse_major_minor_selector(&args[1])?;
            let target = parse_alias_set_target(&args[2])?;

            if let Some(version) = target.as_ref() {
                if version.major != line.major || version.minor != line.minor {
                    return Err(MultiPwshError::InvalidArguments(format!(
                        "version {} does not match alias line {}",
                        version, line
                    )));
                }
            }

            set_minor_pin(&layout, line, target.clone())?;

            let alias_name = format!("pwsh-{}.{}", line.major, line.minor);
            if let Some(version) = target {
                let target_path = layout.version_executable(&version);
                if target_path.exists() {
                    let alias_path = create_or_update_alias(&layout, os, line, &version, &target_path)?;
                    println!("Pinned alias {} to {}", alias_name, version);
                    println!("Updated alias: {}", alias_path.display());
                } else {
                    remove_alias(&layout, os, &alias_name)?;
                    println!(
                        "Pinned alias {} to {} (target is not currently installed; alias is unresolved)",
                        alias_name, version
                    );
                }
            } else {
                let alias_path = sync_minor_alias(&layout, os, line)?;
                println!("Unpinned alias {} (now follows latest in line)", alias_name);
                if let Some(path) = alias_path {
                    println!("Updated alias: {}", path.display());
                }
            }

            Ok(())
        }
        "unset" => {
            if args.len() != 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "alias unset requires: <major.minor>".to_string(),
                ));
            }

            let line = parse_major_minor_selector(&args[1])?;
            set_minor_pin(&layout, line, None)?;

            let alias_path = sync_minor_alias(&layout, os, line)?;
            println!(
                "Removed pin for pwsh-{}.{}, now following latest in line",
                line.major, line.minor
            );
            if let Some(path) = alias_path {
                println!("Updated alias: {}", path.display());
            }
            Ok(())
        }
        _ => Err(MultiPwshError::InvalidArguments(
            "alias requires: set <major.minor> <version|latest> or unset <major.minor>".to_string(),
        )),
    }
}

fn parse_release_selection_options(args: &[String]) -> Result<ReleaseSelectionOptions> {
    let mut arch = None;
    let mut arch_specified = false;
    let mut include_prerelease = false;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--arch" | "-a" => {
                if index + 1 >= args.len() {
                    return Err(MultiPwshError::InvalidArguments(
                        "expected value after --arch".to_string(),
                    ));
                }

                if arch_specified {
                    return Err(MultiPwshError::InvalidArguments(
                        "--arch can only be specified once".to_string(),
                    ));
                }
                arch_specified = true;

                if args[index + 1] == "auto" {
                    arch = None;
                } else {
                    arch = Some(HostArch::parse(&args[index + 1]).ok_or_else(|| {
                        MultiPwshError::InvalidArguments(format!(
                            "unsupported architecture '{}', expected one of: auto, x64, x86, arm64, arm32",
                            args[index + 1]
                        ))
                    })?);
                }

                index += 2;
            }
            "--include-prerelease" | "--prerelease" => {
                include_prerelease = true;
                index += 1;
            }
            _ => {
                return Err(MultiPwshError::InvalidArguments(
                    "expected optional --arch <value> and/or --include-prerelease".to_string(),
                ));
            }
        }
    }

    Ok(ReleaseSelectionOptions {
        arch,
        include_prerelease,
    })
}

fn run_install(selector_input: &str, arch: Option<HostArch>, include_prerelease: bool) -> Result<()> {
    let selector = parse_install_selector(selector_input)?;
    let os = HostOs::detect()?;
    let arch = arch.unwrap_or_else(HostArch::detect);

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let token = env::var("GITHUB_TOKEN").ok();
    let release_client = ReleaseClient::new(token)?;
    let releases = match selector {
        VersionSelector::MajorMinorWildcard(line) => {
            release_client.resolve_all_in_line(line, os, arch, include_prerelease)?
        }
        _ => vec![release_client.resolve_selector(selector, os, arch, include_prerelease)?],
    };

    let mut touched_lines: Vec<MajorMinor> = Vec::new();
    let mut touched_majors: Vec<u64> = Vec::new();

    for release in releases {
        let executable_path = ensure_installed(&layout, release_client.http_client(), os, &release)?;
        let patch_alias = create_or_update_patch_alias(&layout, os, &release.version, &executable_path)?;
        let version_path = executable_path.parent().unwrap_or_else(|| Path::new(""));

        println!("Installed PowerShell {}", release.version);
        println!("Version path: {}", version_path.display());
        println!("Updated patch alias: {}", patch_alias.display());

        let line = release.version_line();
        if !touched_lines.contains(&line) {
            touched_lines.push(line);
        }
        if !touched_majors.contains(&release.version.major) {
            touched_majors.push(release.version.major);
        }
    }

    touched_lines.sort();
    touched_majors.sort();

    for line in touched_lines {
        let pinned = read_minor_pin(&layout, line)?;
        let alias_path = sync_minor_alias(&layout, os, line)?;
        match alias_path {
            Some(path) => println!("Updated alias: {}", path.display()),
            None if pinned.is_some() => {
                println!(
                    "Alias pwsh-{}.{} remains pinned but unresolved (target is not installed)",
                    line.major, line.minor
                );
            }
            None => {}
        }
    }

    for major in touched_majors {
        let major_alias_path = latest_installed_in_major(&layout, major)?
            .map(|version| {
                let target = layout.version_executable(&version);
                create_or_update_major_alias(&layout, os, version.major, &version, &target)
            })
            .transpose()?;

        if let Some(path) = major_alias_path {
            println!("Updated major alias: {}", path.display());
        }
    }

    println!("Add to PATH once: {}", layout.bin_dir().display());

    Ok(())
}

fn run_update(line_input: &str, arch: Option<HostArch>, include_prerelease: bool) -> Result<()> {
    let line = parse_major_minor_selector(line_input)?;
    let os = HostOs::detect()?;
    let arch = arch.unwrap_or_else(HostArch::detect);

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    let token = env::var("GITHUB_TOKEN").ok();
    let release_client = ReleaseClient::new(token)?;
    let release = release_client.resolve_latest_in_line(line, os, arch, include_prerelease)?;
    let executable_path = ensure_installed(&layout, release_client.http_client(), os, &release)?;
    let patch_alias_path = create_or_update_patch_alias(&layout, os, &release.version, &executable_path)?;
    let version_path = executable_path.parent().unwrap_or_else(|| Path::new(""));

    let alias_path = sync_minor_alias(&layout, os, line)?;
    let major_alias_path = latest_installed_in_major(&layout, release.version.major)?
        .map(|version| {
            let target = layout.version_executable(&version);
            create_or_update_major_alias(&layout, os, version.major, &version, &target)
        })
        .transpose()?;

    println!("Updated line {} to {}", line, release.version);
    println!("Version path: {}", version_path.display());
    println!("Updated patch alias: {}", patch_alias_path.display());
    if let Some(path) = alias_path {
        println!("Updated alias: {}", path.display());
    } else if read_minor_pin(&layout, line)?.is_some() {
        println!(
            "Alias pwsh-{}.{} remains pinned but unresolved (target is not installed)",
            line.major, line.minor
        );
    }
    if let Some(path) = major_alias_path {
        println!("Updated major alias: {}", path.display());
    }
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

fn parse_list_option(args: &[String]) -> Result<ListOption> {
    if args.is_empty() {
        return Ok(ListOption::Installed);
    }

    let mut available = false;
    let mut include_prerelease = false;

    for arg in args {
        match arg.as_str() {
            "--available" | "--online" => {
                available = true;
            }
            "--include-prerelease" | "--prerelease" => {
                include_prerelease = true;
            }
            _ => {
                return Err(MultiPwshError::InvalidArguments(
                    "expected optional --available and/or --include-prerelease".to_string(),
                ));
            }
        }
    }

    if include_prerelease && !available {
        return Err(MultiPwshError::InvalidArguments(
            "--include-prerelease requires --available".to_string(),
        ));
    }

    if available {
        return Ok(ListOption::Available { include_prerelease });
    }

    Err(MultiPwshError::InvalidArguments(
        "expected optional --available".to_string(),
    ))
}

fn run_uninstall(version_input: &str, force: bool) -> Result<()> {
    let version = parse_exact_version(version_input)?;
    let os = HostOs::detect()?;

    let layout = InstallLayout::new(os)?;
    layout.ensure_base_dirs()?;

    if layout.remove_version_dirs(&version)? {
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
    let mut unresolved_pinned_aliases = 0usize;

    for alias_name in affected_aliases {
        let alias_selector = parse_alias_command_selector(&alias_name);
        let fallback_version = match alias_selector {
            Some(AliasSelector::MajorMinor(line)) => {
                let pinned = read_minor_pin(&layout, line)?;
                if pinned.as_ref() == Some(&version) {
                    println!(
                        "Keeping pinned alias {} -> {} (target is now unresolved)",
                        alias_name, version
                    );
                    unresolved_pinned_aliases += 1;
                    continue;
                }

                installed_versions
                    .iter()
                    .find(|candidate| MajorMinor::from_version(candidate) == line)
                    .cloned()
            }
            Some(AliasSelector::Major(major)) => installed_versions
                .iter()
                .find(|candidate| candidate.major == major)
                .cloned(),
            Some(AliasSelector::Exact(_)) => None,
            None => None,
        };

        if let Some(fallback_version) = fallback_version {
            let target = layout.version_executable(&fallback_version);
            let alias_path = match alias_selector {
                Some(AliasSelector::MajorMinor(line)) => {
                    create_or_update_alias(&layout, os, line, &fallback_version, &target)?
                }
                Some(AliasSelector::Major(major)) => {
                    create_or_update_major_alias(&layout, os, major, &fallback_version, &target)?
                }
                Some(AliasSelector::Exact(_)) => create_or_update_patch_alias(&layout, os, &fallback_version, &target)?,
                None => continue,
            };
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
        "Alias cleanup complete: {} updated, {} removed, {} pinned unresolved",
        updated_aliases, removed_aliases, unresolved_pinned_aliases
    );

    Ok(())
}

fn run_list(option: ListOption) -> Result<()> {
    match option {
        ListOption::Installed => {
            let os = HostOs::detect()?;
            let layout = InstallLayout::new(os)?;
            let versions = layout.installed_versions()?;
            let aliases = aliases::read_alias_metadata(&layout)?;
            let pins = read_minor_pins(&layout)?;

            println!("Home: {}", layout.home().display());
            println!("Alias bin: {}", layout.bin_dir().display());
            println!("Versions dir: {}", layout.versions_dir().display());
            println!("Venv dir: {}", layout.venvs_dir().display());
            println!("Cache dir: {}", layout.cache_dir().display());
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

            println!();
            if pins.is_empty() {
                println!("Minor alias pins: (none)");
            } else {
                println!("Minor alias pins:");
                let mut items: Vec<_> = pins.into_iter().collect();
                items.sort_by(|a, b| a.0.cmp(&b.0));
                for (line, version) in items {
                    println!("  - {} -> {}", line, version);
                }
            }

            Ok(())
        }
        ListOption::Available { include_prerelease } => {
            let token = env::var("GITHUB_TOKEN").ok();
            let release_client = ReleaseClient::new(token)?;
            let versions = release_client.list_available_versions(include_prerelease)?;

            if versions.is_empty() {
                println!("Available online versions: (none)");
                return Ok(());
            }

            if include_prerelease {
                println!("Available online versions (including prerelease):");
            } else {
                println!("Available online versions:");
            }
            for version in versions {
                println!("  - {}", version);
            }

            Ok(())
        }
    }
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
    let mut relinked_shims = 0usize;

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

        if aliases::repair_host_shim_if_needed(&layout, os, &alias_name)? {
            println!(
                "Relinked host shim: {}",
                if os == HostOs::Windows {
                    layout
                        .bin_dir()
                        .join(format!("{}.exe", alias_name))
                        .display()
                        .to_string()
                } else {
                    layout.bin_dir().join(&alias_name).display().to_string()
                }
            );
            relinked_shims += 1;
        }

        let alias_path = match parse_alias_command_selector(&alias_name) {
            Some(AliasSelector::MajorMinor(line)) => create_or_update_alias(&layout, os, line, &version, &target)?,
            Some(AliasSelector::Major(major)) => create_or_update_major_alias(&layout, os, major, &version, &target)?,
            Some(AliasSelector::Exact(_)) => create_or_update_patch_alias(&layout, os, &version, &target)?,
            None => {
                eprintln!("Skipping alias {}: unsupported alias name format", alias_name);
                skipped += 1;
                continue;
            }
        };
        println!("Repaired alias: {}", alias_path.display());
        repaired += 1;
    }

    if matches!(os, HostOs::Windows | HostOs::Linux | HostOs::Macos) {
        println!(
            "Repair complete: {} repaired, {} skipped, {} host shims relinked",
            repaired, skipped, relinked_shims
        );
    } else {
        println!("Repair complete: {} repaired, {} skipped", repaired, skipped);
    }
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
                    "install requires <version|major|major.minor|major.minor.x>".to_string(),
                ));
            }
            let options = parse_release_selection_options(&args[2..])?;
            run_install(&args[1], options.arch, options.include_prerelease)
        }
        "update" => {
            if args.len() < 2 {
                return Err(MultiPwshError::InvalidArguments(
                    "update requires <major.minor>".to_string(),
                ));
            }
            let options = parse_release_selection_options(&args[2..])?;
            run_update(&args[1], options.arch, options.include_prerelease)
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
            let list_option = parse_list_option(&args[1..])?;
            run_list(list_option)
        }
        "venv" => run_venv(&args[1..]),
        "alias" => run_alias(&args[1..]),
        "host" => {
            let exit_code = run_host_command(&args[1..])?;
            process::exit(exit_code);
        }
        "doctor" => run_doctor(&args[1..]),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        command => Err(MultiPwshError::InvalidArguments(format!(
            "unknown command '{}'. expected: install, update, uninstall, list, venv, alias, host, doctor",
            command
        ))),
    }
}

fn main_impl() -> Result<()> {
    if let Some(exit_code) = run_implicit_host_mode_if_needed()? {
        process::exit(exit_code);
    }

    run()
}

fn main() {
    if let Err(error) = main_impl() {
        eprintln!("error: {}", error);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_var<T>(key: &str, value: Option<&str>, action: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
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

    #[test]
    fn parse_list_option_defaults_to_installed() {
        let args: Vec<String> = Vec::new();
        assert!(matches!(parse_list_option(&args).unwrap(), ListOption::Installed));
    }

    #[test]
    fn parse_list_option_accepts_available() {
        let args = vec!["--available".to_string()];
        assert!(matches!(
            parse_list_option(&args).unwrap(),
            ListOption::Available {
                include_prerelease: false
            }
        ));
    }

    #[test]
    fn parse_list_option_accepts_available_with_prerelease() {
        let args = vec!["--available".to_string(), "--include-prerelease".to_string()];
        assert!(matches!(
            parse_list_option(&args).unwrap(),
            ListOption::Available {
                include_prerelease: true
            }
        ));
    }

    #[test]
    fn parse_list_option_rejects_unexpected_args() {
        let args = vec!["--arch".to_string(), "x64".to_string()];
        assert!(parse_list_option(&args).is_err());
    }

    #[test]
    fn parse_list_option_rejects_prerelease_without_available() {
        let args = vec!["--include-prerelease".to_string()];
        assert!(parse_list_option(&args).is_err());
    }

    #[test]
    fn parse_host_selector_supports_alias_name() {
        let selector = parse_host_selector("pwsh-7.4").unwrap();
        assert_eq!(selector, HostSelector::MajorMinor(MajorMinor { major: 7, minor: 4 }));
    }

    #[test]
    fn parse_host_selector_supports_exact_version() {
        let selector = parse_host_selector("7.4.13").unwrap();
        assert_eq!(selector, HostSelector::Exact(Version::parse("7.4.13").unwrap()));
    }

    #[test]
    fn disable_powershell_update_notifications_sets_off() {
        with_env_var(POWERSHELL_UPDATECHECK_ENV_VAR, Some("LTS"), || {
            disable_powershell_update_notifications();
            assert_eq!(
                env::var(POWERSHELL_UPDATECHECK_ENV_VAR).unwrap(),
                POWERSHELL_UPDATECHECK_OFF
            );
        });
    }

    #[test]
    fn detect_implicit_host_selector_accepts_alias_in_bin_dir() {
        let bin_dir = PathBuf::from("C:/Users/test/.pwsh/bin");

        let selector = detect_implicit_host_selector(&bin_dir, &bin_dir.join("pwsh-7.4.exe"));
        assert_eq!(selector, Some("pwsh-7.4".to_string()));
    }

    #[test]
    fn detect_implicit_host_selector_accepts_posix_alias_with_dot() {
        let bin_dir = PathBuf::from("/home/test/.pwsh/bin");

        let selector = detect_implicit_host_selector(&bin_dir, &bin_dir.join("pwsh-7.4"));
        assert_eq!(selector, Some("pwsh-7.4".to_string()));
    }

    #[test]
    fn detect_implicit_host_selector_accepts_alias_in_overridden_bin_dir() {
        let bin_dir = PathBuf::from("D:/tools/multi-pwsh/bin");

        let selector = detect_implicit_host_selector(&bin_dir, &bin_dir.join("pwsh-7.5.exe"));
        assert_eq!(selector, Some("pwsh-7.5".to_string()));
    }

    #[test]
    fn detect_implicit_host_selector_rejects_multi_pwsh_name() {
        let bin_dir = PathBuf::from("C:/Users/test/.pwsh/bin");

        let selector = detect_implicit_host_selector(&bin_dir, &bin_dir.join("multi-pwsh.exe"));
        assert!(selector.is_none());
    }

    #[test]
    fn detect_implicit_host_selector_rejects_outside_bin_dir() {
        let bin_dir = PathBuf::from("C:/Users/test/.pwsh/bin");

        let selector = detect_implicit_host_selector(&bin_dir, &PathBuf::from("C:/Users/test/other/pwsh-7.4.exe"));
        assert!(selector.is_none());
    }

    #[test]
    fn parse_release_selection_options_accepts_prerelease() {
        let args = vec!["--include-prerelease".to_string()];
        let options = parse_release_selection_options(&args).unwrap();
        assert!(options.include_prerelease);
        assert!(options.arch.is_none());
    }

    #[test]
    fn parse_release_selection_options_accepts_arch_and_prerelease() {
        let args = vec![
            "--arch".to_string(),
            "x64".to_string(),
            "--include-prerelease".to_string(),
        ];
        let options = parse_release_selection_options(&args).unwrap();
        assert!(options.include_prerelease);
        assert!(matches!(options.arch, Some(HostArch::X64)));
    }

    #[test]
    fn parse_alias_set_target_accepts_latest() {
        assert!(parse_alias_set_target("latest").unwrap().is_none());
        assert!(parse_alias_set_target("LATEST").unwrap().is_none());
    }

    #[test]
    fn parse_alias_set_target_accepts_exact_version() {
        let version = parse_alias_set_target("7.4.11").unwrap().unwrap();
        assert_eq!(version, Version::parse("7.4.11").unwrap());
    }

    #[test]
    fn validate_venv_name_accepts_simple_name() {
        assert_eq!(validate_venv_name("msgraph").unwrap(), "msgraph");
        assert_eq!(validate_venv_name("graph-sdk_1.0").unwrap(), "graph-sdk_1.0");
    }

    #[test]
    fn validate_venv_name_rejects_reserved_or_path_like_values() {
        assert!(validate_venv_name("").is_err());
        assert!(validate_venv_name("..").is_err());
        assert!(validate_venv_name("msgraph/tools").is_err());
    }

    #[test]
    fn extract_virtual_environment_arg_removes_host_only_pair() {
        let args = vec![
            OsString::from("-NoProfile"),
            OsString::from("-VirtualEnvironment"),
            OsString::from("msgraph"),
            OsString::from("-Command"),
            OsString::from("$env:PSModulePath"),
        ];

        let (rewritten, virtual_environment) = extract_virtual_environment_arg(args).unwrap();

        assert_eq!(virtual_environment, Some("msgraph".to_string()));
        assert_eq!(
            rewritten,
            vec![
                OsString::from("-NoProfile"),
                OsString::from("-Command"),
                OsString::from("$env:PSModulePath"),
            ]
        );
    }

    #[test]
    fn extract_virtual_environment_arg_rejects_duplicate_flag() {
        let args = vec![
            OsString::from("-VirtualEnvironment"),
            OsString::from("one"),
            OsString::from("-venv"),
            OsString::from("two"),
        ];

        assert!(extract_virtual_environment_arg(args).is_err());
    }

    #[test]
    fn extract_virtual_environment_arg_accepts_short_flag() {
        let args = vec![
            OsString::from("-venv"),
            OsString::from("msgraph"),
            OsString::from("-NoProfile"),
        ];

        let (rewritten, virtual_environment) = extract_virtual_environment_arg(args).unwrap();

        assert_eq!(virtual_environment, Some("msgraph".to_string()));
        assert_eq!(rewritten, vec![OsString::from("-NoProfile")]);
    }

    #[test]
    fn preprocess_host_args_combines_virtual_environment_and_named_pipe_processing() {
        let args = vec![
            OsString::from("-VirtualEnvironment"),
            OsString::from("msgraph"),
            OsString::from("-NoProfile"),
        ];

        let options = preprocess_host_args(args).unwrap();
        assert_eq!(options.virtual_environment, Some("msgraph".to_string()));
        assert_eq!(options.pwsh_args, vec![OsString::from("-NoProfile")]);
    }

    #[test]
    fn process_env_var_guard_restores_previous_value() {
        with_env_var(PSMODULEPATH_ENV_VAR, Some("original"), || {
            {
                let _guard = ProcessEnvVarGuard::set(PSMODULEPATH_ENV_VAR, "override");
                assert_eq!(env::var(PSMODULEPATH_ENV_VAR).unwrap(), "override");
            }

            assert_eq!(env::var(PSMODULEPATH_ENV_VAR).unwrap(), "original");
        });
    }

    #[test]
    fn sanitize_archive_entry_path_accepts_normal_relative_paths() {
        assert_eq!(
            sanitize_archive_entry_path("Module/1.0.0/Module.psm1").unwrap(),
            PathBuf::from("Module").join("1.0.0").join("Module.psm1")
        );
    }

    #[test]
    fn sanitize_archive_entry_path_rejects_parent_segments() {
        assert!(sanitize_archive_entry_path("../escape.txt").is_err());
    }
}
