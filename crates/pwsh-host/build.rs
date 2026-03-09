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
    let mut dotnet_dir = manifest_dir.clone();
    dotnet_dir.push("..");
    dotnet_dir.push("..");
    dotnet_dir.push("dotnet");
    let workspace_root = dotnet_dir.parent().unwrap();

    let bindings_project = dotnet_dir
        .join("bindings")
        .join("Devolutions.PowerShell.SDK.Bindings.csproj");
    let managed_common_props = dotnet_dir.join("Managed.Common.props");
    let bindings_sources = [
        dotnet_dir.join("bindings").join("Bindings.cs"),
        dotnet_dir.join("bindings").join("Bindings.Generated.cs"),
    ];
    let startup_hook_project = dotnet_dir
        .join("startup-hook")
        .join("Devolutions.PowerShell.SDK.StartupHook.csproj");
    let startup_hook_sources = [
        dotnet_dir.join("startup-hook").join("StartupHook.cs"),
        dotnet_dir
            .join("startup-hook")
            .join("StartupHook.ModuleManagementCmdlets.cs"),
        dotnet_dir
            .join("startup-hook")
            .join("StartupHook.ModulePathOverride.cs"),
        dotnet_dir.join("startup-hook").join("StartupHook.NativePatch.cs"),
        dotnet_dir.join("startup-hook").join("StartupHook.PowerShellGet.cs"),
        dotnet_dir.join("startup-hook").join("StartupHook.PSResourceGet.cs"),
    ];

    println!("cargo:rerun-if-changed={}", bindings_project.display());
    println!("cargo:rerun-if-changed={}", startup_hook_project.display());
    for bindings_source in bindings_sources {
        println!("cargo:rerun-if-changed={}", bindings_source.display());
    }
    for startup_hook_source in startup_hook_sources {
        println!("cargo:rerun-if-changed={}", startup_hook_source.display());
    }
    println!("cargo:rerun-if-changed={}", managed_common_props.display());
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("scripts").join("Discover-Bindings.ps1").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("scripts").join("Generate-Bindings.ps1").display()
    );

    build_dotnet_project(&bindings_project);
    build_dotnet_project(&startup_hook_project);
}
