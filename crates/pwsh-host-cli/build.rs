fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").expect("missing CARGO_PKG_VERSION");

    let mut resource = winresource::WindowsResource::new();
    resource
        .set("FileDescription", "pwsh-compatible CLI backed by pwsh-host and hostfxr")
        .set("ProductName", "pwsh-host")
        .set("InternalName", "pwsh-host")
        .set("CompanyName", "Devolutions Inc")
        .set("LegalCopyright", "Copyright 2021-2026 Devolutions Inc.")
        .set("OriginalFilename", "pwsh-host.exe")
        .set("FileVersion", &version)
        .set("ProductVersion", &version);

    resource.compile().expect("failed to compile Windows resources");
}
