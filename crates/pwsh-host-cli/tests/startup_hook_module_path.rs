#![cfg(all(windows, target_pointer_width = "64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn normalize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

fn find_shim_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_pwsh-host") {
        return PathBuf::from(path);
    }

    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_pwsh_host") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("failed to resolve current test executable path");
    path.pop();

    if path.ends_with("deps") {
        path.pop();
    }

    path.push(format!("pwsh-host{}", std::env::consts::EXE_SUFFIX));
    path
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    format!("{}_{}_{}", prefix, std::process::id(), nanos)
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(unique_name(prefix));
        fs::create_dir_all(&path).expect("failed to create temp directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_test_module(module_venv_path: &Path) {
    let module_root = module_venv_path.join("PwshHost.TestModule").join("1.2.3");
    fs::create_dir_all(&module_root).expect("failed to create test module directory");

    let manifest = r#"@{
    ModuleVersion = '1.2.3'
    GUID = '3d77d8db-7279-4d3b-aef5-b81b2a8a58e1'
    Author = 'pwsh-host-rs'
    Description = 'Synthetic module for startup-hook PSResourceGet coverage.'
    FunctionsToExport = @()
    CmdletsToExport = @()
    VariablesToExport = @()
    AliasesToExport = @()
    PrivateData = @{
        PSData = @{
            Tags = @('test', 'pwsh-host')
            ProjectUri = 'https://example.invalid/pwsh-host'
        }
    }
}
"#;

    fs::write(module_root.join("PwshHost.TestModule.psd1"), manifest).expect("failed to write test module manifest");
}

fn run_shim_with_module_path_startup_hook(module_venv_path: &Path) -> Output {
    let shim = find_shim_binary();
    assert!(shim.exists(), "missing shim binary at {}", shim.display());
    let runtime_temp_dir = TempDirGuard::new("pwsh_host_runtime_temp");

    let script = concat!(
        "$module = Get-Module Microsoft.PowerShell.Utility -ErrorAction Ignore;",
        "if ($module) { Remove-Module Microsoft.PowerShell.Utility -Force -ErrorAction Ignore };",
        "$reimported = [bool](Import-Module Microsoft.PowerShell.Utility -PassThru -ErrorAction Ignore);",
        "$moduleHookReady = $false;",
        "$psResourceHookReady = $false;",
        "for ($i = 0; $i -lt 500; $i++) {",
        "  $importCommand = Get-Command Import-Module -ErrorAction SilentlyContinue;",
        "  $installedModuleCommand = Get-Command Get-InstalledModule -ErrorAction SilentlyContinue;",
        "  $command = Get-Command Get-InstalledPSResource -ErrorAction SilentlyContinue;",
        "  if ($importCommand -and $importCommand.CommandType -eq 'Alias' -and $installedModuleCommand -and $installedModuleCommand.CommandType -eq 'Alias') { $moduleHookReady = $true };",
        "  if ($command -and ($command.CommandType -eq 'Alias' -or $command.CommandType -eq 'Cmdlet')) { $psResourceHookReady = $true };",
        "  if ($moduleHookReady -and $psResourceHookReady) { break };",
        "  Start-Sleep -Milliseconds 10;",
        "};",
        "if ($moduleHookReady) { Import-Module PowerShellGet -ErrorAction Stop };",
        "$powerShellGet = Get-Module PowerShellGet -ErrorAction Stop;",
        "$env:PSModulePath;",
        "([bool](Get-Command ConvertTo-Json -ErrorAction Ignore)).ToString();",
        "([bool](Get-Module Microsoft.PowerShell.Utility -ListAvailable -ErrorAction Ignore)).ToString();",
        "$reimported.ToString();",
        "$powerShellGet.SessionState.PSVariable.GetValue('MyDocumentsModulesPath');",
        "(($powerShellGet.SessionState.PSVariable.GetValue('PSGetPath')).CurrentUserModules);",
        "$psResourceCommand = if ($psResourceHookReady) { Get-Command Get-InstalledPSResource -ErrorAction SilentlyContinue } else { $null };",
        "([bool]$psResourceCommand).ToString();",
        "if ($psResourceCommand) { $psResourceCommand.CommandType.ToString() } else { '' };",
        "([bool]$env:DOTNET_STARTUP_HOOKS).ToString();",
        "$env:PSMODULE_VENV_PATH;"
    );

    let output = Command::new(shim)
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .env("TEMP", runtime_temp_dir.path())
        .env("TMP", runtime_temp_dir.path())
        .env_remove("DOTNET_STARTUP_HOOKS")
        .env_remove("PWSH_HOST_STARTUP_HOOKS")
        .env("PSMODULE_VENV_PATH", module_venv_path)
        .env_remove("PWSH_STARTUP_HOOK_STRATEGY")
        .output()
        .expect("failed to run pwsh-host with module-path startup hook");

    let extracted_hook_path = runtime_temp_dir
        .path()
        .join("pwsh-host-rs")
        .join(env!("CARGO_PKG_VERSION"))
        .join("startup-hooks")
        .join("Devolutions.PowerShell.SDK.StartupHook.dll");
    assert!(
        !extracted_hook_path.exists(),
        "startup hook should load from embedded bytes without extracting to {}",
        extracted_hook_path.display()
    );

    output
}

#[test]
fn module_path_is_the_default_startup_hook_behavior() {
    let venv_dir = TempDirGuard::new("pwsh_host_module_path");
    write_test_module(venv_dir.path());
    let output = run_shim_with_module_path_startup_hook(venv_dir.path());

    assert_eq!(
        output.status.code(),
        Some(0),
        "unexpected exit code\nstdout:\n{}\n\nstderr:\n{}",
        normalize_output(&output.stdout),
        normalize_output(&output.stderr)
    );

    let stdout = normalize_output(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 10, "unexpected stdout:\n{}", stdout);
    assert!(
        lines[0].starts_with(venv_dir.path().to_string_lossy().as_ref()),
        "PSModulePath should start with the venv path, got {}",
        lines[0]
    );
    assert!(
        lines[0].contains(';'),
        "PSModulePath should include the bundled PSHOME module path, got {}",
        lines[0]
    );
    assert_eq!(lines[1], "True", "ConvertTo-Json should remain available");
    assert_eq!(lines[2], "True", "Microsoft.PowerShell.Utility should stay listable");
    assert_eq!(
        lines[3], "True",
        "Microsoft.PowerShell.Utility should re-import by name"
    );
    assert_eq!(
        lines[4],
        venv_dir.path().to_string_lossy(),
        "PowerShellGet current-user module path should be rewritten to the venv"
    );
    assert_eq!(
        lines[5],
        venv_dir.path().to_string_lossy(),
        "PowerShellGet PSGetPath should advertise the venv as the current-user module path"
    );
    assert_eq!(
        lines[6], "True",
        "Get-InstalledPSResource should be replaced by an injected C# command"
    );
    assert!(
        matches!(lines[7], "Alias" | "Cmdlet"),
        "Get-InstalledPSResource should resolve to the injected C# command, got {}",
        lines[7]
    );
    assert_eq!(
        lines[8], "False",
        "DOTNET_STARTUP_HOOKS should not leak into process env"
    );
    assert_eq!(
        lines[9],
        venv_dir.path().to_string_lossy(),
        "module venv path should be available in process env"
    );
}
