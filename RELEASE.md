# Release process

## 1. Prepare the version

Update crate versions and the README tag example:

```powershell
.\scripts\Bump-CrateVersions.ps1 -Version 0.9.0
```

Review the resulting diff in:

- `crates\multi-pwsh\Cargo.toml`
- `crates\pwsh-host\Cargo.toml`
- `Cargo.lock`
- `README.md`

## 2. Run release checks

```powershell
rustup toolchain install stable --profile minimal
rustup default stable
rustup component add rustfmt clippy --toolchain stable
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo build --all-targets
cargo test --all-targets
dotnet build dotnet\bindings\Devolutions.PowerShell.SDK.Bindings.csproj -p:PwshExePath="pwsh-7.4"
dotnet test dotnet\bindings\Devolutions.PowerShell.SDK.Bindings.csproj --no-build -p:PwshExePath="pwsh-7.4"
```

If the release changes the in-process FFI SDK (`Devolutions.MultiPwsh.Sdk`),
also run the API baseline verifier and the self-contained Win-x64 NativeAOT
package harness described in [testing](docs/testing.md#in-process-ffi-sdk-package-tests):

```powershell
pwsh -NoLogo -NoProfile -File .\tests\Verify-PwshFfiApiBaseline.ps1
dotnet pack dotnet\sdk-ffi\Devolutions.MultiPwsh.Sdk.csproj -c Release -o artifacts\sdk-nuget
foreach ($payloadRoot in @('<7.4-payload-root>', '<7.7-payload-root>')) {
    pwsh -NoLogo -NoProfile -File .\tests\Test-PwshFfiPackage.ps1 `
        -PackageSource artifacts\sdk-nuget `
        -PowerShellPayloadDirectory $payloadRoot
}
```

The SDK is qualified on Windows x64 against the **latest released PowerShell
7.4.x and 7.7.x**, not fixed patches. Record both exact versions the harness
prints (`Qualified PowerShell payload: <version>`) in the release notes; CI
exports them as `PwshQualifiedVersion` and `Pwsh77QualifiedVersion` and writes
them to the job summary. Do not state a patch that was not exercised.

Live-object contract packs are a coordinated breaking release: an interface
identifier, version, direction, operation shape, or pack ABI change requires
shipping the consumer and its payload adapter together. There is no version
negotiation or compatibility range.

## 3. Publish the release

Dispatch `.github/workflows/release.yml` from the branch or commit you want to release.

- For an actual publish, set `dry_run` to `false` and provide the matching `tag` value, for example `v0.9.0`.
- For an inspection build, set `dry_run` to `true`. This builds and packs artifacts, uploads the `.nupkg` as a workflow artifact, and skips GitHub release publishing.
- A non-dry-run release automatically publishes the packages to NuGet.org. NuGet publishing uses trusted publishing through the selected GitHub environment's `NUGET_BOT_USERNAME` secret.
- Set `github-env` to `auto` unless you need to force signing and NuGet publishing secrets from `publish-test` or `publish-prod`. In `auto`, runs from `master` use `publish-prod`; other branches use `publish-test`.

You do **not** need to create the tag ahead of time. If the tag does not exist yet, the workflow creates it at the dispatched commit when it creates the GitHub release.

NuGet publishing follows `dry_run`: dry runs print the NuGet push commands without executing them, while non-dry-run releases publish the packages.

The workflow:

- validates that the tag input matches both crate versions
- builds artifacts from the commit you dispatched the workflow from
- signs Windows executables on a Linux runner with Devolutions `psign` and the selected environment's Key Vault secrets
- creates the tag at that commit if needed
- uploads archives and `checksums.txt` to the GitHub release, with Windows archives containing signed executables
- builds and uploads `Devolutions.MultiPwsh.Cli.<version>.nupkg` to the GitHub release, with signed Windows payloads under `runtimes\win-*\native` and opt-in AppHost targets
- publishes `Devolutions.MultiPwsh.Cli.<version>.nupkg` to NuGet.org for non-dry-run releases
- uploads install/uninstall bootstrap scripts to the GitHub release so users do not need `raw.githubusercontent.com`

If the release already exists, the workflow uploads the refreshed assets with `--clobber`.

Non-dry-run releases fail if code-signing secrets are unavailable. Dry runs sign Windows executables when the selected environment exposes the signing secrets; otherwise the dry-run logs an explicit warning and produces unsigned Windows payloads for inspection only.

### Dry-run artifact download

When `dry_run` is `true`, download the `native-nuget` artifact from the workflow run page to inspect the `.nupkg` file.

## 4. Verify release assets

Confirm the release contains:

- all platform zip archives
- `Devolutions.MultiPwsh.Cli.<version>.nupkg`
- `checksums.txt`
- `install-multi-pwsh.ps1` and `install-multi-pwsh.sh`
- `uninstall-multi-pwsh.ps1` and `uninstall-multi-pwsh.sh`
- generated release notes or any required manual edits

## 5. Package consumption in .NET apps

Reference the package and build your project:

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.9.0" />
</ItemGroup>
```

The package contributes `multi-pwsh` payloads under `runtimes/<rid>/native/`, copies them to build/publish output, and includes opt-in AppHost targets via `MultiPwshAppHostEnabled=true`.
