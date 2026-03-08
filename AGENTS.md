# AGENTS.md

This file is guidance for AI/code agents working in this repository.

## Scope

- Make minimal, targeted changes.
- Prefer root-cause fixes over superficial edits.
- Avoid unrelated refactors.

## Baseline

- .NET SDK pinned in `global.json` (currently `8.0.400` with `latestPatch` roll-forward).
- .NET target framework is `net8.0` in `dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj`.
- Rust crate uses edition 2018.

## PR preparation policy (mandatory)

- During local iteration, it is acceptable to skip lint/test commands for speed.
- Before any PR-ready action (`git commit` for review, `git push`, or opening/updating a PR), agents **MUST** run the PR gate below and ensure it passes.
- Do not open or update a PR with known failing lint/checks unless the user explicitly asks for that.
- If a gate command cannot run due to environment limitations, call that out clearly to the user before PR creation.

## Mandatory PR gate (match CI lint job)

Run these before opening/updating a PR to avoid immediate CI failures:

```powershell
rustup toolchain install stable --profile minimal
rustup default stable
rustup component add rustfmt clippy --toolchain stable
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

If `cargo fmt --all --check` fails, run `cargo fmt --all` and re-run the check.

## Required verification

Run these after meaningful code changes, and before PR if those changes are part of the PR:

```powershell
cargo build --all-targets
cargo test --all-targets
dotnet build dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj
dotnet test dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj --no-build
```

If dependency changes are made in `dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj`, also run:

```powershell
dotnet list dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj package --vulnerable --include-transitive
```

## Project map

- `crates/pwsh-host/src/bindings.rs`: Rust FFI surface over .NET unmanaged entry points.
- `dotnet/bindings/Bindings.cs`: Unmanaged-callable C# methods around `System.Management.Automation.PowerShell`.
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
- If your default `pwsh` is not 7.4.x, set `PwshExePath` before verification commands. With the default multi-pwsh layout, for example:

```powershell
$env:PwshExePath = "$HOME/.pwsh/bin/pwsh-7.4"
```

- For .NET 8 compatibility with current PowerShell SDK dependency chain, `UseRidGraph` is intentionally enabled.
