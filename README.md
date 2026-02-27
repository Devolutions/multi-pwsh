# pwsh-host-rs

Rust PowerShell hosting library that loads .NET delegates and drives `System.Management.Automation.PowerShell` through unmanaged entry points.

## multi-pwsh

Install and manage side-by-side PowerShell versions from GitHub Releases.

![multi-pwsh](docs/images/multi-pwsh.png)

### Bootstrap

Latest release bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/install-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/install-multi-pwsh.ps1 | iex
```

Install a specific tag (example `v0.5.0`):

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/install-multi-pwsh.sh | bash -s -- v0.5.0
```

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/install-multi-pwsh.ps1))) -Version v0.5.0
```

Uninstall bootstrap scripts:

```bash
curl -fsSL https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/uninstall-multi-pwsh.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/Devolutions/pwsh-host-rs/refs/heads/master/tools/uninstall-multi-pwsh.ps1 | iex
```

### Install and verify aliases

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

### Manage installed lines

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
multi-pwsh doctor --repair-aliases
```

`multi-pwsh` usage reference:

```text
multi-pwsh install <version|major|major.minor|major.minor.x> [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease]
multi-pwsh update <major.minor> [--arch <auto|x64|x86|arm64|arm32>] [--include-prerelease]
multi-pwsh uninstall <version> [--force]
multi-pwsh list [--available] [--include-prerelease]
multi-pwsh alias set <major.minor> <version|latest>
multi-pwsh alias unset <major.minor>
multi-pwsh doctor --repair-aliases
```

Selector behavior:

- `7` installs the latest available 7.x release for your platform.
- `7.4` installs the latest available 7.4.x release for your platform.
- `7.4.x` installs all available releases in that line for your platform.
- `7.4.11` installs that exact version.

`multi-pwsh install 7.4.x` installs every available patch release in that line for your current platform and creates per-version aliases such as `pwsh-7.4.11`.
The `pwsh-7.4` alias tracks latest by default; pin it with `multi-pwsh alias set 7.4 7.4.11` and unpin with `multi-pwsh alias unset 7.4`.
If a pinned target version is not installed, the pin remains in metadata and the alias stays unresolved until you install that version or unpin.

Download cache behavior can be controlled with environment variables:

- `MULTI_PWSH_CACHE_DIR`: override archive cache directory (default: `~/.pwsh/cache`).
- `MULTI_PWSH_CACHE_KEEP`: keep downloaded archives after extraction when set to a truthy value (`1`, `true`, `yes`, or `on`).

CI cache example:

```powershell
$env:MULTI_PWSH_CACHE_DIR = "$(Join-Path $HOME '.pwsh\cache')"
$env:MULTI_PWSH_CACHE_KEEP = "1"
multi-pwsh install 7.4.x
```

When installed via bootstrap scripts, `~/.pwsh/bin` is added to PATH automatically if needed.

## pwsh-host

Run PowerShell commands through a native host shim built in Rust.

This project uses .NET native hosting (hostfxr delegates) to call into `System.Management.Automation.PowerShell` from native code.
For background on this approach, see [dotnet/runtime#46652: Native Host using existing PowerShell 7 installation](https://github.com/dotnet/runtime/issues/46652).

### Download from Releases

Download the `pwsh-host-<os>-<arch>.zip` artifact for your platform from:

- https://github.com/Devolutions/pwsh-host-rs/releases

Current artifact names:

- `pwsh-host-linux-x64.zip`
- `pwsh-host-linux-arm64.zip`
- `pwsh-host-macos-x64.zip`
- `pwsh-host-macos-arm64.zip`
- `pwsh-host-windows-x64.zip`
- `pwsh-host-windows-arm64.zip`

### Run example

Extract and run a command:

```powershell
./pwsh-host -NoLogo -NoProfile -Command "$PSVersionTable.PSVersion"
```

Another example:

```powershell
./pwsh-host -NoLogo -NoProfile -Command "Get-Process pwsh | Select-Object -First 1 Name,Id"
```

On Windows, the binary name is `pwsh-host.exe`.

### `-NamedPipeCommand` (Windows)

`pwsh-host` supports `-NamedPipeCommand <pipeName>` to read command text from a named pipe and forward it as an encoded PowerShell command.

Example invocation:

```powershell
./pwsh-host.exe -NoLogo -NoProfile -NonInteractive -NamedPipeCommand <pipeName>
```
