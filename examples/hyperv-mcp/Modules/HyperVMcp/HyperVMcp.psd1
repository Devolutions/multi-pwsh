@{
    RootModule = 'HyperVMcp.psm1'
    ModuleVersion = '1.0.0'
    GUID = '9f02a17f-96f0-459c-abcf-f9c9a3b47456'
    Author = 'Devolutions'
    Description = 'Hyper-V MCP example commands for multi-pwsh.'
    PowerShellVersion = '7.4'
    FunctionsToExport = @(
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
    CmdletsToExport = @()
    VariablesToExport = @()
    AliasesToExport = @()
    PrivateData = @{
        PSData = @{
            Tags = @('Hyper-V', 'MCP', 'multi-pwsh')
            ProjectUri = 'https://github.com/Devolutions/multi-pwsh'
        }
    }
}
