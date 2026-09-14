# multi-pwsh

Install and manage side-by-side PowerShell versions with aliases, offline bundles, native hosting, module venvs, and MCP command exposure.

![multi-pwsh](docs/images/multi-pwsh.png)

## Bootstrap

Latest release bootstrap scripts:

```bash
curl -fsSL https://github.com/Devolutions/multi-pwsh/releases/latest/download/install-multi-pwsh.sh | bash
```

```powershell
irm https://github.com/Devolutions/multi-pwsh/releases/latest/download/install-multi-pwsh.ps1 | iex
```

Install a specific tag (example `v0.16.0`):

```bash
curl -fsSL https://github.com/Devolutions/multi-pwsh/releases/download/v0.16.0/install-multi-pwsh.sh | bash -s -- v0.16.0
```

```powershell
& ([scriptblock]::Create((irm https://github.com/Devolutions/multi-pwsh/releases/download/v0.16.0/install-multi-pwsh.ps1))) -Version v0.16.0
```

Uninstall bootstrap scripts:

```bash
curl -fsSL https://github.com/Devolutions/multi-pwsh/releases/latest/download/uninstall-multi-pwsh.sh | bash
```

```powershell
irm https://github.com/Devolutions/multi-pwsh/releases/latest/download/uninstall-multi-pwsh.ps1 | iex
```

## Install and verify aliases

```powershell
multi-pwsh install stable
multi-pwsh install preview
multi-pwsh install lts
multi-pwsh install 7.4
```

Verify aliases:

```powershell
pwsh --version
pwsh-preview --version
pwsh-lts --version
pwsh-7.4 --version
```

## Offline and network-share installs

GitHub remains the default source, but you can prefetch release artifacts on a connected machine and use them from a disconnected machine.

On a connected machine, warm an offline release bundle:

```powershell
multi-pwsh cache warm stable --os windows --arch x64 --output \\fileserver\multi-pwsh-cache
multi-pwsh cache warm 7.4.x --os all --arch all --output \\fileserver\multi-pwsh-cache
```

The bundle includes PowerShell archives, PowerShell `hashes.sha256`, a relocatable manifest, and the current `multi-pwsh` release archive/checksums for the requested platform targets. Copy that directory locally or expose it as a read-only network share, then use it offline:

```powershell
$env:MULTI_PWSH_OFFLINE_CACHE = '\\fileserver\multi-pwsh-cache'
multi-pwsh list --available
multi-pwsh install stable
multi-pwsh update 7.4
```

You can also pass `--offline-cache <path>` to `install`, `update`, and `list --available`. Offline mode uses only the local manifest/artifacts and fails instead of falling back to GitHub when something is missing.

`MULTI_PWSH_OFFLINE_CACHE` selects a warmed offline release bundle. `MULTI_PWSH_CACHE_DIR` is separate: it is the user-scope archive/download cache and the default output location for `cache warm` when `--output` is omitted. Empty or whitespace-only path environment variable values are treated as unset.

Disconnected bootstrap uses the same bundle:

```powershell
.\install-multi-pwsh.ps1 -OfflineCache \\fileserver\multi-pwsh-cache -Version latest
```

```bash
./install-multi-pwsh.sh --offline-cache /mnt/share/multi-pwsh-cache --version latest
```

Rerun the bootstrap script against a newer warmed bundle to update `multi-pwsh` itself. Archives are verified against the mirrored checksum files before installation.

## More docs

- [Native host mode and virtual environments](docs/host-and-venv.md)
- [In-process PowerShell FFI experiment](docs/in-process-ffi.md)
- [MCP host mode](docs/mcp.md)
- [Hyper-V MCP server example](examples/hyperv-mcp/README.md)
- [Feature matrix and roadmap](docs/feature-matrix.md)
- [Testing guide](docs/testing.md)
- [Release process](RELEASE.md)

## Scoped installs

`multi-pwsh install`, `update`, `uninstall`, and `list` now support `--scope <user|machine>` across Windows, macOS, and Linux.

That means:

- extracted versions stay side-by-side under the selected install root
- aliases continue to live in one stable bin directory
- PATH only needs one entry per scope
- `user` is the default scope when `--scope` is omitted
- machine installs and removals require explicit `--scope machine`
- `MULTI_PWSH_*` path environment variables affect only the default `user` layout
- `machine` scope uses fixed platform machine paths and does not read `MULTI_PWSH_*` path overrides
- `--root` is an explicit install-root override, requires `--scope <user|machine>`, and does not mix in `MULTI_PWSH_*` child-directory overrides

Platform behavior:

- Windows uses the GitHub ZIP archives with MSI-like install roots and selected installer-style integrations that still make sense for archive installs.
- `user` installs default to `~/.pwsh` on every platform, with aliases in `~/.pwsh/bin`.
- Windows `machine` installs default to `%ProgramFiles%\PowerShell` with aliases in `%ProgramFiles%\PowerShell\bin`.
- macOS `machine` installs use the official `.tar.gz` archives under `/usr/local/microsoft/powershell` with aliases published to `/usr/local/bin`.
- Linux `machine` installs use the official `.tar.gz` archives under `/opt/microsoft/powershell` with aliases published to `/usr/local/bin`.
- Windows `machine` installs may require elevation for `%ProgramFiles%`, registry integrations, and Machine `PATH` updates; use `--no-add-path` if you want to skip the PATH update.
- Unix `machine` installs expect you to provide elevation yourself; `multi-pwsh` does not invoke `sudo`.
- On Windows, `--add-path` / `--no-add-path` controls persistent User or Machine `PATH` updates. On macOS/Linux, `--add-path` is unsupported, `--no-add-path` is accepted as a no-op, and shell/profile PATH updates are manual.
- New scoped installs are metadata-backed. Older non-Windows filesystem-only installs that predate scoped metadata may need to be reinstalled or migrated before scoped `list` / `uninstall` can manage them.

Examples:

```powershell
multi-pwsh install stable
multi-pwsh install preview
multi-pwsh install lts
multi-pwsh install 7.4
multi-pwsh install 7.5 --scope machine --enable-psremoting --add-explorer-context-menu
multi-pwsh install 7.5 --scope machine
multi-pwsh list --scope all
multi-pwsh uninstall 7.4.13 --scope machine
```

Windows scoped-install flags mirror the most useful MSI-style options:

- `--add-path` / `--no-add-path`
- `--register-manifest` / `--no-register-manifest`
- `--enable-psremoting`
- `--disable-telemetry`
- `--add-explorer-context-menu`
- `--add-file-context-menu`
- `--scope <user|machine>`
- `--root <path>`

Microsoft Update registration is intentionally out of scope for archive installs at the moment, even on Windows.

On macOS and Linux, scoped installs support:

- `--scope <user|machine>`
- `--root <path>`
- `--arch <auto|x64|x86|arm64|arm32>`
- `--include-prerelease`
- `--no-add-path` as a cross-platform no-op
- shell/profile `PATH` updates are manual on Unix; `--add-path` is Windows-only

The Windows-only integration flags above currently return an error on macOS/Linux.

## Manage installed lines

### Channel installs and updates

```powershell
multi-pwsh install stable
multi-pwsh install preview
multi-pwsh install lts
multi-pwsh update stable
multi-pwsh update preview
multi-pwsh update lts
multi-pwsh list
multi-pwsh list --available
multi-pwsh list --available --include-prerelease
```

### Exact versions and lines

```powershell
multi-pwsh install 7.4.x
multi-pwsh install 7.6 --include-prerelease
multi-pwsh install 7.6-preview6
multi-pwsh install 7.6-rc1
multi-pwsh install 7.6.0-rc.1
multi-pwsh update 7.4
multi-pwsh update 7.5
multi-pwsh update 7.6 --include-prerelease
multi-pwsh uninstall 7.4.13
```

### Alias policy and host mode

```powershell
multi-pwsh alias set pwsh stable
multi-pwsh alias set pwsh lts
multi-pwsh alias set pwsh-preview preview
multi-pwsh alias set pwsh-lts lts
multi-pwsh alias set 7.4 7.4.11
multi-pwsh alias unset 7.4
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "$env:PSModulePath"
multi-pwsh doctor --repair-aliases
```

### Venv management

```powershell
multi-pwsh venv create msgraph
multi-pwsh venv export msgraph msgraph.zip
multi-pwsh venv import msgraph-copy msgraph.zip
multi-pwsh venv delete msgraph
multi-pwsh venv list
```

`multi-pwsh` usage reference:

```text
multi-pwsh --version
multi-pwsh -V
multi-pwsh version
multi-pwsh --help
multi-pwsh help [command]
multi-pwsh install <stable|preview|lts|version|major|major.minor|major.minor.x> [--scope <user|machine>] [--root <path>] [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease] [--offline-cache <path>] [--add-path|--no-add-path] [--register-manifest|--no-register-manifest] [--enable-psremoting] [--disable-telemetry] [--add-explorer-context-menu] [--add-file-context-menu]
multi-pwsh update <stable|preview|lts|major.minor> [--scope <user|machine>] [--root <path>] [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease] [--offline-cache <path>] [--add-path|--no-add-path] [--register-manifest|--no-register-manifest] [--enable-psremoting] [--disable-telemetry] [--add-explorer-context-menu] [--add-file-context-menu]
multi-pwsh cache warm <selector> [--os <windows|linux|macos|all>] [--arch <x64|x86|arm64|arm32|all>] [--include-prerelease] [--output <path>] [--product <powershell|multi-pwsh|all>]
multi-pwsh uninstall <version> [--scope <user|machine>] [--root <path>] [--force]
multi-pwsh list [--scope <user|machine|all>] [--root <path>] [--available] [--include-prerelease] [--offline-cache <path>]
multi-pwsh venv create <name>
multi-pwsh venv delete <name>
multi-pwsh venv export <name> <archive.zip>
multi-pwsh venv import <name> <archive.zip>
multi-pwsh venv list
multi-pwsh alias set <major.minor> <version|latest>
multi-pwsh alias set <pwsh|pwsh-preview|pwsh-lts> <stable|preview|lts|version>
multi-pwsh alias unset <major.minor|pwsh|pwsh-preview|pwsh-lts>
multi-pwsh host <version|major|major.minor|pwsh-alias> [-VirtualEnvironment <name>|-venv <name>] [pwsh arguments...]
multi-pwsh host <version|major|major.minor|pwsh-alias> -mcp -McpCommands <command> [command ...] [-VirtualEnvironment <name>|-venv <name>]
multi-pwsh doctor --repair-aliases
```

Use `multi-pwsh <command> --help` or `multi-pwsh help <command>` for focused command usage, for example `multi-pwsh install --help`.

The Windows integration flags in the `install` and `update` forms are limited to archive-friendly behaviors; on macOS/Linux, use `--scope`, `--root`, `--arch`, `--include-prerelease`, `--no-add-path`, and manual PATH management for the printed alias bin directory. Whenever `--root` is used with install, update, list, or uninstall, pass `--scope <user|machine>` as well.

`multi-pwsh package ...` remains available as an advanced compatibility command for the scoped install backend, but the top-level commands above are the primary interface.

## Selector behavior

- `stable` installs the latest GA/non-preview release for your platform and configures `pwsh` to follow the latest installed stable release.
- `preview` installs the latest prerelease for your platform and configures `pwsh-preview` to follow the latest installed preview release.
- `lts` installs the latest patch from the current LTS line for your platform and configures `pwsh-lts` to follow the latest installed LTS release.
- `7` installs the latest available 7.x release for your platform.
- `7.4` installs the latest available 7.4.x release for your platform.
- `7.4.x` installs all available releases in that line for your platform.
- `7.4.11` installs that exact version.

`multi-pwsh install 7.4.x` installs every available patch release in that line for your current platform and creates per-version aliases such as `pwsh-7.4.11`.

The `pwsh-7.4` alias tracks latest by default; pin it with `multi-pwsh alias set 7.4 7.4.11` and unpin with `multi-pwsh alias unset 7.4`.

If a pinned target version is not installed, the pin remains in metadata and the alias stays unresolved until you install that version or unpin.

The bare `pwsh` alias is a managed policy alias. Configure it with `multi-pwsh alias set pwsh stable`, `multi-pwsh alias set pwsh lts`, `multi-pwsh alias set pwsh preview`, or an exact version. `pwsh-preview` tracks preview by default, and `pwsh-lts` tracks LTS by default. Policy aliases resolve only to installed versions; install or update the desired channel before pointing an alias at it.

Use `multi-pwsh list` to inspect both resolved aliases and managed named alias policies. The policy section shows whether aliases such as `pwsh`, `pwsh-preview`, and `pwsh-lts` currently resolve to an installed version or remain configured but unresolved.

The current LTS line is encoded in the tool; at the moment that is `7.6`.

## Native host mode and virtual environments

- `multi-pwsh host <selector> ...` runs PowerShell through native hosting instead of launching a `pwsh` subprocess.
- Use `-venv <name>` or `-VirtualEnvironment <name>` to select a managed module root for hosted launches.
- Use `-mcp -McpCommands <command> [command ...]` to expose selected PowerShell commands as stdio MCP tools from the chosen hosted version.
- Use `multi-pwsh doctor --repair-aliases` to repair host shims and named aliases.

Advanced local replacement mode is also supported: if `multi-pwsh` is renamed to `pwsh`/`pwsh.exe`, it runs a local SDK payload directly instead of resolving the managed `pwsh` alias or searching `PATH`. The payload can be adjacent to the executable, or shared from the publish root when the executable is under `runtimes/<rid>/native/`.

See [docs/host-and-venv.md](docs/host-and-venv.md) for host shims, local replacement mode, venv layout, import/export, managed paths, and current limitations. See [docs/mcp.md](docs/mcp.md) for MCP host mode.

## CLI NuGet package and AppHost mode

`Devolutions.MultiPwsh.Cli` packages RID-specific `multi-pwsh` binaries under `runtimes/<rid>/native/` for .NET projects. It also exposes neutral MSBuild metadata for downstream packages that need to consume the same binaries as PowerShell apphosts. The package supplies only the native launcher; downstream packages must provide their own `pwsh.dll` and `pwsh.runtimeconfig.json` either beside the renamed launcher or at the shared publish root above `runtimes/<rid>/native/`.

Package authors can consume the launchers privately and map them into their own package layout:

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.16.0" PrivateAssets="all" />
</ItemGroup>

<PropertyGroup>
  <MultiPwshRuntimeNativeContentCopyEnabled>false</MultiPwshRuntimeNativeContentCopyEnabled>
</PropertyGroup>

<Target Name="StagePowerShellAppHosts" DependsOnTargets="ResolveMultiPwshAppHostAssets">
  <ItemGroup>
    <PowerShellSdkAppHost Include="@(MultiPwshAppHostAsset)">
      <PackagePath>tools/apphost/%(RuntimeIdentifier)/%(AppHostFileName)</PackagePath>
    </PowerShellSdkAppHost>
  </ItemGroup>
</Target>
```

`@(MultiPwshAppHostAsset)` includes the source path as the item identity plus metadata such as `RuntimeIdentifier`, `NativeFileName`, `AppHostFileName`, `PackageRelativePath`, and `PackageId`. The package also sets `MultiPwshAppHostSupportedRuntimeIdentifiers` and `MultiPwshAppHostManifestPath`.

For a simple single-RID project, AppHost mode can copy the selected binary directly to build and publish output. AppHost mode is inert by default.

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.16.0" PrivateAssets="all" />
</ItemGroup>

<PropertyGroup>
  <MultiPwshAppHostEnabled>true</MultiPwshAppHostEnabled>
  <MultiPwshAppHostOutputBaseName>pwsh</MultiPwshAppHostOutputBaseName>
</PropertyGroup>
```

When enabled, the package resolves the RID from `MultiPwshAppHostRuntimeIdentifier`, `PowerShellSDKAppHostRuntimeIdentifier`, `RuntimeIdentifier`, then `NETCoreSdkRuntimeIdentifier`, copies the selected binary to build and publish output, and appends `.exe` for Windows RIDs. Set `MultiPwshAppHostOutputName` for a full explicit file name, or set `MultiPwshAppHostCopyToOutput` / `MultiPwshAppHostCopyToPublish` to `false` and consume `MultiPwshAppHostResolvedNativeBinary` / `@(MultiPwshAppHostNativeBinary)` from custom targets.

## Testing

- Scoped install smoke tests: `pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-ScopedInstallSmokeTest.ps1`
- Venv matrix tests: `pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-VenvTestMatrix.ps1`
- CLI NuGet AppHost smoke test: `pwsh -NoLogo -NoProfile -NonInteractive -File .\tests\Invoke-AppHostNuGetPackageSmokeTest.ps1`

See [docs/testing.md](docs/testing.md) for online test mode, alias-targeted runs, and troubleshooting flags.
