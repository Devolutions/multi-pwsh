use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut dotnet_source_dir = manifest_dir.clone();
    dotnet_source_dir.push("..");
    dotnet_source_dir.push("..");
    dotnet_source_dir.push("dotnet");

    let _output = Command::new("dotnet")
        .arg("build")
        .arg(dotnet_source_dir.as_path().to_str().unwrap())
        .arg("-c")
        .arg("Release")
        .output()
        .expect("failed to execute dotnet build command");

    if !_output.status.success() {
        panic!(
            "dotnet build failed with status {}\nstdout:\n{}\nstderr:\n{}",
            _output.status,
            String::from_utf8_lossy(&_output.stdout),
            String::from_utf8_lossy(&_output.stderr)
        );
    }
}
