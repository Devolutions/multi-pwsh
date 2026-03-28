# multi-pwsh

Install and manage side-by-side PowerShell versions with aliases and native hosting.

![multi-pwsh](docs/images/multi-pwsh.png)

## Bootstrap

Latest release bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/install-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/install-multi-pwsh.ps1 | iex
```

Install a specific tag (example `v0.6.0`):

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/install-multi-pwsh.sh | bash -s -- v0.6.0
```

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/install-multi-pwsh.ps1))) -Version v0.6.0
```

Uninstall bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/uninstall-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/Devolutions/multi-pwsh/refs/heads/master/tools/uninstall-multi-pwsh.ps1 | iex
```

## Install and verify aliases

```powershell
multi-pwsh install 7.4
multi-pwsh install 7.5
```

Verify aliases:

```powershell
pwsh-7 --version
pwsh-7.4 --version
pwsh-7.5 --version
```

## Scoped installs

`multi-pwsh install`, `update`, `uninstall`, and `list` now support `--scope <user|machine>` across Windows, macOS, and Linux.

That means:

- extracted versions stay side-by-side under the selected install root
- aliases continue to live in one stable bin directory
- PATH only needs one entry per scope
- `user` is the default scope when `--scope` is omitted

Platform behavior:

- Windows uses the GitHub ZIP archives with MSI-like install roots and selected installer-style integrations that still make sense for archive installs.
- macOS `machine` installs use the official `.tar.gz` archives under `/usr/local/microsoft/powershell` with aliases published to `/usr/local/bin`.
- Linux `machine` installs use the official `.tar.gz` archives under `/opt/microsoft/powershell` with aliases published to `/usr/local/bin`.
- Unix `machine` installs expect you to provide elevation yourself; `multi-pwsh` does not invoke `sudo`.

Examples:

```powershell
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
- `--add-path` / `--no-add-path`

The Windows-only integration flags above currently return an error on macOS/Linux.

## Manage installed lines

```powershell
multi-pwsh install 7.4.x
multi-pwsh update 7.4
multi-pwsh update 7.5
multi-pwsh list
multi-pwsh list --available
multi-pwsh list --available --include-prerelease
multi-pwsh install 7.6 --include-prerelease
multi-pwsh install 7.6-preview6
multi-pwsh install 7.6-rc1
multi-pwsh install 7.6.0-rc.1
multi-pwsh update 7.6 --include-prerelease
multi-pwsh alias set 7.4 7.4.11
multi-pwsh alias unset 7.4
multi-pwsh venv create msgraph
multi-pwsh venv export msgraph msgraph.zip
multi-pwsh venv import msgraph-copy msgraph.zip
multi-pwsh venv delete msgraph
multi-pwsh venv list
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "$env:PSModulePath"
multi-pwsh doctor --repair-aliases
```

`multi-pwsh` usage reference:

```text
multi-pwsh install <version|major|major.minor|major.minor.x> [--scope <user|machine>] [--root <path>] [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease] [--add-path|--no-add-path] [--register-manifest|--no-register-manifest] [--enable-psremoting] [--disable-telemetry] [--add-explorer-context-menu] [--add-file-context-menu]
multi-pwsh update <major.minor> [--scope <user|machine>] [--root <path>] [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease] [--add-path|--no-add-path] [--register-manifest|--no-register-manifest] [--enable-psremoting] [--disable-telemetry] [--add-explorer-context-menu] [--add-file-context-menu]
multi-pwsh uninstall <version> [--scope <user|machine>] [--root <path>] [--force]
multi-pwsh list [--scope <user|machine|all>] [--root <path>] [--available] [--include-prerelease]
multi-pwsh venv create <name>
multi-pwsh venv delete <name>
multi-pwsh venv export <name> <archive.zip>
multi-pwsh venv import <name> <archive.zip>
multi-pwsh venv list
multi-pwsh alias set <major.minor> <version|latest>
multi-pwsh alias unset <major.minor>
multi-pwsh host <version|major|major.minor|pwsh-alias> [-VirtualEnvironment <name>|-venv <name>] [pwsh arguments...]
multi-pwsh doctor --repair-aliases
```

The Windows integration flags in the `install` and `update` forms are limited to archive-friendly behaviors; on macOS/Linux, use `--scope`, `--root`, `--arch`, `--include-prerelease`, and `--add-path` controls. Legacy scope aliases such as `current-user` and `all-users` are still accepted for compatibility.

### Venv cmdlet matrix tests (Pester)

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
- `Install-PSResource` uses `-TrustRepository -Quiet` and `Install-Module` uses `-Force -AcceptLicense -Confirm:$false` to run non-interactively.
- The install checks use `Yayaml` to keep downloads and execution lightweight.

Notes:

- The runner creates a temporary venv per alias and deletes it by default.
- Use `-KeepVenv` to keep those venvs for troubleshooting.
- The runner stops on the first failed alias by default.
- Use `-ContinueOnFailure` to keep running remaining aliases after a failure.
- Pester must be available in the host PowerShell session.

Selector behavior:

- `7` installs the latest available 7.x release for your platform.
- `7.4` installs the latest available 7.4.x release for your platform.
- `7.4.x` installs all available releases in that line for your platform.
- `7.4.11` installs that exact version.

`multi-pwsh install 7.4.x` installs every available patch release in that line for your current platform and creates per-version aliases such as `pwsh-7.4.11`.
The `pwsh-7.4` alias tracks latest by default; pin it with `multi-pwsh alias set 7.4 7.4.11` and unpin with `multi-pwsh alias unset 7.4`.
If a pinned target version is not installed, the pin remains in metadata and the alias stays unresolved until you install that version or unpin.

Native host mode:

- `multi-pwsh host <selector> ...` runs PowerShell through native hosting (`pwsh-host` crate) instead of launching a `pwsh` subprocess.
- `<selector>` supports `7`, `7.4`, `7.4.13`, or alias-form selectors such as `pwsh-7.4`.
- `-VirtualEnvironment <name>` and `-venv <name>` are consumed by `multi-pwsh` before handing control to PowerShell and set `PSModulePath` to the selected venv root for that launch.
- `PSMODULE_VENV_PATH` can also be used as an explicit path-based venv selector for hosted launches. If it is already set in the environment, `multi-pwsh host` treats it as an intentional venv opt-in.
- Alias lifecycle now maintains native host shims as hard links to `multi-pwsh` automatically during install/update/doctor alias repair.
- On Windows, alias command paths are `pwsh-*.exe` host shims in `MULTI_PWSH_BIN_DIR` (default: `~/.pwsh/bin`).
- On Linux/macOS, alias command paths (`pwsh-*`) are hard links to `multi-pwsh`.
- `multi-pwsh doctor --repair-aliases` performs a shim health check and re-links broken hard links automatically.
- You can still manually copy/rename `multi-pwsh.exe` under `MULTI_PWSH_BIN_DIR` (default: `~/.pwsh/bin`) to an alias-like name (for example `pwsh-7.4.exe`); it automatically enters host mode and resolves the target installation from that alias name.
- `-NamedPipeCommand <pipeName>` is supported in host mode (Windows only), matching `pwsh-host` behavior.

### Virtual environments

`multi-pwsh` virtual environments provide isolated PowerShell module roots. They are conceptually similar to Python virtual environments, but in this first version the isolation is implemented by selecting a venv-specific `PSModulePath` root for hosted launches.

By default, venvs live under `~/.pwsh/venv/<name>`. If `MULTI_PWSH_VENV_DIR` is set, they live under that directory instead.

Available commands:

- `multi-pwsh venv create <name>` creates a named venv.
- `multi-pwsh venv delete <name>` removes a named venv.
- `multi-pwsh venv export <name> <archive.zip>` exports a named venv to a zip archive.
- `multi-pwsh venv import <name> <archive.zip>` imports a named venv from a zip archive.
- `multi-pwsh venv list` shows the configured venv root and all known venvs.

#### Create and use a venv

Create a venv and launch a hosted PowerShell session that uses it:

```powershell
multi-pwsh venv create msgraph
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile
```

You can verify which module root is being used:

```powershell
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "$env:PSModulePath"
```

Both `-venv <name>` and `-VirtualEnvironment <name>` are supported.

You can also opt into a venv by path with `PSMODULE_VENV_PATH`:

```powershell
$env:PSMODULE_VENV_PATH = Join-Path $HOME ".pwsh/venv/msgraph"
multi-pwsh host 7.4 -NoLogo -NoProfile
```

`-venv <name>` and `-VirtualEnvironment <name>` accept a venv name and resolve it to a path before launch. `PSMODULE_VENV_PATH` is the lower-level path form of the same idea and is useful when a parent PowerShell session already knows which venv path should flow to child hosted sessions.

If both a venv flag and `PSMODULE_VENV_PATH` are present, the flag wins for that launch because `multi-pwsh` resolves the named venv and sets the effective path explicitly. If neither is present, no venv-specific startup-hook behavior is enabled.

#### Populate a venv with modules

Venvs are module discovery roots, so modules should live directly under `<venv-root>/<ModuleName>`.

For the current implementation, the safest way to place modules into a venv is to save them directly into that venv root:

```powershell
$venvRoot = Join-Path $HOME ".pwsh/venv/msgraph"
Save-Module -Name Microsoft.Graph.Authentication -Repository PSGallery -Path $venvRoot -Force
Save-Module -Name Microsoft.Graph.Users -Repository PSGallery -Path $venvRoot -Force
```

Then use the venv when launching PowerShell:

```powershell
multi-pwsh host 7.4 -venv msgraph -NoLogo -NoProfile -Command "Get-Module -ListAvailable Microsoft.Graph.Authentication"
```

#### Export and import a venv

You can package a venv as a zip archive and recreate it elsewhere:

```powershell
multi-pwsh venv export msgraph msgraph.zip
multi-pwsh venv import msgraph-copy msgraph.zip
multi-pwsh host 7.4 -venv msgraph-copy -NoLogo -NoProfile
```

Import is intentionally conservative: importing into an existing destination venv is rejected instead of merging archive contents.

#### Current behavior and limitations

- Venv selection changes module discovery and import precedence for hosted launches.
- In this first version, `Install-Module` is not automatically redirected into the venv just because `-venv` is used.
- PowerShell may still include some built-in or default module paths in the effective `PSModulePath`; the venv is intended to be the selected module root, not a perfect process-level sandbox.
- The venv feature currently applies to `multi-pwsh host ...` and implicit host shims such as `pwsh-7.4.exe`, not to arbitrary external `pwsh` processes.

Managed paths can be controlled with environment variables:

- `MULTI_PWSH_HOME`: override the multi-pwsh home directory (default: `~/.pwsh`). Extracted PowerShell versions are stored under `MULTI_PWSH_HOME/multi`, virtual environments are stored under `MULTI_PWSH_HOME/venv` unless `MULTI_PWSH_VENV_DIR` is set, and alias metadata is stored in `MULTI_PWSH_HOME/aliases.json`.
- `MULTI_PWSH_BIN_DIR`: override the shim and launcher directory (default: `MULTI_PWSH_HOME/bin`).
- `MULTI_PWSH_CACHE_DIR`: override archive cache directory (default: `MULTI_PWSH_HOME/cache`).
- `MULTI_PWSH_VENV_DIR`: override the virtual-environment root directory (default: `MULTI_PWSH_HOME/venv`).
- `MULTI_PWSH_CACHE_KEEP`: keep downloaded archives after extraction when set to a truthy value (`1`, `true`, `yes`, or `on`).

CI cache example:

```powershell
$env:MULTI_PWSH_HOME = "$(Join-Path $HOME '.pwsh')"
$env:MULTI_PWSH_BIN_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'bin')"
$env:MULTI_PWSH_CACHE_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'cache')"
$env:MULTI_PWSH_VENV_DIR = "$(Join-Path $env:MULTI_PWSH_HOME 'venv')"
$env:MULTI_PWSH_CACHE_KEEP = "1"
multi-pwsh install 7.4.x
```

When installed via bootstrap scripts, `MULTI_PWSH_BIN_DIR` (default: `~/.pwsh/bin`) is added to PATH automatically if needed.

