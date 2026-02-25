# pwsh-host-rs

Rust PowerShell hosting library that loads .NET delegates and drives `System.Management.Automation.PowerShell` through unmanaged entry points.

## What this repository contains

- A Rust crate (`pwsh-host`) that loads and invokes PowerShell hosting delegates.
- A .NET bindings project (`dotnet/Bindings.csproj`) exposing `[UnmanagedCallersOnly]` methods consumed by Rust.
- Parsing and conversion helpers for PowerShell CLIXML output.

## Baseline and toolchain

- .NET SDK is pinned via `global.json` to **8.0.418**.
- .NET project target framework: **net8.0**.
- Rust crate edition: **2018**.
- Primary OS target in current setup: **Windows**.

## Prerequisites

- Rust toolchain (`cargo`, `rustc`)
- .NET SDK 8.0.418 (or update `global.json` intentionally)
- PowerShell 7+ (`pwsh`) available in `PATH`

## Build

```powershell
cargo build --all-targets
dotnet build pwsh-host-rs.sln
```

## Test

```powershell
cargo test --all-targets
dotnet test pwsh-host-rs.sln --no-build
```

## Typical Rust usage (from tests)

```rust
use pwsh_host::bindings::PowerShell;

let pwsh = PowerShell::new().unwrap();
pwsh.add_command("Get-Date");
pwsh.add_parameter_long("-UnixTimeSeconds", 1577836800);
pwsh.add_command("Set-Variable");
pwsh.add_parameter_string("-Name", "Date");
pwsh.add_statement();
pwsh.invoke(true);

let date_json = pwsh.export_to_json("Date");
assert_eq!(date_json, "\"2019-12-31T19:00:00-05:00\"");
```

## Repository layout

- `src/` – Rust host, delegate loading, CLIXML parsing, tests
- `dotnet/` – .NET unmanaged-callable bindings
- `global.json` – pinned .NET SDK version
- `pwsh-host-rs.sln` – .NET solution for bindings

## Notes

- The .NET bindings package currently uses `Microsoft.PowerShell.SDK` `7.2.24` for compatibility with this repository’s interop layer.
- `dotnet/Bindings.csproj` enables `UseRidGraph` to keep runtime-identifier compatibility under .NET 8.
