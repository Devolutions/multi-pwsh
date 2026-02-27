use std::path::PathBuf;
use std::process::Command;

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

#[test]
fn update_accepts_include_prerelease_flag() {
    let exe = find_multi_pwsh_binary();
    assert!(
        exe.exists(),
        "failed to locate multi-pwsh test binary at {}",
        exe.display()
    );

    let output = Command::new(exe)
        .args(["update", "not-a-line", "--include-prerelease"])
        .output()
        .expect("failed to run multi-pwsh test binary");

    assert!(!output.status.success(), "expected command to fail on invalid line selector");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a major.minor selector"),
        "expected selector parse error, got stderr: {}",
        stderr
    );
}
