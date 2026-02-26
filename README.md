# pwsh-host-rs

Rust PowerShell hosting library that loads .NET delegates and drives `System.Management.Automation.PowerShell` through unmanaged entry points.

## Use from GitHub Releases

This repository publishes two user-facing binaries on Releases:

- `multi-pwsh`: install and manage side-by-side PowerShell lines
- `pwsh-host`: run PowerShell commands through the native host shim

### 1) Bootstrap `multi-pwsh`

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

### 2) Install PowerShell 7.4 and 7.5 side-by-side

```powershell
multi-pwsh install 7.4
multi-pwsh install 7.5
```

Verify aliases:

```powershell
pwsh-7.4 -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'
pwsh-7.5 -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'
```

Manage installed lines:

```powershell
multi-pwsh update 7.4
multi-pwsh update 7.5
multi-pwsh list
multi-pwsh doctor --repair-aliases
```

When installed via bootstrap scripts, `~/.pwsh/bin` is added to PATH automatically if needed.

### 3) Download `pwsh-host` from Releases

Download the `pwsh-host-<os>-<arch>.zip` artifact for your platform from:

- https://github.com/Devolutions/pwsh-host-rs/releases

Current artifact names:

- `pwsh-host-linux-x64.zip`
- `pwsh-host-linux-arm64.zip`
- `pwsh-host-macos-x64.zip`
- `pwsh-host-macos-arm64.zip`
- `pwsh-host-windows-x64.zip`
- `pwsh-host-windows-arm64.zip`

Extract and run:

```powershell
./pwsh-host -NoLogo -NoProfile -Command "$PSVersionTable.PSVersion"
```

On Windows, the binary name is `pwsh-host.exe`.

### 4) `-NamedPipeCommand` (Windows)

`pwsh-host` supports `-NamedPipeCommand <pipeName>` to read command text from a named pipe and forward it as an encoded PowerShell command.

Example invocation:

```powershell
./pwsh-host.exe -NoLogo -NoProfile -NonInteractive -NamedPipeCommand <pipeName>
```
