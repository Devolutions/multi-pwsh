# MCP host mode

`multi-pwsh` can expose selected PowerShell commands as Model Context Protocol (MCP) tools over stdio while running them inside a specific installed PowerShell version. Use this when an MCP client needs predictable PowerShell version selection, native host startup behavior, and optional module venv isolation.

## Start a server

```powershell
multi-pwsh install 7.4
multi-pwsh host 7.4 -mcp -McpCommands Get-Process Get-Service
```

`-McpCommands` accepts one or more command names after the flag. Values may also be comma- or semicolon-separated:

```powershell
multi-pwsh host stable -mcp -McpCommands 'Get-ChildItem,Get-Content'
multi-pwsh host pwsh-7.4 -mcp -McpCommands 'Get-Process;Get-Service'
```

MCP mode is part of host mode, so the selector can be any selector supported by `multi-pwsh host`: an exact version, major version, major/minor line, or managed alias such as `pwsh`, `pwsh-preview`, `pwsh-lts`, or `pwsh-7.4`.

An MCP client configuration typically launches `multi-pwsh` as a stdio server:

```json
{
  "command": "multi-pwsh",
  "args": ["host", "7.4", "-mcp", "-McpCommands", "Get-Process", "Get-Service"]
}
```

## Tool shape

At startup, the bridge resolves each listed command with `Get-Command` in the selected PowerShell runspace and builds an MCP tool from the command metadata.

- Tool names are prefixed with `powershell_` and normalized to lowercase ASCII with separators converted to `_`; for example, `Get-Process` becomes `powershell_get_process`.
- Tool descriptions use the command help synopsis when available, then fall back to the command definition.
- Mandatory PowerShell parameters become required JSON properties.
- Switch parameters become booleans. Integer, number, array, object, and string parameters are mapped to the closest JSON schema type.
- Commands that normalize to the same MCP tool name are rejected at startup.

Tool calls splat the JSON arguments into the PowerShell command and return the command output after `Out-String`. `null` values, empty arrays, and `false` switch values are omitted before invocation so PowerShell defaults still apply.

## Venv support

`-venv <name>` and `-VirtualEnvironment <name>` work in MCP mode:

```powershell
multi-pwsh venv create graph
multi-pwsh host 7.4 -mcp -McpCommands Get-Module -venv graph
```

The selected venv changes module discovery for both command metadata discovery and later tool calls. Path-based venv selection through `PSMODULE_VENV_PATH` is also honored by the native host path.

## Examples

- [Hyper-V management and guest execution](../examples/hyperv-mcp/README.md)

## Limits and safety

- MCP mode exposes only commands named in `-McpCommands`; it is not an interactive shell.
- Extra `pwsh` arguments are rejected in MCP mode. Choose commands with `-McpCommands` and use MCP tool arguments for command parameters.
- The transport is stdio only.
- Tool output is text, not structured PowerShell objects.
- Tool calls run with the current user's privileges in the selected hosted PowerShell version. Expose only trusted commands to trusted MCP clients.
