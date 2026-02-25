use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};

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

fn run_invocation(
    exe: impl AsRef<OsStr>,
    args: &[OsString],
    stdin_text: Option<&str>,
    working_dir: Option<&Path>,
) -> Output {
    let mut command = Command::new(exe);
    command.args(args);

    if let Some(path) = working_dir {
        command.current_dir(path);
    }

    if let Some(input) = stdin_text {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn process");

        let mut stdin = child.stdin.take().expect("failed to open child stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write to child stdin");
        drop(stdin);

        child.wait_with_output().expect("failed to read process output")
    } else {
        command.output().expect("failed to execute process")
    }
}

fn assert_invocation_matches(args: &[OsString], stdin_text: Option<&str>, working_dir: Option<&Path>) {
    let shim = find_shim_binary();
    assert!(
        shim.exists(),
        "failed to locate pwsh-host test binary at {}",
        shim.display()
    );

    let expected = run_invocation("pwsh", args, stdin_text, working_dir);
    let actual = run_invocation(&shim, args, stdin_text, working_dir);

    assert_eq!(
        expected.status.code(),
        actual.status.code(),
        "exit code mismatch for args {:?}",
        args
    );
    assert_eq!(
        normalize_output(&expected.stdout),
        normalize_output(&actual.stdout),
        "stdout mismatch for args {:?}",
        args
    );
    assert_eq!(
        normalize_output(&expected.stderr),
        normalize_output(&actual.stderr),
        "stderr mismatch for args {:?}",
        args
    );
}

fn utf16le_base64(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|unit| unit.to_le_bytes()).collect();
    BASE64_STANDARD.encode(bytes)
}

fn os_args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
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

fn create_temp_script(contents: &str) -> (TempDirGuard, PathBuf) {
    let dir = TempDirGuard::new("pwsh_host_cli_script");
    let script_path = dir.path().join("script.ps1");
    fs::write(&script_path, contents).expect("failed to write temporary script file");
    (dir, script_path)
}

#[test]
fn reported_version_matches_pwsh() {
    assert_invocation_matches(&os_args(&["-Version"]), None, None);
}

#[test]
fn help_variants_match_pwsh() {
    for flag in ["-h", "-Help", "-?", "/?"] {
        assert_invocation_matches(&os_args(&[flag]), None, None);
    }
}

#[test]
fn command_forms_match_pwsh() {
    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "1..5 | ForEach-Object { $_ * 2 } | ConvertTo-Json -Compress",
        ]),
        None,
        None,
    );

    let encoded = utf16le_base64("'encoded-command-ok'");
    assert_invocation_matches(
        &[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-EncodedCommand"),
            OsString::from(encoded),
        ],
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-CommandWithArgs",
            "$args | ForEach-Object { \"arg: $_\" }",
            "alpha",
            "beta",
        ]),
        None,
        None,
    );
}

#[test]
fn stdin_driven_modes_match_pwsh() {
    let stdin_script = "'in'; 'hi' | ForEach-Object { \"$_ there\" }; 'out'";

    assert_invocation_matches(
        &os_args(&["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "-"]),
        Some(stdin_script),
        None,
    );

    assert_invocation_matches(
        &os_args(&["-NoLogo", "-NoProfile", "-NonInteractive", "-File", "-"]),
        Some("'stdin-file-ok'"),
        None,
    );
}

#[test]
fn file_mode_and_exit_codes_match_pwsh() {
    let (_dir, script_path) = create_temp_script(
        "param([string]$Name, [switch]$All)\n\"name=$Name all=$All\"\n",
    );

    assert_invocation_matches(
        &[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-File"),
            script_path.clone().into_os_string(),
            OsString::from("-Name"),
            OsString::from("World"),
            OsString::from("-All:$false"),
        ],
        None,
        None,
    );

    let (_exit_dir, exit_script_path) = create_temp_script("exit 23\n");
    assert_invocation_matches(
        &[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-File"),
            exit_script_path.into_os_string(),
        ],
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "exit 17"]),
        None,
        None,
    );
}

#[test]
fn startup_and_format_flags_match_pwsh() {
    let work_dir = TempDirGuard::new("pwsh_host_cli_workdir");

    assert_invocation_matches(
        &[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-WorkingDirectory"),
            work_dir.path().as_os_str().to_os_string(),
            OsString::from("-Command"),
            OsString::from("(Get-Location).ProviderPath"),
        ],
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-OutputFormat",
            "XML",
            "-Command",
            "'xml-output-ok'",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-NoProfileLoadTime",
            "-Command",
            "'no-profile-load-time-ok'",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-Login",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "'login-flag-ok'",
        ]),
        None,
        None,
    );
}

#[test]
fn custom_pipe_name_matches_pwsh() {
    let pipe_name = unique_name("pwsh_host_cli_pipe");
    assert_invocation_matches(
        &[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-CustomPipeName"),
            OsString::from(pipe_name),
            OsString::from("-Command"),
            OsString::from("'custom-pipe-ok'"),
        ],
        None,
        None,
    );
}

#[cfg(windows)]
#[test]
fn windows_specific_flags_match_pwsh() {
    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "'execution-policy-ok'",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-Command",
            "[Threading.Thread]::CurrentThread.GetApartmentState().ToString()",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-MTA",
            "-Command",
            "[Threading.Thread]::CurrentThread.GetApartmentState().ToString()",
        ]),
        None,
        None,
    );

    assert_invocation_matches(
        &os_args(&[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Normal",
            "-Command",
            "'window-style-ok'",
        ]),
        None,
        None,
    );
}
