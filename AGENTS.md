# AGENTS.md

This file is guidance for AI/code agents working in this repository.

## Scope

- Make minimal, targeted changes.
- Prefer root-cause fixes over superficial edits.
- Avoid unrelated refactors.

## Baseline

- .NET SDK pinned in `global.json` (currently `8.0.418`).
- .NET target framework is `net8.0` in `dotnet/Bindings.csproj`.
- Rust crate uses edition 2018.

## Required verification

Run these after meaningful code changes:

```powershell
cargo build --all-targets
cargo test --all-targets
dotnet build pwsh-host-rs.sln
dotnet test pwsh-host-rs.sln --no-build
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
- For .NET 8 compatibility with current PowerShell SDK dependency chain, `UseRidGraph` is intentionally enabled.
