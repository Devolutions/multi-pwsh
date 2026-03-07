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

fn run_shim_with_module_path_startup_hook(forced_module_path: &Path) -> Output {
    let shim = find_shim_binary();
    assert!(shim.exists(), "missing shim binary at {}", shim.display());
    let runtime_temp_dir = TempDirGuard::new("pwsh_host_runtime_temp");

    let script = concat!(
        "$module = Get-Module Microsoft.PowerShell.Utility -ErrorAction Ignore;",
        "if ($module) { Remove-Module Microsoft.PowerShell.Utility -Force -ErrorAction Ignore };",
        "$reimported = [bool](Import-Module Microsoft.PowerShell.Utility -PassThru -ErrorAction Ignore);",
        "$env:PSModulePath;",
        "([bool](Get-Command ConvertTo-Json -ErrorAction Ignore)).ToString();",
        "([bool](Get-Module Microsoft.PowerShell.Utility -ListAvailable -ErrorAction Ignore)).ToString();",
        "$reimported.ToString();",
        "([bool]$env:DOTNET_STARTUP_HOOKS).ToString();",
        "([bool]$env:PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH).ToString();"
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
        .env("PWSH_STARTUP_HOOK_FORCE_PSMODULEPATH", forced_module_path)
        .env_remove("PWSH_STARTUP_HOOK_STRATEGY")
        .output()
        .expect("failed to run pwsh-host with module-path startup hook");

    let extracted_hook_path = runtime_temp_dir
        .path()
        .join("pwsh-host-rs")
        .join(env!("CARGO_PKG_VERSION"))
        .join("startup-hooks")
        .join("Devolutions.PowerShell.StartupHook.dll");
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
    assert_eq!(lines.len(), 6, "unexpected stdout:\n{}", stdout);
    assert_eq!(lines[0], venv_dir.path().to_string_lossy());
    assert_eq!(lines[1], "True", "ConvertTo-Json should remain available");
    assert_eq!(lines[2], "True", "Microsoft.PowerShell.Utility should stay listable");
    assert_eq!(
        lines[3], "True",
        "Microsoft.PowerShell.Utility should re-import by name"
    );
    assert_eq!(
        lines[4], "False",
        "DOTNET_STARTUP_HOOKS should not leak into process env"
    );
    assert_eq!(lines[5], "False", "forced module path should not leak into process env");
}
