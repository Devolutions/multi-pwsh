use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

fn build_dotnet_project(project_path: &Path) {
    let output = Command::new("dotnet")
        .arg("build")
        .arg(project_path.to_str().unwrap())
        .arg("-c")
        .arg("Release")
        .output()
        .expect("failed to execute dotnet build command");

    if !output.status.success() {
        panic!(
            "dotnet build failed for {} with status {}\nstdout:\n{}\nstderr:\n{}",
            project_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn main() {
    if env::var_os("PWSH_HOST_SKIP_DOTNET_BUILD").is_some() {
        println!("cargo:warning=skipping dotnet build because PWSH_HOST_SKIP_DOTNET_BUILD is set");
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut dotnet_source_dir = manifest_dir.clone();
    dotnet_source_dir.push("..");
    dotnet_source_dir.push("..");
    dotnet_source_dir.push("dotnet");

    let mut startup_hook_project = manifest_dir.clone();
    startup_hook_project.push("..");
    startup_hook_project.push("..");
    startup_hook_project.push("prototype");
    startup_hook_project.push("startup-hook");
    startup_hook_project.push("PwshModulePathStartupHook.csproj");

    println!("cargo:rerun-if-changed={}", dotnet_source_dir.display());
    println!("cargo:rerun-if-changed={}", startup_hook_project.display());
    println!(
        "cargo:rerun-if-changed={}",
        startup_hook_project
            .parent()
            .unwrap()
            .join("PwshModulePathStartupHook.cs")
            .display()
    );

    build_dotnet_project(&dotnet_source_dir);
    build_dotnet_project(&startup_hook_project);
}
