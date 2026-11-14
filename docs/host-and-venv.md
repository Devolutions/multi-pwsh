# Native host mode and virtual environments

## Native host mode

- `multi-pwsh host <selector> ...` runs PowerShell through native hosting (`pwsh-host`) instead of launching a `pwsh` subprocess.
- `<selector>` supports `7`, `7.4`, `7.4.13`, or alias-form selectors such as `pwsh-7.4`.
- `-VirtualEnvironment <name>` and `-venv <name>` are consumed by `multi-pwsh` before handing control to PowerShell and set `PSModulePath` to the selected venv module root for that launch.
- `PSMODULE_VENV_PATH` can also be used as an explicit path-based venv selector for hosted launches. If it is already set in the environment, `multi-pwsh host` treats it as an intentional venv opt-in.
- Alias lifecycle maintains native host shims as hard links to `multi-pwsh` during install, update, and `doctor --repair-aliases`.
- On Windows, alias command paths are `pwsh-*.exe` host shims in the active install scope's `bin` directory. The default user scope uses `~/.pwsh/bin`, matching Linux/macOS.
- On Linux/macOS, alias command paths (`pwsh-*`) are hard links to `multi-pwsh`.
- `multi-pwsh doctor --repair-aliases` performs a shim health check and re-links broken hard links automatically.
- Direct `multi-pwsh host`, `alias`, `doctor`, and `venv` commands operate on the default user layout. Machine-scope aliases are normally entered through the generated machine-scope shims, which carry layout hints.
- Copying or renaming `multi-pwsh.exe` to an alias-like name such as `pwsh-7.4.exe` also enters implicit host mode.
- `-NamedPipeCommand <pipeName>` is supported in host mode on Windows.
- `-mcp -McpCommands <command> [command ...]` starts the hosted runspace as a stdio MCP server that exposes selected PowerShell commands as tools. See [MCP host mode](mcp.md).

### Local `pwsh` apphost replacement mode

As an advanced replacement workflow, `multi-pwsh` can be renamed to `pwsh`/`pwsh.exe` and placed directly in a PowerShell SDK/apphost output directory. This mode is intentionally separate from managed alias-shim mode.

Detection uses the executable path reported by the OS, not the current working directory and not `PATH`. It activates only when the executable name is exactly `pwsh` or `pwsh.exe` and either the same directory contains both `pwsh.dll` and `pwsh.runtimeconfig.json`, or the executable is under `runtimes/<rid>/native/` and those marker files exist three directories up at the shared publish root. Additional files such as `System.Management.Automation.dll`, `Microsoft.PowerShell.ConsoleHost.dll`, and `Modules/` are expected in complete PowerShell payloads but are not required as marker files.

When this local payload probe succeeds, `multi-pwsh` bypasses the managed `pwsh` alias policy and layout-shim inference, then hosts `pwsh.dll` from the resolved payload directory. Host-side preprocessing still applies, including `-venv` / `-VirtualEnvironment`, `-NamedPipeCommand`, stdin command rewriting, MCP mode, startup-hook setup, and PowerShell update-check suppression.

Hostfxr loading is app-local first. If `hostfxr` is not present beside the payload, `pwsh-host` falls back to the .NET hosting layer via `nethost`/global .NET roots, which supports framework-dependent SDK build output. Self-contained payloads still need their app-local hosting files such as `hostfxr` and `hostpolicy`.

### CLI NuGet package AppHost mode

`Devolutions.MultiPwsh.Cli` is the reusable package form of local apphost replacement mode. It ships RID-specific `multi-pwsh` binaries and `buildTransitive` AppHost targets; AppHost mode has no build side effects unless `MultiPwshAppHostEnabled` is set to `true`.

Typical downstream vendored-SDK usage:

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.19.0" PrivateAssets="all" />
</ItemGroup>

<PropertyGroup>
  <MultiPwshAppHostEnabled>true</MultiPwshAppHostEnabled>
  <MultiPwshAppHostOutputBaseName>pwsh</MultiPwshAppHostOutputBaseName>
</PropertyGroup>
```

The targets resolve the RID from `MultiPwshAppHostRuntimeIdentifier`, `PowerShellSDKAppHostRuntimeIdentifier`, `RuntimeIdentifier`, then `NETCoreSdkRuntimeIdentifier`. By default they copy `multi-pwsh` / `multi-pwsh.exe`; setting `MultiPwshAppHostOutputBaseName` to `pwsh` copies `pwsh` / `pwsh.exe`. Set `MultiPwshAppHostOutputName` for a full explicit file name. Downstream targets can also disable automatic copying with `MultiPwshAppHostCopyToOutput=false` and `MultiPwshAppHostCopyToPublish=false`, then consume `MultiPwshAppHostResolvedNativeBinary` or `@(MultiPwshAppHostNativeBinary)` directly.

## Virtual environments

`multi-pwsh` virtual environments provide isolated PowerShell module roots for hosted launches.

By default, venvs live under `~/.pwsh/venv/<name>`. If `MULTI_PWSH_VENV_DIR` is set, they live under that directory instead.

Available commands:

- `multi-pwsh venv create <name>`
- `multi-pwsh venv delete <name>`
- `multi-pwsh venv export <name> <archive.zip>`
- `multi-pwsh venv import <name> <archive.zip|url>`
- `multi-pwsh venv list`

### Create and use a venv

```powershell
multi-pwsh venv create msgraph
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "$env:PSModulePath"
```

You can also opt into a venv by path:

```powershell
$env:PSMODULE_VENV_PATH = Join-Path $HOME ".pwsh/venv/msgraph"
multi-pwsh host 7.4 -NoLogo -NoProfile
```

If both a venv flag and `PSMODULE_VENV_PATH` are present, the named flag wins for that launch.

### Populate a venv with modules

Modules should live under `<venv-root>\Modules\<ModuleName>`.

```powershell
$venvRoot = Join-Path $HOME ".pwsh/venv/msgraph"
$venvModules = Join-Path $venvRoot "Modules"
Save-Module -Name Microsoft.Graph.Authentication -Repository PSGallery -Path $venvModules -Force
Save-Module -Name Microsoft.Graph.Users -Repository PSGallery -Path $venvModules -Force
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "Get-Module -ListAvailable Microsoft.Graph.Authentication"
```

### Export and import a venv

```powershell
multi-pwsh venv export msgraph msgraph.zip
multi-pwsh venv import msgraph-copy msgraph.zip
multi-pwsh host 7.4 -venv msgraph-copy -NoLogo -NoProfile
```

Import is intentionally conservative: importing into an existing destination venv is rejected instead of merging archive contents.

`venv import` also accepts `https://` archive URLs. `http://` remote imports are rejected. For authenticated remote archives, include any required one-time credential in the URL query string.

```powershell
multi-pwsh venv import msgraph-copy "https://example.invalid/venvs/msgraph.zip?token=$appToken"
```

Remote imports cache archives under `MULTI_PWSH_CACHE_DIR` or the default cache root when the server returns an `ETag`. Later imports of the same URL send `If-None-Match` with the stored ETag; if the server returns `304 Not Modified`, `multi-pwsh` imports from the cached archive instead of downloading it again. Cached archive files are identified by both URL and ETag.

### Current behavior and limitations

- Venv selection changes module discovery and import precedence for hosted launches.
- `Install-Module` and `Install-PSResource` use the venv `Modules` directory during hosted launches.
- The effective `PSModulePath` is the venv `Modules` directory plus the bundled PSHOME `Modules` directory when present; the venv is a selected module root, not a full process sandbox.
- The feature applies to `multi-pwsh host ...`, implicit host shims such as `pwsh-7.4.exe`, and local `pwsh` apphost replacement mode, not to arbitrary external `pwsh` processes.

## Managed paths

- `MULTI_PWSH_HOME`: override the default user-scope multi-pwsh home directory.
- `MULTI_PWSH_BIN_DIR`: override the default user-scope shim and launcher directory.
- `MULTI_PWSH_CACHE_DIR`: override the default user-scope archive/download cache directory.
- `MULTI_PWSH_VENV_DIR`: override the default user-scope virtual-environment root directory.
- `MULTI_PWSH_CACHE_KEEP`: keep downloaded archives after extraction when set to a truthy value.

These `MULTI_PWSH_*` path variables affect only the default `user` layout. `machine` scope uses platform machine paths, and `--root` is an explicit install-root override that requires `--scope <user|machine>` and does not mix in child-directory overrides from the environment. Empty or whitespace-only path values are treated as unset.

Offline release bundles are separate from the archive/download cache. Use `MULTI_PWSH_OFFLINE_CACHE` or `--offline-cache <path>` to read releases from a warmed bundle.

New scoped installs are metadata-backed. Older non-Windows filesystem-only installs that predate scoped metadata may need to be reinstalled or migrated before scoped `list` / `uninstall` can manage them.

CI cache example:

```powershell
$env:MULTI_PWSH_HOME = "$(Join-Path $HOME '.pwsh')"
$env:MULTI_PWSH_BIN_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'bin')"
$env:MULTI_PWSH_CACHE_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'cache')"
$env:MULTI_PWSH_VENV_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'venv')"
$env:MULTI_PWSH_CACHE_KEEP = "1"
multi-pwsh install 7.4.x
```
