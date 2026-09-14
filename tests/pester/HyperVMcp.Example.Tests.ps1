Describe 'Hyper-V MCP example module' {
    BeforeAll {
        $expectedCommands = @(
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
        $modulePath = Join-Path $PSScriptRoot '..\..\examples\hyperv-mcp\Modules\HyperVMcp\HyperVMcp.psd1'
        Import-Module $modulePath -Force -DisableNameChecking
    }

    AfterAll {
        Remove-Module HyperVMcp -Force -ErrorAction SilentlyContinue
    }

    It 'exports the complete reference tool surface' {
        $actualCommands = @(
            Get-Command -Module HyperVMcp -CommandType Function |
                Select-Object -ExpandProperty Name
        )

        $actualCommands.Count | Should -Be 19
        $actualCommands | Sort-Object | Should -Be ($expectedCommands | Sort-Object)
    }

    It 'keeps reference defaults in command metadata' {
        (Get-Command hyperv_stop_vm).Parameters.method.Attributes.ValidValues |
            Should -Be @('shutdown', 'save', 'turnoff')
        (Get-Command hyperv_configure_kdnet).Parameters.port.Attributes.MinRange |
            Should -Be 1024
        (Get-Command hyperv_configure_kdnet).Parameters.port.Attributes.MaxRange |
            Should -Be 65535
        (Get-Command hyperv_configure_kdcom).Parameters.com_port.Attributes.ValidValues |
            Should -Be @(1, 2)
    }

    It 'requires the same core parameters as the reference tools' {
        (Get-Command hyperv_get_vm_info).Parameters.vm_name.Attributes.Mandatory |
            Should -BeTrue
        (Get-Command hyperv_checkpoint_restore).Parameters.checkpoint_name.Attributes.Mandatory |
            Should -BeTrue
        (Get-Command hyperv_guest_run).Parameters.command.Attributes.Mandatory |
            Should -BeTrue
        (Get-Command hyperv_guest_run_ps).Parameters.script.Attributes.Mandatory |
            Should -BeTrue
    }

    It 'returns a structured credential error without exposing a password' {
        $oldUsername = $env:HYPERV_GUEST_USERNAME
        $oldPassword = $env:HYPERV_GUEST_PASSWORD
        try {
            $env:HYPERV_GUEST_USERNAME = $null
            $env:HYPERV_GUEST_PASSWORD = $null
            $result = hyperv_guest_run_ps -vm_name test -script 'Get-Date' | ConvertFrom-Json

            $result.ok | Should -BeFalse
            $result.error | Should -Match 'Guest credentials are required'
        }
        finally {
            $env:HYPERV_GUEST_USERNAME = $oldUsername
            $env:HYPERV_GUEST_PASSWORD = $oldPassword
        }
    }
}
