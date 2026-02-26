# pwsh-host-rs

Rust PowerShell hosting library that loads .NET delegates and drives `System.Management.Automation.PowerShell` through unmanaged entry points.

## Workspace layout

- `crates/pwsh-host` – Rust library crate
- `crates/pwsh-host-cli` – Rust CLI crate
- `crates/multi-pwsh` – Rust tool to install/update side-by-side `pwsh` versions
- `dotnet` – unmanaged-callable .NET bindings project

## Origin

This repository follows the path discussed in the original .NET runtime issue:

- [dotnet/runtime#46652 - Native Host using existing PowerShell 7 installation](https://github.com/dotnet/runtime/issues/46652)

That thread established the core approach used here:

- There is no JNI-style "embed arbitrary managed APIs directly" surface in .NET hosting.
- Native callers should expose managed helper methods (C# glue) with native-callable ABI (for example `[UnmanagedCallersOnly]`).
- `hostfxr` is the recommended hosting path for this scenario, rather than driving lower-level `coreclr` hosting APIs directly.
- For PowerShell hosting specifically, initializing against `pwsh.dll` via `hostfxr_initialize_for_dotnet_command_line` is the key enabler for loading and invoking PowerShell in-process from native code.

## What this repository contains

- A Rust crate (`pwsh-host`) that loads and invokes PowerShell hosting delegates.
- A Rust CLI crate (`pwsh-host-cli`) that forwards arguments to PowerShell via `hostfxr_initialize_for_dotnet_command_line`.
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

## Build smaller release binaries

The default `release` profile is tuned for smaller binaries. Build `pwsh-host-cli` and `multi-pwsh` with:

```powershell
cargo build --release -p pwsh-host-cli --bin pwsh-host
cargo build --release -p multi-pwsh --bin multi-pwsh
```

If PowerShell 7.4 is not in `PATH`, pass it explicitly:

```powershell
dotnet build pwsh-host-rs.sln -p:PwshExePath="C:\Program Files\PowerShell\7.4\pwsh.exe"
```

## Discover larger PowerShell SDK surface

Generate reflection-based wrapper candidates and a generated contract with a matching PowerShell 7.4 runtime:

```powershell
& "C:\Program Files\PowerShell\7.4\pwsh.exe" -NoLogo -NoProfile -File ./scripts/Discover-Bindings.ps1
```

The build uses discovery output by default. To force an explicit generated contract path:

```powershell
dotnet build pwsh-host-rs.sln -p:PwshExePath="C:\Program Files\PowerShell\7.4\pwsh.exe" -p:BindingsContractPath="$(Resolve-Path ./dotnet/obj/bindings.ps74.discovered.contract.json)"
```

## Run `pwsh-host`

```powershell
cargo run -p pwsh-host-cli --bin pwsh-host -- -NoLogo -NoProfile -Command "$PSVersionTable.PSVersion"
```

## Manage side-by-side PowerShell installs with `multi-pwsh`

`multi-pwsh` downloads official PowerShell release archives from GitHub and installs them under the current user profile (no package manager or system installer required).

- Install root: `~/.pwsh`
- Per-version location: `~/.pwsh/<version>` (example: `~/.pwsh/7.4.13`)
- Alias location: `~/.pwsh/bin`
- Alias format: `pwsh-<major.minor>` (example: `pwsh-7.4`)

### Install `multi-pwsh` from GitHub Releases

Latest release bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/install-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/install-multi-pwsh.ps1 | iex
```

Both scripts:

- Download the latest `multi-pwsh` release archive for the current OS/architecture
- Install the executable to `~/.pwsh/bin`
- Add `~/.pwsh/bin` to PATH if missing

Uninstall bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/uninstall-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/uninstall-multi-pwsh.ps1 | iex
```

Install a specific tag (for example `v0.5.0`):

```bash
curl -fsSL https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/install-multi-pwsh.sh | bash -s -- v0.5.0
```

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/awakecoding/pwsh-host-rs/main/tools/install-multi-pwsh.ps1))) -Version v0.5.0
```

Examples:

```powershell
# Install exact version and update pwsh-7.4 alias
cargo run -p multi-pwsh -- install 7.4.13

# Install latest patch in a line and update alias
cargo run -p multi-pwsh -- install 7.4

# Update an existing line alias to the newest available patch
cargo run -p multi-pwsh -- update 7.4

# Show installed versions and alias mapping metadata
cargo run -p multi-pwsh -- list

# Repair/regenerate aliases from metadata (useful after upgrading older aliases)
cargo run -p multi-pwsh -- doctor --repair-aliases
```

When `multi-pwsh` is installed via the bootstrap scripts above, `~/.pwsh/bin` is added to PATH automatically if needed.

## `-NamedPipeCommand` (Windows)

`pwsh-host` supports a custom shim argument to read command text from a Windows named pipe and forward it to PowerShell through `-EncodedCommand`.

- Argument: `-NamedPipeCommand <pipeName>`
- Pipe payload format: UTF-8 command text
- Internally converted to UTF-16LE Base64 and passed as `-EncodedCommand`

This keeps command contents out of process command-line arguments while preserving normal non-interactive PowerShell invocation behavior.

Helper script: [scripts/Start-NamedPipeTextServer.ps1](scripts/Start-NamedPipeTextServer.ps1)

Example:

```powershell
$pipeName = "pwsh-host-$([Guid]::NewGuid().ToString('N'))"
$command = "'hello from named pipe'"

$job = Start-Job -ScriptBlock {
	param($repoRoot, $pipeName, $command)
	& (Join-Path $repoRoot "scripts/Start-NamedPipeTextServer.ps1") `
		-PipeName $pipeName `
		-Command $command | Out-Null
} -ArgumentList $PWD.Path, $pipeName, $command

cargo run -p pwsh-host-cli --bin pwsh-host -- -NoLogo -NoProfile -NonInteractive -NamedPipeCommand $pipeName

Receive-Job $job -Wait -AutoRemoveJob
```

## Test

```powershell
cargo test --all-targets
dotnet test pwsh-host-rs.sln --no-build
```

## Typical Rust usage (from tests)

```rust
use pwsh_host::PowerShell;

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

- `crates/pwsh-host/` – Rust host library crate
- `crates/pwsh-host/src/` – hostfxr interop, delegate loading, CLIXML parsing, tests
- `crates/pwsh-host-cli/` – Rust CLI crate that runs `pwsh.dll` through hostfxr command-line initialization
- `crates/multi-pwsh/` – Rust side-by-side PowerShell installer/updater and alias manager
- `dotnet/` – .NET unmanaged-callable bindings
- `global.json` – pinned .NET SDK version
- `pwsh-host-rs.sln` – .NET solution for bindings

## Notes

- The unmanaged bindings function-table contract is versioned as `PS74` (for PowerShell 7.4 on .NET 8 LTS).
- The .NET bindings package baseline uses `Microsoft.PowerShell.SDK` `7.4.13`, and should track the latest compatible `7.4.x` patch.
- `dotnet/Bindings.csproj` enables `UseRidGraph` to keep runtime-identifier compatibility under .NET 8.
- `scripts/Discover-Bindings.ps1` emits a generated contract (`dotnet/obj/bindings.ps74.discovered.contract.json`), a surface report (`dotnet/obj/powershell.ps74.surface.json`), and wrapper candidates (`dotnet/obj/Bindings.Discovered.Generated.cs`).
- `scripts/Generate-Bindings.ps1` consumes a generated contract and emits `dotnet/Bindings.Generated.cs` plus `crates/pwsh-host/src/bindings/bindings_generated.rs`.
