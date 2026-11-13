# Testing

## Bootstrap installer tests

Run the mocked Windows bootstrap installer harness:

```powershell
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-BootstrapInstallerTest.ps1
```

## Scoped install smoke tests

Run the cross-platform scoped install smoke harness:

```powershell
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-ScopedInstallSmokeTest.ps1
```

## Venv cmdlet matrix tests

Use the local Pester harness to validate venv-sensitive cmdlet behavior across installed version aliases (`pwsh-x.y.z`).

Run all installed version aliases:

```powershell
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-VenvTestMatrix.ps1
```

Run one alias only:

```powershell
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-VenvTestMatrix.ps1 -Aliases pwsh-7.4.13
```

Include online install tests (`Install-PSResource` / `Install-Module`):

```powershell
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-VenvTestMatrix.ps1 -EnableOnlineTests
```

Online mode details:

- The tests do not modify PSGallery trust policy.
- `Install-PSResource` uses `-TrustRepository -Quiet`.
- `Install-Module` uses `-Force -AcceptLicense -Confirm:$false`.
- The install checks use `Yayaml` to keep downloads and execution lightweight.

Useful flags:

- `-KeepVenv` keeps the temporary venvs for troubleshooting.
- `-ContinueOnFailure` keeps running after a failed alias.

Pester must be available in the host PowerShell session.

## CLI NuGet AppHost smoke tests

Build the native NuGet package first, then run the AppHost smoke harness:

```powershell
pwsh -NoLogo -NoProfile -File .\scripts\Build-NativeNuGetPackages.ps1 -RuntimeIdentifiers win-x64 -Clean
pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-AppHostNuGetPackageSmokeTest.ps1 -RuntimeIdentifier win-x64
```

The smoke harness creates a temporary SDK-style sample project, restores `Devolutions.MultiPwsh.Cli` from the local package source, validates AppHost build/publish output copying, and runs a renamed local apphost beside `pwsh.dll` and `pwsh.runtimeconfig.json` when a PowerShell payload is available.

## In-process FFI SDK package tests

Pack the SDK and run the self-contained Win-x64 NativeAOT package harness
against real PowerShell 7.4 and 7.7 payload roots (the directories holding
`pwsh.dll` and `pwsh.runtimeconfig.json`):

```powershell
dotnet pack dotnet\sdk-ffi\Devolutions.MultiPwsh.Sdk.csproj -c Release -o artifacts\sdk-nuget
foreach ($payloadRoot in @('<7.4-payload-root>', '<7.7-payload-root>')) {
    pwsh -NoLogo -NoProfile -File .\tests\Test-PwshFfiPackage.ps1 `
        -PackageSource artifacts\sdk-nuget `
        -PowerShellPayloadDirectory $payloadRoot
}
```

The harness restores the inspected local `.nupkg` into an isolated NuGet cache,
verifies the restored package against its SHA-512, publishes a NativeAOT
consumer, and exercises the SDK surface end to end. It prints the qualified
PowerShell version and cross-checks it against the `PowerShellFileVersion`
reported by `PowerShellRuntime.Diagnostics`.

Other PowerShell lines require the explicit `-AllowPowerShellVersionMismatch`
opt-in and are not covered by this qualification.

The harness normally requires the package version to match the crate version.
For an explicitly named local preview built with a package-version override,
pass `-AllowPreviewVersionMismatch`; this opt-in retains all package hash and
NativeAOT checks while making the version mismatch visible as a warning.

Verify the public/binding API baseline after changing any exported surface:

```powershell
pwsh -NoLogo -NoProfile -File .\tests\Verify-PwshFfiApiBaseline.ps1
```

The verifier exits non-zero on any drift; update `tests\PwshFfiApiBaseline.txt`
deliberately in the same change that alters the surface.

Contract-pack rejection fixtures run through the NativeAOT sample. Each must
exit `0` by being rejected — see
[in-process FFI](in-process-ffi.md#contract-packs-are-a-coordinated-breaking-release)
for the fixture list and the rejection each one proves.
