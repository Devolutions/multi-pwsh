# BundledPwshHost

This sample shows how to:

- target `.NET 10`;
- reference `Microsoft.PowerShell.SDK` from managed code;
- publish a merged PowerShell-style output;
- use the `pwsh.exe` shipped in the `PowerShell` NuGet package;
- boot `pwsh.dll` from the publish directory without depending on a separately installed PowerShell.

## What the publish step does

`dotnet publish` for this sample:

1. publishes the sample app as a self-contained `.NET 10` app;
2. overlays the `PowerShell` NuGet package payload (`pwsh.exe`, `pwsh.dll`, `pwsh.deps.json`, modules, and related files);
4. rewrites `pwsh.runtimeconfig.json` to the local `.NET 10` shared framework version;
5. copies the matching shared runtime folders from the local dotnet installation into `publish\shared\...`.

The bundled `pwsh.exe` is the official apphost that ships inside the `PowerShell` NuGet package payload. The sample keeps that executable and adjusts the runtime layout around it so it can start against the local `.NET 10` runtime copied into the publish output.

The package versions are intentionally linked: the `PowerShell` payload package reuses the same version property as `Microsoft.PowerShell.SDK` (`BundledPwshSdkVersion`), so changing the SDK version automatically changes the imported `pwsh.exe` / `pwsh.dll` payload version too.

## Prerequisites

Publish the sample from this directory so the local `global.json` selects `.NET 10`:

```powershell
dotnet publish -c Release -f net10.0-windows -r win-x64
```

## Validate the output

Run the managed sample app:

```powershell
.\bin\Release\net10.0-windows\win-x64\publish\BundledPwshHost.exe
```

Run the bundled PowerShell host:

```powershell
.\bin\Release\net10.0-windows\win-x64\publish\pwsh.exe -NoLogo -NoProfile -Command '$PSHOME; [System.Runtime.InteropServices.RuntimeInformation]::FrameworkDescription'
```

On a successful run:

- `BundledPwshHost.exe` shows a PowerShell SDK invocation result;
- `pwsh.exe` reports the sample publish directory as `$PSHOME`;
- `pwsh.exe` reports the local `.NET 10` runtime version from the bundled layout.

## Current notes

- The payload selection logic is RID-aware (`win` / `unix`), but the validated path in this repository session is `win-x64`.
- The PowerShell packages currently emit `NU1903` vulnerability warnings for transitive dependencies during publish. The sample does not suppress them.
- This sample is a proof of concept for local hosting layout. It intentionally favors clarity and reproducibility over minimal output size.
