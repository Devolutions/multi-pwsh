# Hyper-V MCP server example

This Windows-only example recreates the 19-tool surface of
[`originsec/hyperv-mcp`](https://github.com/originsec/hyperv-mcp) with a
PowerShell module hosted directly by `multi-pwsh`. It supports VM lifecycle,
checkpoints, KDNET/KDCOM setup, PowerShell Direct guest execution, file
transfer, and a separate unprivileged guest identity.

The example is behaviorally similar, not wire-compatible. `multi-pwsh`
prefixes exposed tools with `powershell_`, so `hyperv_list_vms` is advertised
as `powershell_hyperv_list_vms`. Tool results are compact JSON inside the text
output returned by the current MCP bridge.

## Requirements

- Windows 10/11 Pro or Enterprise, or Windows Server, with Hyper-V enabled.
- The Hyper-V PowerShell module available to PowerShell 7.4.
- `multi-pwsh` installed and `multi-pwsh install 7.4` completed.
- A host identity with the required Hyper-V permissions. Prefer membership in
  `Hyper-V Administrators` over running the entire MCP client elevated.
- Windows guests with PowerShell Direct support for guest operations.

Verify the host:

```powershell
Get-Module -ListAvailable Hyper-V
Get-VM
multi-pwsh host 7.4 -NoProfile -Command 'Get-Module -ListAvailable Hyper-V'
```

## Credentials

Guest operations resolve credentials from explicit `username` and `password`
tool arguments first, then:

```text
HYPERV_GUEST_USERNAME
HYPERV_GUEST_PASSWORD
```

The `hyperv_victim_*` tools only use:

```text
HYPERV_GUEST_VICTIM_USERNAME
HYPERV_GUEST_VICTIM_PASSWORD
```

Set these variables in the MCP child-process environment or a user-scoped
secret launcher. Do not commit credentials to an MCP configuration. Explicit
credential arguments can be retained in model context, client logs, or
telemetry, so environment-based credential injection is preferred.

## Start the server

From the repository root:

```powershell
.\examples\hyperv-mcp\Start-HyperVMcpServer.ps1
```

The launcher sets `PSMODULE_VENV_PATH` to this example, allowing the hosted
runspace to auto-load `Modules\HyperVMcp`, and starts a stdio MCP server. To
select a different installed line:

```powershell
.\examples\hyperv-mcp\Start-HyperVMcpServer.ps1 -PowerShellVersion 7.5
```

An MCP client configuration can launch the script directly:

```json
{
  "mcpServers": {
    "hyperv": {
      "command": "pwsh",
      "args": [
        "-NoLogo",
        "-NoProfile",
        "-File",
        "C:\\src\\multi-pwsh\\examples\\hyperv-mcp\\Start-HyperVMcpServer.ps1"
      ],
      "env": {
        "HYPERV_GUEST_USERNAME": "Administrator",
        "HYPERV_GUEST_PASSWORD": "<inject-with-your-secret-manager>",
        "HYPERV_GUEST_VICTIM_USERNAME": "Victim",
        "HYPERV_GUEST_VICTIM_PASSWORD": "<inject-with-your-secret-manager>"
      }
    }
  }
}
```

## Tool mapping

| Tool | Purpose |
|---|---|
| `powershell_hyperv_list_vms` | List VMs and current state |
| `powershell_hyperv_get_vm_info` | Get detailed VM configuration |
| `powershell_hyperv_start_vm` | Start a VM |
| `powershell_hyperv_stop_vm` | Gracefully stop, save, or turn off a VM |
| `powershell_hyperv_reset_vm` | Hard-reset a VM |
| `powershell_hyperv_checkpoint_create` | Create a checkpoint |
| `powershell_hyperv_checkpoint_list` | List checkpoints |
| `powershell_hyperv_checkpoint_restore` | Restore a checkpoint |
| `powershell_hyperv_checkpoint_remove` | Delete a checkpoint |
| `powershell_hyperv_configure_kdnet` | Configure KDNET in a guest |
| `powershell_hyperv_configure_kdcom` | Configure named-pipe serial debugging |
| `powershell_hyperv_guest_run` | Run a guest executable |
| `powershell_hyperv_guest_run_ps` | Run guest PowerShell |
| `powershell_hyperv_guest_put` | Copy a host file into a guest |
| `powershell_hyperv_guest_get` | Copy a guest file to the host |
| `powershell_hyperv_guest_read_file` | Read a bounded guest file as base64 |
| `powershell_hyperv_guest_list_dir` | List a guest directory |
| `powershell_hyperv_victim_run` | Run a guest executable as the victim identity |
| `powershell_hyperv_victim_run_ps` | Run guest PowerShell as the victim identity |

Start with `powershell_hyperv_list_vms`, then inspect a VM with
`powershell_hyperv_get_vm_info`.

## Safety

This is a privileged local administration server. The exposed tools can hard
power off VMs, discard checkpoints, alter guest boot configuration, execute
arbitrary guest code, and copy files across the host/guest boundary.

- Connect it only to trusted local MCP clients.
- Use a dedicated Hyper-V host and disposable guests.
- Keep human approval enabled for destructive and arbitrary-execution tools.
- Use least-privilege host and guest identities.
- Treat checkpoint restore/removal and `turnoff`/reset as destructive.
- `elevated=true` uses `Start-Process -Verb RunAs` with
  `-ExecutionPolicy Bypass`, matching the reference behavior. It requires the
  built-in Administrator account, disabled UAC, or configured auto-elevation.
