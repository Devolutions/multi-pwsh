[CmdletBinding()]
param(
    [string] $PowerShellVersion = '7.4'
)

$ErrorActionPreference = 'Stop'
$env:PSMODULE_VENV_PATH = $PSScriptRoot

$commands = @(
    'hyperv_list_vms'
    'hyperv_get_vm_info'
    'hyperv_start_vm'
    'hyperv_stop_vm'
    'hyperv_reset_vm'
    'hyperv_checkpoint_create'
    'hyperv_checkpoint_list'
    'hyperv_checkpoint_restore'
    'hyperv_checkpoint_remove'
    'hyperv_configure_kdnet'
    'hyperv_configure_kdcom'
    'hyperv_guest_run'
    'hyperv_guest_run_ps'
    'hyperv_guest_put'
    'hyperv_guest_get'
    'hyperv_guest_read_file'
    'hyperv_guest_list_dir'
    'hyperv_victim_run'
    'hyperv_victim_run_ps'
)

& multi-pwsh host $PowerShellVersion -mcp -McpCommands $commands
exit $LASTEXITCODE
