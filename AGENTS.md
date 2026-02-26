# AGENTS.md

This file is guidance for AI/code agents working in this repository.

## Scope

- Make minimal, targeted changes.
- Prefer root-cause fixes over superficial edits.
- Avoid unrelated refactors.

## Baseline

- .NET SDK pinned in `global.json` (currently `8.0.400` with `latestPatch` roll-forward).
- .NET target framework is `net8.0` in `dotnet/Bindings.csproj`.
- Rust crate uses edition 2018.

## Pre-PR checklist (match CI lint job)

Run these before opening a PR to avoid lint failures:

```powershell
rustup toolchain install stable --profile minimal
rustup default stable
rustup component add rustfmt clippy --toolchain stable
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

If `cargo fmt --all --check` fails, run `cargo fmt --all` and re-run the check.

## Required verification

Run these after meaningful code changes:

```powershell
cargo build --all-targets
cargo test --all-targets
dotnet build dotnet/Bindings.csproj
dotnet test dotnet/Bindings.csproj --no-build
```

If dependency changes are made in `dotnet/Bindings.csproj`, also run:

```powershell
dotnet list dotnet/Bindings.csproj package --vulnerable --include-transitive
```

## Project map

- `crates/pwsh-host/src/bindings.rs`: Rust FFI surface over .NET unmanaged entry points.
- `dotnet/Bindings.cs`: Unmanaged-callable C# methods around `System.Management.Automation.PowerShell`.
- `crates/pwsh-host/src/cli_xml.rs`: CLIXML parsing helpers.
- `crates/pwsh-host/src/tests.rs`: behavior and integration tests used as usage references.

## Editing conventions

- Preserve existing style and naming.
- Do not introduce new dependencies unless needed.
- Keep patches small and cohesive.
- Update docs if behavior or baseline changes.

## Operational notes

- Tests and runtime behavior expect `pwsh` to be resolvable from `PATH`.
- CI installs PowerShell `stable`/`lts` (7.4.x) and that matches `Discover-Bindings.ps1` requirements.
- If your default `pwsh` is not 7.4.x, set `PwshExePath` before verification commands, for example:

```powershell
$env:PwshExePath = "$HOME/.pwsh/bin/pwsh-7.4"
```

- For .NET 8 compatibility with current PowerShell SDK dependency chain, `UseRidGraph` is intentionally enabled.
