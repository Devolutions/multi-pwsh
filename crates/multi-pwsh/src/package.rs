use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{MultiPwshError, Result};
use crate::layout::InstallLayout;
use crate::platform::{HostArch, HostOs};

pub const PACKAGE_METADATA_FILE: &str = "package-installs.json";
const EXPLORER_CONTEXT_MENU_KEY: &str = "MultiPwsh.OpenPowerShell";
const FILE_CONTEXT_MENU_KEY: &str = "MultiPwsh.RunWithPowerShell";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageScope {
    CurrentUser,
    AllUsers,
}

impl PackageScope {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "current-user" | "currentuser" | "user" => Some(PackageScope::CurrentUser),
            "all-users" | "allusers" | "machine" | "system" => Some(PackageScope::AllUsers),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            PackageScope::CurrentUser => "current-user",
            PackageScope::AllUsers => "all-users",
        }
    }

    fn registry_target_name(self) -> &'static str {
        match self {
            PackageScope::CurrentUser => "User",
            PackageScope::AllUsers => "Machine",
        }
    }

    fn registry_drive(self) -> &'static str {
        match self {
            PackageScope::CurrentUser => "HKCU:",
            PackageScope::AllUsers => "HKLM:",
        }
    }

    fn classes_root(self) -> &'static str {
        match self {
            PackageScope::CurrentUser => r"Registry::HKEY_CURRENT_USER\Software\Classes",
            PackageScope::AllUsers => r"Registry::HKEY_LOCAL_MACHINE\Software\Classes",
        }
    }

    fn start_menu_programs_dir(self) -> Result<PathBuf> {
        let base = match self {
            PackageScope::CurrentUser => env::var_os("APPDATA").map(PathBuf::from).ok_or_else(|| {
                MultiPwshError::InvalidArguments(
                    "APPDATA is not defined; unable to determine current-user Start Menu location".to_string(),
                )
            })?,
            PackageScope::AllUsers => env::var_os("ProgramData").map(PathBuf::from).ok_or_else(|| {
                MultiPwshError::InvalidArguments(
                    "ProgramData is not defined; unable to determine all-users Start Menu location".to_string(),
                )
            })?,
        };

        Ok(base
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("PowerShell"))
    }
}

#[derive(Clone, Debug)]
pub struct PackageInstallOptions {
    pub scope: PackageScope,
    pub arch: Option<HostArch>,
    pub include_prerelease: bool,
    pub install_root: Option<PathBuf>,
    pub add_path: bool,
    pub register_manifest: bool,
    pub enable_psremoting: bool,
    pub disable_telemetry: bool,
    pub add_explorer_context_menu: bool,
    pub add_file_context_menu: bool,
    pub use_mu: bool,
    pub enable_mu: bool,
}

impl PackageInstallOptions {
    pub fn with_defaults(scope: PackageScope) -> Self {
        let privileged_defaults = scope == PackageScope::AllUsers;

        Self {
            scope,
            arch: None,
            include_prerelease: false,
            install_root: None,
            add_path: true,
            register_manifest: privileged_defaults,
            enable_psremoting: false,
            disable_telemetry: false,
            add_explorer_context_menu: false,
            add_file_context_menu: false,
            use_mu: privileged_defaults,
            enable_mu: privileged_defaults,
        }
    }

    pub fn validate(&self, os: HostOs) -> Result<()> {
        if os != HostOs::Windows {
            return Err(MultiPwshError::UnsupportedPlatform(
                "package installs are currently supported only on windows".to_string(),
            ));
        }

        if self.enable_mu && !self.use_mu {
            return Err(MultiPwshError::InvalidArguments(
                "--enable-mu requires --use-mu".to_string(),
            ));
        }

        if self.scope == PackageScope::CurrentUser {
            for (enabled, flag_name) in [
                (self.register_manifest, "--register-manifest"),
                (self.enable_psremoting, "--enable-psremoting"),
                (self.use_mu, "--use-mu"),
                (self.enable_mu, "--enable-mu"),
            ] {
                if enabled {
                    return Err(MultiPwshError::InvalidArguments(format!(
                        "{} requires --scope all-users",
                        flag_name
                    )));
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackageMetadata {
    #[serde(default)]
    installs: Vec<PackageInstallRecord>,
}

impl PackageMetadata {
    #[cfg(test)]
    pub fn installs(&self) -> &[PackageInstallRecord] {
        &self.installs
    }

    pub fn upsert_install(&mut self, version: &Version, layout: &InstallLayout, options: &PackageInstallOptions) {
        let install_dir = layout.version_install_dir(version);
        let install_dir = install_dir.to_string_lossy().to_string();
        let new_record = PackageInstallRecord {
            version: version.to_string(),
            install_dir,
            add_path: options.add_path,
            register_manifest: options.register_manifest,
            enable_psremoting: options.enable_psremoting,
            disable_telemetry: options.disable_telemetry,
            add_explorer_context_menu: options.add_explorer_context_menu,
            add_file_context_menu: options.add_file_context_menu,
            use_mu: options.use_mu,
            enable_mu: options.enable_mu,
        };

        if let Some(existing) = self
            .installs
            .iter_mut()
            .find(|record| record.version == new_record.version)
        {
            *existing = new_record;
        } else {
            self.installs.push(new_record);
        }
    }

    pub fn remove_install(&mut self, version: &Version) -> bool {
        let before = self.installs.len();
        let version_text = version.to_string();
        self.installs.retain(|record| record.version != version_text);
        before != self.installs.len()
    }

    pub fn resolved_records(&self) -> Result<Vec<ResolvedPackageRecord>> {
        let mut records = Vec::new();
        for record in &self.installs {
            records.push(ResolvedPackageRecord {
                version: Version::parse(&record.version)?,
                record: record.clone(),
            });
        }

        records.sort_by(|left, right| right.version.cmp(&left.version));
        Ok(records)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageInstallRecord {
    pub version: String,
    pub install_dir: String,
    pub add_path: bool,
    pub register_manifest: bool,
    pub enable_psremoting: bool,
    pub disable_telemetry: bool,
    pub add_explorer_context_menu: bool,
    pub add_file_context_menu: bool,
    pub use_mu: bool,
    pub enable_mu: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedPackageRecord {
    pub version: Version,
    pub record: PackageInstallRecord,
}

impl ResolvedPackageRecord {
    fn executable_path(&self, os: HostOs) -> PathBuf {
        PathBuf::from(&self.record.install_dir).join(os.executable_name())
    }
}

pub fn package_layout(
    os: HostOs,
    arch: HostArch,
    scope: PackageScope,
    install_root: Option<PathBuf>,
) -> Result<InstallLayout> {
    if os != HostOs::Windows {
        return Err(MultiPwshError::UnsupportedPlatform(
            "package installs are currently supported only on windows".to_string(),
        ));
    }

    let root = match install_root {
        Some(path) => path,
        None => default_install_root(scope, arch)?,
    };

    InstallLayout::from_root_with_versions_dir(os, root.clone(), root)
}

pub fn default_install_root(scope: PackageScope, arch: HostArch) -> Result<PathBuf> {
    let base = match scope {
        PackageScope::CurrentUser => env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or_else(|| {
            MultiPwshError::InvalidArguments(
                "LOCALAPPDATA is not defined; unable to determine current-user package install root".to_string(),
            )
        })?,
        PackageScope::AllUsers => {
            let key = if arch == HostArch::X86 {
                "ProgramFiles(x86)"
            } else {
                "ProgramFiles"
            };

            env::var_os(key)
                .or_else(|| env::var_os("ProgramFiles"))
                .map(PathBuf::from)
                .ok_or_else(|| {
                    MultiPwshError::InvalidArguments(format!(
                        "{} is not defined; unable to determine all-users package install root",
                        key
                    ))
                })?
        }
    };

    Ok(base.join("PowerShell"))
}

pub fn load_package_metadata(layout: &InstallLayout) -> Result<PackageMetadata> {
    let path = package_metadata_file(layout);
    if !path.exists() {
        return Ok(PackageMetadata::default());
    }

    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_package_metadata(layout: &InstallLayout, metadata: &PackageMetadata) -> Result<()> {
    let path = package_metadata_file(layout);
    let payload = serde_json::to_string_pretty(metadata)?;
    fs::write(path, payload)?;
    Ok(())
}

pub fn package_metadata_file(layout: &InstallLayout) -> PathBuf {
    layout.home().join(PACKAGE_METADATA_FILE)
}

pub fn persist_installer_properties(
    layout: &InstallLayout,
    scope: PackageScope,
    version: &Version,
    options: &PackageInstallOptions,
) -> Result<()> {
    let key_path = format!(
        r"{}\Software\Microsoft\PowerShellCore\{}",
        scope.registry_drive(),
        installer_properties_key(version)
    );
    let install_root = layout.home().display().to_string();
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $key = {key}; \
         New-Item -Path $key -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'InstallFolder' -Value {install_root} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'AddToPath' -Value {add_path} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'RegisterManifest' -Value {register_manifest} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'EnablePSRemoting' -Value {enable_psremoting} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'DisableTelemetry' -Value {disable_telemetry} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'AddExplorerContextMenuOpenPowerShell' -Value {add_explorer} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'AddFileContextMenuRunPowerShell' -Value {add_file} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'UseMU' -Value {use_mu} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'EnableMU' -Value {enable_mu} -PropertyType String -Force | Out-Null",
        key = ps_literal(&key_path),
        install_root = ps_literal(&install_root),
        add_path = ps_literal(bool_property(options.add_path)),
        register_manifest = ps_literal(bool_property(options.register_manifest)),
        enable_psremoting = ps_literal(bool_property(options.enable_psremoting)),
        disable_telemetry = ps_literal(bool_property(options.disable_telemetry)),
        add_explorer = ps_literal(bool_property(options.add_explorer_context_menu)),
        add_file = ps_literal(bool_property(options.add_file_context_menu)),
        use_mu = ps_literal(bool_property(options.use_mu)),
        enable_mu = ps_literal(bool_property(options.enable_mu)),
    );

    execute_windows_powershell(&script, "persist installer properties")
}

pub fn persist_installed_version_registration(
    scope: PackageScope,
    version: &Version,
    executable_path: &Path,
) -> Result<()> {
    let install_dir = executable_path
        .parent()
        .ok_or_else(|| MultiPwshError::Host("installed executable path had no parent directory".to_string()))?;
    let key_path = installed_version_registry_path(scope, version);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $key = {key}; \
         New-Item -Path $key -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'SemanticVersion' -Value {version} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'InstallLocation' -Value {install_dir} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'ExecutablePath' -Value {exe} -PropertyType String -Force | Out-Null",
        key = ps_literal(&key_path),
        version = ps_literal(&version.to_string()),
        install_dir = ps_literal(&install_dir.display().to_string()),
        exe = ps_literal(&executable_path.display().to_string()),
    );

    execute_windows_powershell(&script, "persist installed version registration")
}

pub fn remove_installed_version_registration(scope: PackageScope, version: &Version) -> Result<()> {
    let key_path = installed_version_registry_path(scope, version);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         if (Test-Path -Path {key}) {{ Remove-Item -Path {key} -Recurse -Force }}",
        key = ps_literal(&key_path),
    );

    execute_windows_powershell(&script, "remove installed version registration")
}

pub fn run_install_time_actions(executable_path: &Path, options: &PackageInstallOptions) -> Result<()> {
    let install_dir = executable_path
        .parent()
        .ok_or_else(|| MultiPwshError::Host("installed executable path had no parent directory".to_string()))?;

    if options.register_manifest {
        let script_path = install_dir.join("RegisterManifest.ps1");
        if !script_path.exists() {
            return Err(MultiPwshError::Host(format!(
                "register manifest was requested, but '{}' was not found",
                script_path.display()
            )));
        }

        run_installed_pwsh(
            executable_path,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.display().to_string(),
            ],
            "register event manifest",
        )?;
    }

    if options.enable_psremoting {
        run_installed_pwsh(
            executable_path,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "Enable-PSRemoting -Force",
            ],
            "enable psremoting",
        )?;
    }

    if options.enable_mu {
        let script_path = install_dir.join("RegisterMicrosoftUpdate.ps1");
        if !script_path.exists() {
            return Err(MultiPwshError::Host(format!(
                "microsoft update registration was requested, but '{}' was not found",
                script_path.display()
            )));
        }

        run_installed_pwsh(
            executable_path,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.display().to_string(),
            ],
            "enable microsoft update registration",
        )?;
    }

    Ok(())
}

pub fn reconcile_shared_integrations(
    layout: &InstallLayout,
    scope: PackageScope,
    metadata: &PackageMetadata,
) -> Result<()> {
    let os = HostOs::Windows;
    let resolved = metadata.resolved_records()?;
    let primary = resolved
        .iter()
        .map(|record| record.executable_path(os))
        .find(|path| path.exists());
    let stable_primary = resolved
        .iter()
        .find(|record| record.version.pre.is_empty())
        .map(|record| record.executable_path(os))
        .filter(|path| path.exists());

    sync_path_membership(
        scope,
        &layout.bin_dir(),
        resolved.iter().any(|record| record.record.add_path),
    )?;
    sync_telemetry_opt_out(scope, resolved.iter().any(|record| record.record.disable_telemetry))?;
    sync_use_mu(scope, resolved.iter().any(|record| record.record.use_mu))?;

    if let Some(primary_executable) = primary.as_ref() {
        sync_start_menu_shortcut(scope, primary_executable)?;
    } else {
        remove_start_menu_shortcut(scope)?;
    }

    if resolved.iter().any(|record| record.record.add_explorer_context_menu) {
        if let Some(primary_executable) = primary.as_ref() {
            sync_explorer_context_menu(scope, primary_executable)?;
        } else {
            remove_explorer_context_menu(scope)?;
        }
    } else {
        remove_explorer_context_menu(scope)?;
    }

    if resolved.iter().any(|record| record.record.add_file_context_menu) {
        if let Some(primary_executable) = primary.as_ref() {
            sync_file_context_menu(scope, primary_executable)?;
        } else {
            remove_file_context_menu(scope)?;
        }
    } else {
        remove_file_context_menu(scope)?;
    }

    if let Some(stable_executable) = stable_primary.as_ref() {
        sync_app_path(scope, stable_executable)?;
    } else {
        remove_app_path(scope)?;
    }

    Ok(())
}

fn installed_version_registry_path(scope: PackageScope, version: &Version) -> String {
    format!(
        r"{}\Software\Microsoft\PowerShellCore\InstalledVersions\multi-pwsh\{}",
        scope.registry_drive(),
        version
    )
}

fn installer_properties_key(version: &Version) -> &'static str {
    if version.pre.is_empty() {
        "InstallerProperties"
    } else {
        "PreviewInstallerProperties"
    }
}

fn bool_property(enabled: bool) -> &'static str {
    if enabled {
        "1"
    } else {
        "0"
    }
}

fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn run_installed_pwsh(executable_path: &Path, args: &[&str], description: &str) -> Result<()> {
    let output = Command::new(executable_path).args(args).output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };

    Err(MultiPwshError::Host(format!(
        "{} failed for '{}': {}",
        description,
        executable_path.display(),
        detail
    )))
}

fn execute_windows_powershell(script: &str, description: &str) -> Result<()> {
    if cfg!(not(windows)) {
        return Ok(());
    }

    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };

    Err(MultiPwshError::Host(format!("{} failed: {}", description, detail)))
}

fn sync_path_membership(scope: PackageScope, bin_dir: &Path, enabled: bool) -> Result<()> {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $target = {target}; \
         $scope = [System.EnvironmentVariableTarget]::{scope_name}; \
         $current = [System.Environment]::GetEnvironmentVariable('PATH', $scope); \
         $parts = @(); \
         if (-not [string]::IsNullOrWhiteSpace($current)) {{ $parts = @($current -split [System.IO.Path]::PathSeparator | Where-Object {{ -not [string]::IsNullOrWhiteSpace($_) }}) }}; \
         $normalizedTarget = $target.TrimEnd([System.IO.Path]::DirectorySeparatorChar); \
         $filtered = @($parts | Where-Object {{ $_.TrimEnd([System.IO.Path]::DirectorySeparatorChar) -ne $normalizedTarget }}); \
         if ({enabled}) {{ $filtered += $target }}; \
         $newValue = if ($filtered.Count -eq 0) {{ $null }} else {{ ($filtered -join [System.IO.Path]::PathSeparator) }}; \
         [System.Environment]::SetEnvironmentVariable('PATH', $newValue, $scope)",
        target = ps_literal(&bin_dir.display().to_string()),
        scope_name = scope.registry_target_name(),
        enabled = if enabled { "$true" } else { "$false" },
    );

    execute_windows_powershell(&script, "update PATH")
}

fn sync_telemetry_opt_out(scope: PackageScope, enabled: bool) -> Result<()> {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $scope = [System.EnvironmentVariableTarget]::{scope_name}; \
         $value = if ({enabled}) {{ '1' }} else {{ $null }}; \
         [System.Environment]::SetEnvironmentVariable('POWERSHELL_TELEMETRY_OPTOUT', $value, $scope)",
        scope_name = scope.registry_target_name(),
        enabled = if enabled { "$true" } else { "$false" },
    );

    execute_windows_powershell(&script, "update POWERSHELL_TELEMETRY_OPTOUT")
}

fn sync_use_mu(scope: PackageScope, enabled: bool) -> Result<()> {
    let key_path = format!(r"{}\Software\Microsoft\PowerShellCore", scope.registry_drive());
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $key = {key}; \
         New-Item -Path $key -Force | Out-Null; \
         if ({enabled}) {{ \
            New-ItemProperty -Path $key -Name 'UseMU' -Value 1 -PropertyType DWord -Force | Out-Null \
         }} else {{ \
            Remove-ItemProperty -Path $key -Name 'UseMU' -ErrorAction SilentlyContinue \
         }}",
        key = ps_literal(&key_path),
        enabled = if enabled { "$true" } else { "$false" },
    );

    execute_windows_powershell(&script, "update UseMU")
}

fn sync_app_path(scope: PackageScope, executable_path: &Path) -> Result<()> {
    let install_dir = executable_path
        .parent()
        .ok_or_else(|| MultiPwshError::Host("installed executable path had no parent directory".to_string()))?;
    let key_path = format!(
        r"{}\Software\Microsoft\Windows\CurrentVersion\App Paths\pwsh.exe",
        scope.registry_drive()
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $key = {key}; \
         New-Item -Path $key -Force | Out-Null; \
         New-ItemProperty -Path $key -Name '(default)' -Value {exe} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $key -Name 'Path' -Value {path_value} -PropertyType String -Force | Out-Null",
        key = ps_literal(&key_path),
        exe = ps_literal(&executable_path.display().to_string()),
        path_value = ps_literal(&install_dir.display().to_string()),
    );

    execute_windows_powershell(&script, "update App Paths")
}

fn remove_app_path(scope: PackageScope) -> Result<()> {
    let key_path = format!(
        r"{}\Software\Microsoft\Windows\CurrentVersion\App Paths\pwsh.exe",
        scope.registry_drive()
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         if (Test-Path -Path {key}) {{ Remove-Item -Path {key} -Recurse -Force }}",
        key = ps_literal(&key_path),
    );

    execute_windows_powershell(&script, "remove App Paths entry")
}

fn sync_start_menu_shortcut(scope: PackageScope, executable_path: &Path) -> Result<()> {
    let shortcut_dir = scope.start_menu_programs_dir()?;
    let link_path = shortcut_dir.join("PowerShell.lnk");
    let description = format!(
        "PowerShell {}",
        executable_path
            .parent()
            .and_then(|path| path.file_name())
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "package".to_string())
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $dir = {dir}; \
         $link = {link}; \
         New-Item -Path $dir -ItemType Directory -Force | Out-Null; \
         $shell = New-Object -ComObject WScript.Shell; \
         $shortcut = $shell.CreateShortcut($link); \
         $shortcut.TargetPath = {exe}; \
         $shortcut.Arguments = '-WorkingDirectory ~'; \
         $shortcut.Description = {description}; \
         $shortcut.IconLocation = {exe}; \
         $shortcut.Save()",
        dir = ps_literal(&shortcut_dir.display().to_string()),
        link = ps_literal(&link_path.display().to_string()),
        exe = ps_literal(&executable_path.display().to_string()),
        description = ps_literal(&description),
    );

    execute_windows_powershell(&script, "update Start Menu shortcut")
}

fn remove_start_menu_shortcut(scope: PackageScope) -> Result<()> {
    let shortcut_dir = scope.start_menu_programs_dir()?;
    let link_path = shortcut_dir.join("PowerShell.lnk");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         if (Test-Path -Path {link}) {{ Remove-Item -Path {link} -Force }}; \
         if (Test-Path -Path {dir}) {{ \
            $remaining = @(Get-ChildItem -Path {dir} -Force -ErrorAction SilentlyContinue); \
            if ($remaining.Count -eq 0) {{ Remove-Item -Path {dir} -Force }} \
         }}",
        link = ps_literal(&link_path.display().to_string()),
        dir = ps_literal(&shortcut_dir.display().to_string()),
    );

    execute_windows_powershell(&script, "remove Start Menu shortcut")
}

fn sync_explorer_context_menu(scope: PackageScope, executable_path: &Path) -> Result<()> {
    let base = scope.classes_root();
    let title = format!(
        "Open PowerShell here ({})",
        executable_path
            .parent()
            .and_then(|path| path.file_name())
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "PowerShell".to_string())
    );
    let command = format!(
        "\"{}\" -NoExit -Command \"Set-Location -LiteralPath '%V'\"",
        executable_path.display()
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $paths = @({background}, {directory}, {drive}); \
         foreach ($path in $paths) {{ \
            New-Item -Path $path -Force | Out-Null; \
            New-ItemProperty -Path $path -Name 'MUIVerb' -Value {title} -PropertyType String -Force | Out-Null; \
            New-ItemProperty -Path $path -Name 'Icon' -Value {exe} -PropertyType String -Force | Out-Null; \
            $commandKey = Join-Path $path 'command'; \
            New-Item -Path $commandKey -Force | Out-Null; \
            New-ItemProperty -Path $commandKey -Name '(default)' -Value {command} -PropertyType String -Force | Out-Null \
         }}",
        background = ps_literal(&format!(r"{}\Directory\Background\shell\{}", base, EXPLORER_CONTEXT_MENU_KEY)),
        directory = ps_literal(&format!(r"{}\Directory\shell\{}", base, EXPLORER_CONTEXT_MENU_KEY)),
        drive = ps_literal(&format!(r"{}\Drive\shell\{}", base, EXPLORER_CONTEXT_MENU_KEY)),
        title = ps_literal(&title),
        exe = ps_literal(&executable_path.display().to_string()),
        command = ps_literal(&command),
    );

    execute_windows_powershell(&script, "update Explorer context menu")
}

fn remove_explorer_context_menu(scope: PackageScope) -> Result<()> {
    let base = scope.classes_root();
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         foreach ($path in @({background}, {directory}, {drive})) {{ \
            if (Test-Path -Path $path) {{ Remove-Item -Path $path -Recurse -Force }} \
         }}",
        background = ps_literal(&format!(
            r"{}\Directory\Background\shell\{}",
            base, EXPLORER_CONTEXT_MENU_KEY
        )),
        directory = ps_literal(&format!(r"{}\Directory\shell\{}", base, EXPLORER_CONTEXT_MENU_KEY)),
        drive = ps_literal(&format!(r"{}\Drive\shell\{}", base, EXPLORER_CONTEXT_MENU_KEY)),
    );

    execute_windows_powershell(&script, "remove Explorer context menu")
}

fn sync_file_context_menu(scope: PackageScope, executable_path: &Path) -> Result<()> {
    let base = scope.classes_root();
    let title = format!(
        "Run with PowerShell ({})",
        executable_path
            .parent()
            .and_then(|path| path.file_name())
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "PowerShell".to_string())
    );
    let command = format!("\"{}\" -File \"%1\"", executable_path.display());
    let key = format!(r"{}\Microsoft.PowerShellScript.1\Shell\{}", base, FILE_CONTEXT_MENU_KEY);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         New-Item -Path {key} -Force | Out-Null; \
         New-ItemProperty -Path {key} -Name 'MUIVerb' -Value {title} -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path {key} -Name 'Icon' -Value {exe} -PropertyType String -Force | Out-Null; \
         $commandKey = Join-Path {key} 'command'; \
         New-Item -Path $commandKey -Force | Out-Null; \
         New-ItemProperty -Path $commandKey -Name '(default)' -Value {command} -PropertyType String -Force | Out-Null",
        key = ps_literal(&key),
        title = ps_literal(&title),
        exe = ps_literal(&executable_path.display().to_string()),
        command = ps_literal(&command),
    );

    execute_windows_powershell(&script, "update PowerShell file context menu")
}

fn remove_file_context_menu(scope: PackageScope) -> Result<()> {
    let base = scope.classes_root();
    let key = format!(r"{}\Microsoft.PowerShellScript.1\Shell\{}", base, FILE_CONTEXT_MENU_KEY);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         if (Test-Path -Path {key}) {{ Remove-Item -Path {key} -Recurse -Force }}",
        key = ps_literal(&key),
    );

    execute_windows_powershell(&script, "remove PowerShell file context menu")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_scope_parses_aliases() {
        assert_eq!(PackageScope::parse("current-user"), Some(PackageScope::CurrentUser));
        assert_eq!(PackageScope::parse("all-users"), Some(PackageScope::AllUsers));
        assert_eq!(PackageScope::parse("machine"), Some(PackageScope::AllUsers));
    }

    #[test]
    fn current_user_defaults_disable_machine_actions() {
        let options = PackageInstallOptions::with_defaults(PackageScope::CurrentUser);
        assert!(!options.register_manifest);
        assert!(!options.use_mu);
        assert!(!options.enable_mu);
    }

    #[test]
    fn validate_rejects_enable_mu_without_use_mu() {
        let mut options = PackageInstallOptions::with_defaults(PackageScope::AllUsers);
        options.use_mu = false;
        options.enable_mu = true;

        let error = options.validate(HostOs::Windows).unwrap_err();
        assert!(error.to_string().contains("--enable-mu requires --use-mu"));
    }

    #[test]
    fn metadata_upsert_replaces_existing_version() {
        let layout = InstallLayout::from_root_with_versions_dir(
            HostOs::Windows,
            PathBuf::from(r"C:\PowerShell"),
            PathBuf::from(r"C:\PowerShell"),
        )
        .unwrap();
        let version = Version::parse("7.4.13").unwrap();
        let mut metadata = PackageMetadata::default();
        let mut options = PackageInstallOptions::with_defaults(PackageScope::AllUsers);
        options.disable_telemetry = true;

        metadata.upsert_install(&version, &layout, &options);
        assert_eq!(metadata.installs().len(), 1);
        assert!(metadata.installs()[0].disable_telemetry);
        assert_eq!(metadata.installs()[0].install_dir, r"C:\PowerShell\7.4.13");

        options.disable_telemetry = false;
        metadata.upsert_install(&version, &layout, &options);
        assert_eq!(metadata.installs().len(), 1);
        assert!(!metadata.installs()[0].disable_telemetry);
    }

    #[test]
    fn resolved_records_sort_descending() {
        let metadata = PackageMetadata {
            installs: vec![
                PackageInstallRecord {
                    version: "7.4.12".to_string(),
                    install_dir: r"C:\PowerShell\7.4.12".to_string(),
                    add_path: true,
                    register_manifest: true,
                    enable_psremoting: false,
                    disable_telemetry: false,
                    add_explorer_context_menu: false,
                    add_file_context_menu: false,
                    use_mu: true,
                    enable_mu: true,
                },
                PackageInstallRecord {
                    version: "7.5.0".to_string(),
                    install_dir: r"C:\PowerShell\7.5.0".to_string(),
                    add_path: true,
                    register_manifest: true,
                    enable_psremoting: false,
                    disable_telemetry: false,
                    add_explorer_context_menu: false,
                    add_file_context_menu: false,
                    use_mu: true,
                    enable_mu: true,
                },
            ],
        };

        let resolved = metadata.resolved_records().unwrap();
        assert_eq!(resolved[0].version, Version::parse("7.5.0").unwrap());
        assert_eq!(resolved[1].version, Version::parse("7.4.12").unwrap());
    }

    #[test]
    fn package_layout_uses_direct_version_root() {
        let layout = package_layout(
            HostOs::Windows,
            HostArch::X64,
            PackageScope::AllUsers,
            Some(PathBuf::from(r"C:\PowerShell")),
        )
        .unwrap();

        assert_eq!(layout.home(), Path::new(r"C:\PowerShell"));
        assert_eq!(layout.bin_dir(), PathBuf::from(r"C:\PowerShell\bin"));
        assert_eq!(layout.versions_dir(), PathBuf::from(r"C:\PowerShell"));
        assert_eq!(
            layout.version_install_dir(&Version::parse("7.4.13").unwrap()),
            PathBuf::from(r"C:\PowerShell\7.4.13")
        );
    }
}
