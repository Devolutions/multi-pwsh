#![cfg(windows)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};

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

fn normalize_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

fn utf16le_base64(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|unit| unit.to_le_bytes()).collect();
    BASE64_STANDARD.encode(bytes)
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    format!("{}_{}_{}", prefix, std::process::id(), nanos)
}

fn run_pwsh(args: &[OsString]) -> Output {
    Command::new("pwsh").args(args).output().expect("failed to run pwsh")
}

fn run_shim(args: &[OsString]) -> Output {
    let shim = find_shim_binary();
    assert!(shim.exists(), "missing shim binary at {}", shim.display());

    Command::new(shim).args(args).output().expect("failed to run pwsh-host")
}

fn assert_output_parity(expected: Output, actual: Output) {
    assert_eq!(expected.status.code(), actual.status.code(), "exit code mismatch");
    assert_eq!(
        normalize_output(&expected.stdout),
        normalize_output(&actual.stdout),
        "stdout mismatch"
    );
    assert_eq!(
        normalize_output(&expected.stderr),
        normalize_output(&actual.stderr),
        "stderr mismatch"
    );
}

fn spawn_pipe_server(pipe_name: &str, payload_utf8: &str, delay_ms: u32) -> Child {
    let payload_base64 = BASE64_STANDARD.encode(payload_utf8.as_bytes());
    let script = format!(
        "$pipe = [System.IO.Pipes.NamedPipeServerStream]::new('{pipe_name}', [System.IO.Pipes.PipeDirection]::Out, 1, [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::None); \
         $pipe.WaitForConnection(); \
         if ({delay_ms} -gt 0) {{ Start-Sleep -Milliseconds {delay_ms}; }}; \
         $bytes = [Convert]::FromBase64String('{payload_base64}'); \
         $pipe.Write($bytes, 0, $bytes.Length); \
         $pipe.Flush(); \
         $pipe.Dispose();"
    );

    Command::new("pwsh")
        .args(&["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn named pipe server")
}

fn assert_server_ok(server: Child) {
    let output = server.wait_with_output().expect("failed waiting for named pipe server");
    assert!(
        output.status.success(),
        "named pipe server failed: {}",
        normalize_output(&output.stderr)
    );
}

fn query_process_command_line(pid: u32) -> Option<String> {
    let script = format!(
        "$proc = Get-CimInstance Win32_Process -Filter \"ProcessId = {pid}\"; if ($null -ne $proc) {{ $proc.CommandLine }}"
    );

    for _ in 0..20 {
        let output = Command::new("pwsh")
            .args(&["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("failed to query process command line");

        let line = normalize_output(&output.stdout);
        if !line.is_empty() {
            return Some(line);
        }

        sleep(Duration::from_millis(50));
    }

    None
}

#[test]
fn named_pipe_command_matches_encoded_command_output() {
    let command = "$PSVersionTable.PSVersion.ToString()";
    let expected = run_pwsh(&[
        OsString::from("-NoLogo"),
        OsString::from("-NoProfile"),
        OsString::from("-NonInteractive"),
        OsString::from("-EncodedCommand"),
        OsString::from(utf16le_base64(command)),
    ]);

    let pipe_name = unique_name("pwsh_host_cli_named_pipe");
    let server = spawn_pipe_server(&pipe_name, command, 0);

    let actual = run_shim(&[
        OsString::from("-NoLogo"),
        OsString::from("-NoProfile"),
        OsString::from("-NonInteractive"),
        OsString::from("-NamedPipeCommand"),
        OsString::from(pipe_name),
    ]);

    assert_server_ok(server);
    assert_output_parity(expected, actual);
}

#[test]
fn named_pipe_command_conflicts_with_command_flag() {
    let output = run_shim(&[
        OsString::from("-NamedPipeCommand"),
        OsString::from("pwsh_host_cli_conflict"),
        OsString::from("-Command"),
        OsString::from("'test'"),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        normalize_output(&output.stderr).contains("cannot be combined"),
        "unexpected stderr: {}",
        normalize_output(&output.stderr)
    );
}

#[test]
fn named_pipe_command_times_out_when_pipe_is_missing() {
    let output = run_shim(&[
        OsString::from("-NamedPipeCommand"),
        OsString::from(unique_name("pwsh_host_cli_missing_pipe")),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        normalize_output(&output.stderr).contains("timed out"),
        "unexpected stderr: {}",
        normalize_output(&output.stderr)
    );
}

#[test]
fn named_pipe_command_does_not_leak_secret_in_command_line() {
    let secret = unique_name("SECRET_SENTINEL");
    let pipe_name = unique_name("pwsh_host_cli_leak");
    let payload = format!("$null = '{}'; 'named-pipe-ok'", secret);

    let server = spawn_pipe_server(&pipe_name, &payload, 750);

    let shim = find_shim_binary();
    let child = Command::new(&shim)
        .args(&[
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-NamedPipeCommand"),
            OsString::from(&pipe_name),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn shim process");

    sleep(Duration::from_millis(100));

    let command_line =
        query_process_command_line(child.id()).expect("failed to query shim command line while process is alive");
    assert!(
        !command_line.contains(&secret),
        "command line leaked secret: {}",
        command_line
    );

    let output = child.wait_with_output().expect("failed waiting for shim process");
    assert_server_ok(server);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(normalize_output(&output.stdout), "named-pipe-ok");
}
