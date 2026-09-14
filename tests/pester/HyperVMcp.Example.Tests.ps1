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

    It 'saves a running VM before mapping KDCOM and waits for PowerShell Direct' {
        InModuleScope HyperVMcp {
            $script:calls = [System.Collections.Generic.List[string]]::new()
            $script:sessionAttempts = 0
            Mock Get-VM { [pscustomobject]@{ State = 'Running' } }
            Mock New-HyperVGuestCredential {
                [pscredential]::new('guest', (ConvertTo-SecureString 'password' -AsPlainText -Force))
            }
            Mock Stop-VM {
                $script:calls.Add('save') | Out-Null
            } -ParameterFilter { $Save }
            Mock Set-VMComPort {
                $script:calls.Add('map') | Out-Null
            }
            Mock Start-VM {
                $script:calls.Add('start') | Out-Null
            }
            Mock New-HyperVGuestSession {
                $script:sessionAttempts++
                $script:calls.Add('session') | Out-Null
                if ($script:sessionAttempts -eq 1) {
                    throw 'PowerShell Direct is not ready'
                }
                [System.Runtime.Serialization.FormatterServices]::GetUninitializedObject(
                    [System.Management.Automation.Runspaces.PSSession]
                )
            }
            Mock Start-Sleep {}
            Mock Invoke-Command {
                [ordered]@{ DbgSettings = 'ok'; DebugOn = 'ok'; Current = 'ok' }
            } -RemoveParameterType Session
            Mock Remove-PSSession {} -RemoveParameterType Session

            $result = hyperv_configure_kdcom -vm_name test -username guest -password password |
                ConvertFrom-Json

            $result.status | Should -Be 'configured'
            $script:calls | Should -Be @('save', 'map', 'start', 'session', 'session')
        }
    }

    It 'restores an off VM after configuring KDCOM' {
        InModuleScope HyperVMcp {
            $script:state = 'Off'
            $script:calls = [System.Collections.Generic.List[string]]::new()
            Mock Get-VM { [pscustomobject]@{ State = $script:state } }
            Mock New-HyperVGuestCredential {
                [pscredential]::new('guest', (ConvertTo-SecureString 'password' -AsPlainText -Force))
            }
            Mock Set-VMComPort {
                $script:calls.Add('map') | Out-Null
            }
            Mock Start-VM {
                $script:state = 'Running'
                $script:calls.Add('start') | Out-Null
            }
            Mock New-HyperVGuestSession {
                [System.Runtime.Serialization.FormatterServices]::GetUninitializedObject(
                    [System.Management.Automation.Runspaces.PSSession]
                )
            }
            Mock Invoke-Command {
                [ordered]@{ DbgSettings = 'ok'; DebugOn = 'ok'; Current = 'ok' }
            } -RemoveParameterType Session
            Mock Remove-PSSession {} -RemoveParameterType Session
            Mock Stop-VM {
                $script:state = 'Off'
                $script:calls.Add('off') | Out-Null
            } -ParameterFilter { $TurnOff }

            $result = hyperv_configure_kdcom -vm_name test -username guest -password password |
                ConvertFrom-Json

            $result.status | Should -Be 'configured'
            $script:calls | Should -Be @('map', 'start', 'off')
            $script:state | Should -Be 'Off'
        }
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

    Context 'guest execution helpers' {
        It 'makes a missing command working directory fail' {
            $module = Get-Module HyperVMcp
            $script = & $module {
                New-HyperVCommandScript -Command 'cmd.exe' -Args @('/c', 'exit 0') -Cwd 'Z:\missing'
            }

            $script | Should -Match 'Push-Location .* -ErrorAction Stop'
        }

        It 'returns a result when a command exits with a nonzero code' {
            $module = Get-Module HyperVMcp
            Mock New-HyperVGuestSession {
                [System.Runtime.Serialization.FormatterServices]::GetUninitializedObject(
                    [System.Management.Automation.Runspaces.PSSession]
                )
            } -ModuleName HyperVMcp
            Mock Remove-PSSession -ModuleName HyperVMcp
            Mock Invoke-Command {
                & $ScriptBlock @ArgumentList
            } -ModuleName HyperVMcp
            $credential = [pscredential]::new(
                'test',
                (ConvertTo-SecureString 'test' -AsPlainText -Force)
            )
            $result = & $module {
                param($Credential)
                $script = New-HyperVCommandScript -Command 'cmd.exe' -Args @('/c', 'exit 7')
                Invoke-HyperVGuestScriptInternal `
                    -VMName 'test' `
                    -Script $script `
                    -Credential $Credential
            } $credential

            $result.ok | Should -BeTrue
            $result.exit_code | Should -Be 7
            Should -Invoke Remove-PSSession -ModuleName HyperVMcp -Times 1
        }

        It 'throws when the guest reboot command is refused' {
            $module = Get-Module HyperVMcp
            Mock New-HyperVGuestSession {
                [System.Runtime.Serialization.FormatterServices]::GetUninitializedObject(
                    [System.Management.Automation.Runspaces.PSSession]
                )
            } -ModuleName HyperVMcp
            Mock Remove-PSSession -ModuleName HyperVMcp
            Mock Invoke-Command {
                $global:capturedGuestRebootScript = $ScriptBlock
                throw 'Guest reboot failed with exit code 5.'
            } -ModuleName HyperVMcp
            $credential = [pscredential]::new(
                'test',
                (ConvertTo-SecureString 'test' -AsPlainText -Force)
            )

            {
                & $module {
                    param($Credential)
                    Invoke-HyperVGuestReboot -VMName 'test' -Credential $Credential
                } $credential
            } | Should -Throw '*exit code 5*'
            Should -Invoke Remove-PSSession -ModuleName HyperVMcp -Times 1
            $global:capturedGuestRebootScript.ToString() |
                Should -Match '\$LASTEXITCODE -ne 0'
            Remove-Variable capturedGuestRebootScript -Scope Global -ErrorAction SilentlyContinue
        }
    }

    It 'treats the guest source path as a literal path when downloading' {
        $module = Get-Module HyperVMcp
        $definition = & $module {
            (Get-Command hyperv_guest_get).Definition
        }

        $definition | Should -Match (
            'Copy-Item -FromSession \$session -LiteralPath \$remote_path ' +
            '-Destination \$local_path'
        )
    }

    It 'reads no more than the requested guest file bytes before checking truncation' {
        try {
            $sourcePath = Join-Path $TestDrive 'guest-file.bin'
            [System.IO.File]::WriteAllBytes($sourcePath, [byte[]](0..9))
            Mock New-HyperVGuestSession { [pscustomobject]@{ Id = 1 } } -ModuleName HyperVMcp
            Mock Invoke-Command {
                param($Session, $ErrorAction, $ScriptBlock, $ArgumentList)
                $global:capturedGuestReadScript = $ScriptBlock
                & $ScriptBlock @ArgumentList
            } -ModuleName HyperVMcp -RemoveParameterType Session
            Mock Remove-PSSession -ModuleName HyperVMcp -RemoveParameterType Session

            $result = hyperv_guest_read_file -vm_name test -remote_path $sourcePath `
                -max_bytes 4 -username user -password password |
                ConvertFrom-Json

            $result.ok | Should -BeTrue
            $result.bytes_read | Should -Be 4
            $result.truncated | Should -BeTrue
            [Convert]::FromBase64String($result.content_b64) |
                Should -Be ([byte[]](0..3))

            $global:capturedGuestReadScript.ToString() | Should -Not -Match 'ReadAllBytes'
        }
        finally {
            Remove-Variable capturedGuestReadScript -Scope Global -ErrorAction SilentlyContinue
        }
    }
}
