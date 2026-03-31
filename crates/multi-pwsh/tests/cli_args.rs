use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn find_multi_pwsh_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_multi-pwsh") {
        return PathBuf::from(path);
    }

    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_multi_pwsh") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("failed to resolve current test executable path");
    path.pop();

    if path.ends_with("deps") {
        path.pop();
    }

    path.push(format!("multi-pwsh{}", std::env::consts::EXE_SUFFIX));
    path
}

fn run_multi_pwsh(args: &[&str], home: &std::path::Path) -> std::process::Output {
    let exe = find_multi_pwsh_binary();
    assert!(
        exe.exists(),
        "failed to locate multi-pwsh test binary at {}",
        exe.display()
    );

    Command::new(exe)
        .env("MULTI_PWSH_HOME", home)
        .args(args)
        .output()
        .expect("failed to run multi-pwsh test binary")
}

fn run_multi_pwsh_with_stdin(args: &[&str], home: &Path, stdin_text: &str) -> Output {
    let exe = find_multi_pwsh_binary();
    assert!(
        exe.exists(),
        "failed to locate multi-pwsh test binary at {}",
        exe.display()
    );

    let mut child = Command::new(exe)
        .env("MULTI_PWSH_HOME", home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start multi-pwsh test binary");

    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(stdin_text.as_bytes())
        .expect("failed to write stdin to multi-pwsh test binary");

    child
        .wait_with_output()
        .expect("failed to wait for multi-pwsh test binary")
}

fn normalize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n").trim().to_string()
}

fn split_module_path_entries(module_path: &str) -> Vec<PathBuf> {
    std::env::split_paths(&std::ffi::OsString::from(module_path)).collect()
}

fn quote_pwsh_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn run_pwsh(script: &str) -> Output {
    Command::new("pwsh")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .expect("failed to run pwsh")
}

fn discover_pwsh_install() -> (String, PathBuf) {
    let output = run_pwsh(
        "$exe = (Get-Command pwsh).Source; Write-Output \"$($PSVersionTable.PSVersion.ToString())|$(Split-Path -Parent $exe)\"",
    );
    assert!(
        output.status.success(),
        "failed to discover pwsh install: {}",
        normalize_output(&output.stderr)
    );

    let line = normalize_output(&output.stdout);
    let (version, install_dir) = line
        .split_once('|')
        .expect("expected version and install dir from pwsh discovery");

    (version.to_string(), PathBuf::from(install_dir))
}

#[cfg(windows)]
fn link_directory(link_path: &Path, target_path: &Path) {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
        quote_pwsh_literal(&link_path.display().to_string()),
        quote_pwsh_literal(&target_path.display().to_string())
    );
    let output = run_pwsh(&script);

    assert!(
        output.status.success(),
        "failed to create directory junction: {}",
        normalize_output(&output.stderr)
    );
}

#[cfg(unix)]
fn link_directory(link_path: &Path, target_path: &Path) {
    std::os::unix::fs::symlink(target_path, link_path).expect("failed to create directory symlink");
}

fn save_gallery_module(module_name: &str, destination: &Path) {
    std::fs::create_dir_all(destination).expect("failed to create gallery module destination");

    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $ProgressPreference = 'SilentlyContinue'; \
         Set-PSRepository -Name PSGallery -InstallationPolicy Trusted; \
         Save-Module -Name {} -Repository PSGallery -Path {} -Force",
        quote_pwsh_literal(module_name),
        quote_pwsh_literal(&destination.display().to_string())
    );

    let output = run_pwsh(&script);
    assert!(
        output.status.success(),
        "failed to save module {} from PSGallery: {}",
        module_name,
        normalize_output(&output.stderr)
    );
}

fn venv_modules_dir(venv_root: &Path) -> PathBuf {
    venv_root.join("Modules")
}

fn query_module_bases(home: &Path, selector: &str, venv: &str) -> Value {
    let command = "$result = [ordered]@{ \
            Yayaml = @(Get-Module -ListAvailable Yayaml | Select-Object -ExpandProperty ModuleBase); \
            PSToml = @(Get-Module -ListAvailable PSToml | Select-Object -ExpandProperty ModuleBase) \
        }; \
        $result | ConvertTo-Json -Compress";

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query module bases through multi-pwsh host: {}",
        normalize_output(&output.stderr)
    );

    serde_json::from_str(&normalize_output(&output.stdout)).expect("failed to parse module base JSON")
}

fn query_single_module_bases(home: &Path, selector: &str, venv: &str, module_name: &str) -> Vec<String> {
    let command = format!(
        "$result = [ordered]@{{ ModuleBases = @(Get-Module -ListAvailable {} | Select-Object -ExpandProperty ModuleBase) }}; \
         $result | ConvertTo-Json -Compress",
        quote_pwsh_literal(module_name)
    );

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query module bases through multi-pwsh host: {}",
        normalize_output(&output.stderr)
    );

    let parsed: Value =
        serde_json::from_str(&normalize_output(&output.stdout)).expect("failed to parse module base JSON");
    json_strings(&parsed, "ModuleBases")
}

fn query_venv_runtime_paths(home: &Path, selector: &str, venv: &str) -> Value {
    let command = "Import-Module PowerShellGet -ErrorAction Stop; \
        $powerShellGet = Get-Module PowerShellGet -ErrorAction Stop; \
        $result = [ordered]@{ \
            EnvModuleVenvPath = $env:PSMODULE_VENV_PATH; \
            EnvPSModulePath = $env:PSModulePath; \
            PowerShellGetCurrentUserModules = $powerShellGet.SessionState.PSVariable.GetValue('MyDocumentsModulesPath'); \
            PowerShellGetPsGetPathCurrentUser = (($powerShellGet.SessionState.PSVariable.GetValue('PSGetPath')).CurrentUserModules) \
        }; \
        $result | ConvertTo-Json -Compress";

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query venv runtime paths through multi-pwsh host: {}",
        normalize_output(&output.stderr)
    );

    serde_json::from_str(&normalize_output(&output.stdout)).expect("failed to parse venv runtime path JSON")
}

fn query_psrepositories_without_powershellget_import(home: &Path, selector: &str, venv: &str) -> Vec<String> {
    let command =
        "$result = @(Get-PSRepository | Select-Object -ExpandProperty Name); $result | ConvertTo-Json -Compress";

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query PS repositories without explicit PowerShellGet import: {}",
        normalize_output(&output.stderr)
    );

    let parsed: Value =
        serde_json::from_str(&normalize_output(&output.stdout)).expect("failed to parse PS repository JSON");

    match parsed {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect(),
        Value::String(name) => vec![name],
        _ => panic!("unexpected PS repository JSON: {:?}", parsed),
    }
}

fn set_psgallery_trusted_without_powershellget_import(home: &Path, selector: &str, venv: &str) -> String {
    let command = "Set-PSRepository -Name PSGallery -InstallationPolicy Trusted -ErrorAction Stop; (Get-PSRepository -Name PSGallery).InstallationPolicy";

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to set PSGallery trust without explicit PowerShellGet import: {}",
        normalize_output(&output.stderr)
    );

    normalize_output(&output.stdout)
}

fn query_installed_module_location_after_powershellget_import(
    home: &Path,
    selector: &str,
    venv: &str,
    module_name: &str,
) -> String {
    let command = format!(
        "Import-Module PowerShellGet -ErrorAction Stop; \
         Get-InstalledModule {} -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation",
        quote_pwsh_literal(module_name)
    );

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query installed module after PowerShellGet import: {}",
        normalize_output(&output.stderr)
    );

    normalize_output(&output.stdout)
}

fn install_module_without_powershellget_import(
    home: &Path,
    selector: &str,
    venv: &str,
    module_name: &str,
    repository_name: &str,
) -> Output {
    let command = format!(
        "$ProgressPreference = 'SilentlyContinue'; \
         Install-Module {} -Repository {} -Force -ErrorAction Stop",
        quote_pwsh_literal(module_name),
        quote_pwsh_literal(repository_name)
    );

    run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &command,
        ],
        home,
    )
}

fn query_installed_psresource_location_after_import(
    home: &Path,
    selector: &str,
    venv: &str,
    module_name: &str,
) -> String {
    let command = format!(
        "Import-Module Microsoft.PowerShell.PSResourceGet -ErrorAction Stop; \
         Get-InstalledPSResource {} -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation",
        quote_pwsh_literal(module_name)
    );

    let output = run_multi_pwsh(
        &[
            "host",
            selector,
            "-venv",
            venv,
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &command,
        ],
        home,
    );

    assert!(
        output.status.success(),
        "failed to query installed psresource after import: {}",
        normalize_output(&output.stderr)
    );

    normalize_output(&output.stdout)
}

fn json_strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .expect("expected JSON array")
        .iter()
        .filter_map(|item| item.as_str().map(ToOwned::to_owned))
        .collect()
}

fn normalize_path_for_compare(path: &Path) -> String {
    match std::fs::canonicalize(path) {
        Ok(canonical_path) => normalize_path_text(&canonical_path.to_string_lossy()),
        Err(_) => normalize_path_text(&path.to_string_lossy()),
    }
}

fn normalize_path_text(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").to_ascii_lowercase();

    if let Some(stripped) = normalized.strip_prefix("\\\\?\\unc\\") {
        normalized = format!("\\\\{}", stripped);
    } else if let Some(stripped) = normalized.strip_prefix("\\\\?\\") {
        normalized = stripped.to_string();
    } else if let Some(stripped) = normalized.strip_prefix("\\\\.\\") {
        normalized = stripped.to_string();
    }

    normalized
}

fn output_contains_module_base_under(paths: &[String], root: &Path) -> bool {
    let root = normalize_path_for_compare(root);
    paths
        .iter()
        .map(|path| normalize_path_for_compare(Path::new(path)))
        .any(|path| path.starts_with(&root))
}

fn pwsh_executable_name() -> &'static str {
    if cfg!(windows) {
        "pwsh.exe"
    } else {
        "pwsh"
    }
}

#[test]
fn update_accepts_include_prerelease_flag() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output = run_multi_pwsh(&["update", "not-a-line", "--include-prerelease"], temp_dir.path());

    assert!(
        !output.status.success(),
        "expected command to fail on invalid line selector"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a major.minor selector"),
        "expected selector parse error, got stderr: {}",
        stderr
    );
}

#[test]
fn venv_create_and_list_use_multi_pwsh_home() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let create_output = run_multi_pwsh(&["venv", "create", "msgraph"], temp_dir.path());
    assert!(
        create_output.status.success(),
        "expected venv create to succeed: {}",
        String::from_utf8_lossy(&create_output.stderr)
    );

    let expected_venv = temp_dir.path().join("venv").join("msgraph");
    let expected_modules = venv_modules_dir(&expected_venv);
    assert!(
        expected_venv.is_dir(),
        "expected venv dir at {}",
        expected_venv.display()
    );
    assert!(
        expected_modules.is_dir(),
        "expected venv modules dir at {}",
        expected_modules.display()
    );

    let list_output = run_multi_pwsh(&["venv", "list"], temp_dir.path());
    assert!(
        list_output.status.success(),
        "expected venv list to succeed: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        stdout.contains("Virtual environments:"),
        "unexpected stdout: {}",
        stdout
    );
    assert!(stdout.contains("msgraph"), "unexpected stdout: {}", stdout);

    let delete_output = run_multi_pwsh(&["venv", "delete", "msgraph"], temp_dir.path());
    assert!(
        delete_output.status.success(),
        "expected venv delete to succeed: {}",
        String::from_utf8_lossy(&delete_output.stderr)
    );
    assert!(
        !expected_venv.exists(),
        "expected venv dir to be removed at {}",
        expected_venv.display()
    );

    let list_after_delete = run_multi_pwsh(&["venv", "list"], temp_dir.path());
    assert!(
        list_after_delete.status.success(),
        "expected venv list after delete to succeed: {}",
        String::from_utf8_lossy(&list_after_delete.stderr)
    );

    let stdout_after_delete = String::from_utf8_lossy(&list_after_delete.stdout);
    assert!(
        stdout_after_delete.contains("Virtual environments: (none)"),
        "unexpected stdout after delete: {}",
        stdout_after_delete
    );
}

#[test]
fn venv_delete_reports_missing_name() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output = run_multi_pwsh(&["venv", "delete", "missing"], temp_dir.path());

    assert!(
        !output.status.success(),
        "expected venv delete to fail for missing name"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("virtual environment 'missing' was not found"),
        "unexpected stderr: {}",
        stderr
    );
}

#[test]
fn venv_export_and_import_round_trip_module_contents() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let create_output = run_multi_pwsh(&["venv", "create", "roundtrip"], temp_dir.path());
    assert!(
        create_output.status.success(),
        "expected venv create to succeed: {}",
        normalize_output(&create_output.stderr)
    );

    let original_venv = temp_dir.path().join("venv").join("roundtrip");
    let module_dir = venv_modules_dir(&original_venv).join("RoundTripModule");
    std::fs::create_dir_all(&module_dir).expect("failed to create test module dir");
    std::fs::write(
        module_dir.join("RoundTripModule.psm1"),
        "function Get-RoundTripValue { 'roundtrip-ok' }\n",
    )
    .expect("failed to write test module");
    std::fs::write(module_dir.join("data.txt"), "roundtrip-data").expect("failed to write test data");

    let archive_path = temp_dir.path().join("roundtrip.zip");
    let archive_text = archive_path.display().to_string();

    let export_output = run_multi_pwsh(&["venv", "export", "roundtrip", &archive_text], temp_dir.path());
    assert!(
        export_output.status.success(),
        "expected venv export to succeed: {}",
        normalize_output(&export_output.stderr)
    );
    assert!(archive_path.is_file(), "expected archive at {}", archive_path.display());

    let delete_output = run_multi_pwsh(&["venv", "delete", "roundtrip"], temp_dir.path());
    assert!(
        delete_output.status.success(),
        "expected venv delete after export to succeed: {}",
        normalize_output(&delete_output.stderr)
    );

    let import_output = run_multi_pwsh(&["venv", "import", "roundtrip-copy", &archive_text], temp_dir.path());
    assert!(
        import_output.status.success(),
        "expected venv import to succeed: {}",
        normalize_output(&import_output.stderr)
    );

    let imported_venv = temp_dir.path().join("venv").join("roundtrip-copy");
    let imported_data = venv_modules_dir(&imported_venv)
        .join("RoundTripModule")
        .join("data.txt");
    assert!(
        imported_data.is_file(),
        "expected imported data at {}",
        imported_data.display()
    );
    assert_eq!(
        std::fs::read_to_string(&imported_data).expect("failed to read imported data"),
        "roundtrip-data"
    );

    let (version, pwsh_install_dir) = discover_pwsh_install();
    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let module_bases = query_single_module_bases(temp_dir.path(), &version, "roundtrip-copy", "RoundTripModule");
    assert!(
        output_contains_module_base_under(&module_bases, &venv_modules_dir(&imported_venv)),
        "expected imported module to be discoverable from imported venv, got {:?}",
        module_bases
    );
}

#[test]
fn venv_import_rejects_existing_destination() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let source_output = run_multi_pwsh(&["venv", "create", "source"], temp_dir.path());
    assert!(
        source_output.status.success(),
        "expected source venv create to succeed: {}",
        normalize_output(&source_output.stderr)
    );

    let archive_path = temp_dir.path().join("source.zip");
    let archive_text = archive_path.display().to_string();
    let export_output = run_multi_pwsh(&["venv", "export", "source", &archive_text], temp_dir.path());
    assert!(
        export_output.status.success(),
        "expected source export to succeed: {}",
        normalize_output(&export_output.stderr)
    );

    let existing_output = run_multi_pwsh(&["venv", "create", "existing"], temp_dir.path());
    assert!(
        existing_output.status.success(),
        "expected destination venv create to succeed: {}",
        normalize_output(&existing_output.stderr)
    );

    let import_output = run_multi_pwsh(&["venv", "import", "existing", &archive_text], temp_dir.path());
    assert!(
        !import_output.status.success(),
        "expected import into existing venv to fail"
    );

    let stderr = String::from_utf8_lossy(&import_output.stderr);
    assert!(stderr.contains("already exists"), "unexpected stderr: {}", stderr);
}

#[test]
fn host_reports_missing_virtual_environment_before_launching_pwsh() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let fake_pwsh_dir = temp_dir.path().join("multi").join("7.4.13");
    std::fs::create_dir_all(&fake_pwsh_dir).expect("failed to create fake pwsh dir");
    std::fs::write(fake_pwsh_dir.join(pwsh_executable_name()), b"").expect("failed to create fake pwsh exe");

    let output = run_multi_pwsh(&["host", "7.4.13", "-venv", "missing", "-NoProfile"], temp_dir.path());

    assert!(!output.status.success(), "expected host command to fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("virtual environment 'missing' was not found"),
        "unexpected stderr: {}",
        stderr
    );
}

#[test]
fn host_venv_isolates_psgallery_modules() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    for venv_name in ["yaml", "toml"] {
        let output = run_multi_pwsh(&["venv", "create", venv_name], temp_dir.path());
        assert!(
            output.status.success(),
            "failed to create venv {}: {}",
            venv_name,
            normalize_output(&output.stderr)
        );
    }

    let yaml_root = temp_dir.path().join("venv").join("yaml");
    let toml_root = temp_dir.path().join("venv").join("toml");
    let yaml_modules_root = venv_modules_dir(&yaml_root);
    let toml_modules_root = venv_modules_dir(&toml_root);

    save_gallery_module("Yayaml", &yaml_modules_root);
    save_gallery_module("PSToml", &toml_modules_root);

    let yaml_result = query_module_bases(temp_dir.path(), &version, "yaml");
    let yaml_bases = json_strings(&yaml_result, "Yayaml");
    let toml_bases_from_yaml = json_strings(&yaml_result, "PSToml");

    assert!(
        output_contains_module_base_under(&yaml_bases, &yaml_modules_root),
        "expected Yayaml to be discovered from yaml venv, got {:?}",
        yaml_bases
    );
    assert!(
        !output_contains_module_base_under(&toml_bases_from_yaml, &toml_modules_root),
        "did not expect PSToml from toml venv to leak into yaml venv, got {:?}",
        toml_bases_from_yaml
    );

    let toml_result = query_module_bases(temp_dir.path(), &version, "toml");
    let toml_bases = json_strings(&toml_result, "PSToml");
    let yaml_bases_from_toml = json_strings(&toml_result, "Yayaml");

    assert!(
        output_contains_module_base_under(&toml_bases, &toml_modules_root),
        "expected PSToml to be discovered from toml venv, got {:?}",
        toml_bases
    );
    assert!(
        !output_contains_module_base_under(&yaml_bases_from_toml, &yaml_modules_root),
        "did not expect Yayaml from yaml venv to leak into toml venv, got {:?}",
        yaml_bases_from_toml
    );
}

#[test]
fn host_venv_rewrites_powershellget_current_user_module_path() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "msgraph"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let venv_root = temp_dir.path().join("venv").join("msgraph");
    let venv_modules_root = venv_modules_dir(&venv_root);
    let runtime_paths = query_venv_runtime_paths(temp_dir.path(), &version, "msgraph");
    let expected = venv_root.to_string_lossy().to_string();
    let expected_modules = venv_modules_root.to_string_lossy().to_string();
    let module_path_entries = split_module_path_entries(
        runtime_paths["EnvPSModulePath"]
            .as_str()
            .expect("expected EnvPSModulePath string"),
    );

    assert_eq!(
        module_path_entries.len(),
        2,
        "expected venv PSModulePath to contain only the venv Modules path and bundled PSHOME modules, got {:?}",
        module_path_entries
    );
    assert_eq!(module_path_entries[0], venv_modules_root);
    assert!(
        module_path_entries[1].ends_with("Modules"),
        "expected bundled PSHOME modules path, got {:?}",
        module_path_entries[1]
    );
    assert_eq!(runtime_paths["EnvModuleVenvPath"].as_str(), Some(expected.as_str()));
    assert_eq!(
        runtime_paths["PowerShellGetCurrentUserModules"].as_str(),
        Some(expected_modules.as_str())
    );
    assert_eq!(
        runtime_paths["PowerShellGetPsGetPathCurrentUser"].as_str(),
        Some(expected_modules.as_str())
    );
}

#[test]
fn host_venv_get_psrepository_works_without_powershellget_import() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "repo-query"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let repositories = query_psrepositories_without_powershellget_import(temp_dir.path(), &version, "repo-query");

    assert!(
        repositories.iter().any(|name| name == "PSGallery"),
        "expected PSGallery to be discoverable without explicit PowerShellGet import, got {:?}",
        repositories
    );
}

#[test]
fn host_venv_set_psrepository_works_without_powershellget_import() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "repo-set"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let installation_policy = set_psgallery_trusted_without_powershellget_import(temp_dir.path(), &version, "repo-set");

    assert_eq!(
        installation_policy, "Trusted",
        "expected Set-PSRepository to succeed without explicit PowerShellGet import"
    );
}

#[test]
fn host_venv_import_module_powershellget_keeps_get_installed_module_venv_aware() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "yaml"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let venv_root = temp_dir.path().join("venv").join("yaml");
    let venv_modules_root = venv_modules_dir(&venv_root);
    save_gallery_module("Yayaml", &venv_modules_root);

    let installed_location =
        query_installed_module_location_after_powershellget_import(temp_dir.path(), &version, "yaml", "Yayaml");

    assert!(
        output_contains_module_base_under(&[installed_location], &venv_modules_root),
        "expected explicit PowerShellGet import to preserve venv-installed module discovery"
    );
}

#[test]
fn host_venv_install_module_without_powershellget_import_reaches_repository_lookup() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "yaml-install"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let output = install_module_without_powershellget_import(
        temp_dir.path(),
        &version,
        "yaml-install",
        "__multi_pwsh_missing_module__",
        "PSGallery",
    );

    assert!(
        !output.status.success(),
        "expected Install-Module to fail for a fake repository module name"
    );

    let stderr = normalize_output(&output.stderr);

    assert!(
        stderr.contains("No match was found"),
        "expected Install-Module without an explicit PowerShellGet import to reach repository lookup, got: {}",
        stderr
    );
    assert!(
        !stderr.contains("could not be loaded"),
        "did not expect a PowerShellGet module load failure, got: {}",
        stderr
    );
    assert!(
        !stderr.contains("pipeline is already running"),
        "did not expect a nested pipeline failure, got: {}",
        stderr
    );
}

#[test]
fn host_venv_stdin_import_module_powershellget_keeps_get_installed_module_venv_aware() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "yaml-stdin"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let venv_root = temp_dir.path().join("venv").join("yaml-stdin");
    let venv_modules_root = venv_modules_dir(&venv_root);
    save_gallery_module("Yayaml", &venv_modules_root);

    let host_output = run_multi_pwsh_with_stdin(
        &["host", &version, "-venv", "yaml-stdin", "-NoLogo", "-NoProfile", "-File", "-"],
        temp_dir.path(),
        "Import-Module PowerShellGet -ErrorAction Stop\nGet-InstalledModule Yayaml -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation\n",
    );

    assert!(
        host_output.status.success(),
        "stdin-driven host launch failed: {}",
        normalize_output(&host_output.stderr)
    );

    let stdout = normalize_output(&host_output.stdout);
    assert!(
        output_contains_module_base_under(&[stdout], &venv_modules_root),
        "expected stdin-driven PowerShellGet import to preserve venv-installed module discovery"
    );
}

#[test]
fn host_venv_import_module_psresourceget_keeps_get_installed_psresource_venv_aware() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "yaml-psresource"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let venv_root = temp_dir.path().join("venv").join("yaml-psresource");
    let venv_modules_root = venv_modules_dir(&venv_root);
    save_gallery_module("Yayaml", &venv_modules_root);

    let installed_location =
        query_installed_psresource_location_after_import(temp_dir.path(), &version, "yaml-psresource", "Yayaml");

    assert!(
        output_contains_module_base_under(&[installed_location], &venv_modules_root),
        "expected explicit PSResourceGet import to preserve venv-installed resource discovery"
    );
}

#[test]
fn host_venv_stdin_import_module_psresourceget_keeps_get_installed_psresource_venv_aware() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let (version, pwsh_install_dir) = discover_pwsh_install();

    let managed_version_dir = temp_dir.path().join("multi").join(&version);
    std::fs::create_dir_all(managed_version_dir.parent().expect("missing version dir parent"))
        .expect("failed to create managed multi dir");
    link_directory(&managed_version_dir, &pwsh_install_dir);

    let output = run_multi_pwsh(&["venv", "create", "yaml-psresource-stdin"], temp_dir.path());
    assert!(
        output.status.success(),
        "failed to create venv: {}",
        normalize_output(&output.stderr)
    );

    let venv_root = temp_dir.path().join("venv").join("yaml-psresource-stdin");
    let venv_modules_root = venv_modules_dir(&venv_root);
    save_gallery_module("Yayaml", &venv_modules_root);

    let host_output = run_multi_pwsh_with_stdin(
        &[
            "host",
            &version,
            "-venv",
            "yaml-psresource-stdin",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-File",
            "-",
        ],
        temp_dir.path(),
        "Import-Module Microsoft.PowerShell.PSResourceGet -ErrorAction Stop\nGet-InstalledPSResource Yayaml -ErrorAction Stop | Select-Object -First 1 -ExpandProperty InstalledLocation\n",
    );

    assert!(
        host_output.status.success(),
        "stdin-driven PSResourceGet host launch failed: {}",
        normalize_output(&host_output.stderr)
    );

    let stdout = normalize_output(&host_output.stdout);
    assert!(
        output_contains_module_base_under(&[stdout], &venv_modules_root),
        "expected stdin-driven PSResourceGet import to preserve venv-installed resource discovery"
    );
}

#[cfg(windows)]
#[test]
fn list_uses_explicit_root_and_scope() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let package_root = temp_dir.path().join("package-root");
    let package_root_text = package_root.display().to_string();

    let output = run_multi_pwsh(
        &["list", "--scope", "machine", "--root", &package_root_text],
        temp_dir.path(),
    );

    assert!(
        output.status.success(),
        "expected scoped list to succeed: {}",
        normalize_output(&output.stderr)
    );

    let stdout = normalize_output(&output.stdout);
    assert!(stdout.contains("Scope: machine"), "unexpected stdout: {}", stdout);
    assert!(
        stdout.contains(&package_root_text),
        "expected package root in output: {}",
        stdout
    );
}

#[cfg(windows)]
#[test]
fn install_rejects_machine_actions_for_current_user_scope() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output = run_multi_pwsh(
        &["install", "7.4", "--scope", "user", "--register-manifest"],
        temp_dir.path(),
    );

    assert!(
        !output.status.success(),
        "expected install to fail for current-user manifest registration"
    );

    let stderr = normalize_output(&output.stderr);
    assert!(
        stderr.contains("--register-manifest requires --scope machine"),
        "unexpected stderr: {}",
        stderr
    );
}
