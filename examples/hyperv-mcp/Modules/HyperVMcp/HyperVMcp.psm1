Set-StrictMode -Version 3.0

function ConvertTo-HyperVMcpJson {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [object] $InputObject,

        [int] $Depth = 8
    )

    ConvertTo-Json -InputObject $InputObject -Depth $Depth -Compress
}

function Assert-HyperVValue {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Value,

        [Parameter(Mandatory)]
        [string] $Name
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Name is required"
    }
}

function New-HyperVGuestCredential {
    param(
        [AllowEmptyString()]
        [string] $Username = '',

        [AllowEmptyString()]
        [string] $Password = '',

        [switch] $Victim
    )

    if ($Victim) {
        $Username = $env:HYPERV_GUEST_VICTIM_USERNAME
        $Password = $env:HYPERV_GUEST_VICTIM_PASSWORD
        if ([string]::IsNullOrWhiteSpace($Username) -or [string]::IsNullOrWhiteSpace($Password)) {
            throw 'No victim credential configured. Set HYPERV_GUEST_VICTIM_USERNAME and HYPERV_GUEST_VICTIM_PASSWORD environment variables to an unprivileged guest account.'
        }
    }
    else {
        if ([string]::IsNullOrWhiteSpace($Username)) {
            $Username = $env:HYPERV_GUEST_USERNAME
        }
        if ([string]::IsNullOrWhiteSpace($Password)) {
            $Password = $env:HYPERV_GUEST_PASSWORD
        }
        if ([string]::IsNullOrWhiteSpace($Username) -or [string]::IsNullOrWhiteSpace($Password)) {
            throw 'Guest credentials are required. Supply username/password arguments or set HYPERV_GUEST_USERNAME and HYPERV_GUEST_PASSWORD environment variables.'
        }
    }

    $securePassword = ConvertTo-SecureString -String $Password -AsPlainText -Force
    [System.Management.Automation.PSCredential]::new($Username, $securePassword)
}

function New-HyperVGuestSession {
    param(
        [Parameter(Mandatory)]
        [string] $VMName,

        [Parameter(Mandatory)]
        [System.Management.Automation.PSCredential] $Credential,

        [int] $OperationTimeoutMs = 120000
    )

    $sessionOption = New-PSSessionOption -OperationTimeout $OperationTimeoutMs
    New-PSSession -VMName $VMName -Credential $Credential -SessionOption $sessionOption -ErrorAction Stop
}

function ConvertTo-HyperVPowerShellLiteral {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Value
    )

    "'$($Value.Replace("'", "''"))'"
}

function New-HyperVCommandScript {
    param(
        [Parameter(Mandatory)]
        [string] $Command,

        [AllowNull()]
        [string[]] $Args,

        [AllowNull()]
        [string] $Cwd
    )

    $argumentLiterals = @($Args | ForEach-Object { ConvertTo-HyperVPowerShellLiteral -Value $_ })
    $invocation = "& $(ConvertTo-HyperVPowerShellLiteral -Value $Command)"
    if ($argumentLiterals.Count -gt 0) {
        $invocation += " $($argumentLiterals -join ' ')"
    }

    if ([string]::IsNullOrWhiteSpace($Cwd)) {
        return "$invocation`nexit `$LASTEXITCODE"
    }

    @"
Push-Location $(ConvertTo-HyperVPowerShellLiteral -Value $Cwd)
try {
    $invocation
}
finally {
    Pop-Location
}
exit `$LASTEXITCODE
"@
}

function Invoke-HyperVGuestScriptInternal {
    param(
        [Parameter(Mandatory)]
        [string] $VMName,

        [Parameter(Mandatory)]
        [string] $Script,

        [Parameter(Mandatory)]
        [System.Management.Automation.PSCredential] $Credential,

        [int] $TimeoutMs = 60000,

        [bool] $Elevated = $false
    )

    $operationTimeout = [Math]::Max(30000, $TimeoutMs + 10000)
    $session = New-HyperVGuestSession -VMName $VMName -Credential $Credential -OperationTimeoutMs $operationTimeout
    try {
        $encodedScript = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
        $result = Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
            param($EncodedScript, $RunElevated)

            $text = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($EncodedScript))
            if ($RunElevated) {
                $scriptPath = [System.IO.Path]::GetTempFileName() + '.ps1'
                $outputPath = [System.IO.Path]::GetTempFileName()
                try {
                    [System.IO.File]::WriteAllText($scriptPath, $text, [Text.Encoding]::Unicode)
                    $process = Start-Process powershell.exe `
                        -ArgumentList "-NonInteractive -NoProfile -ExecutionPolicy Bypass -File `"$scriptPath`" *>`"$outputPath`"" `
                        -Verb RunAs -Wait -PassThru -ErrorAction Stop
                    [pscustomobject]@{
                        exit_code = $process.ExitCode
                        stdout = if (Test-Path -LiteralPath $outputPath) {
                            [System.IO.File]::ReadAllText($outputPath)
                        }
                        else {
                            ''
                        }
                        stderr = ''
                    }
                }
                finally {
                    Remove-Item -LiteralPath $scriptPath, $outputPath -Force -ErrorAction SilentlyContinue
                }
            }
            else {
                $global:LASTEXITCODE = $null
                $output = (& ([scriptblock]::Create($text)) 2>&1) | Out-String
                [pscustomobject]@{
                    exit_code = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
                    stdout = $output
                    stderr = ''
                }
            }
        } -ArgumentList $encodedScript, $Elevated

        [pscustomobject]@{
            ok = $true
            exit_code = $result.exit_code
            stdout = ([string] $result.stdout).Trim()
            stderr = [string] $result.stderr
        }
    }
    finally {
        Remove-PSSession -Session $session -ErrorAction SilentlyContinue
    }
}

function Invoke-HyperVGuestReboot {
    param(
        [Parameter(Mandatory)]
        [string] $VMName,

        [Parameter(Mandatory)]
        [System.Management.Automation.PSCredential] $Credential
    )

    $session = New-HyperVGuestSession -VMName $VMName -Credential $Credential -OperationTimeoutMs 30000
    try {
        Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
            shutdown.exe /r /t 3
        } | Out-Null
    }
    finally {
        Remove-PSSession -Session $session -ErrorAction SilentlyContinue
    }
}

function hyperv_list_vms {
    <#
    .SYNOPSIS
    List all Hyper-V virtual machines and their current state.
    #>
    [CmdletBinding()]
    param()

    $vms = @(
        Get-VM -ErrorAction Stop | ForEach-Object {
            [ordered]@{
                name = $_.Name
                state = [string] $_.State
                status = $_.Status
                memory_mb = [Math]::Round($_.MemoryAssigned / 1MB, 1)
                cpu_count = $_.ProcessorCount
                uptime_seconds = $_.Uptime.TotalSeconds
            }
        }
    )
    ConvertTo-HyperVMcpJson -InputObject $vms
}

function hyperv_get_vm_info {
    <#
    .SYNOPSIS
    Get detailed configuration and runtime information for a Hyper-V VM.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    $vm = Get-VM -Name $vm_name -ErrorAction Stop
    $comPorts = @(Get-VMComPort -VMName $vm_name -ErrorAction Stop | Select-Object Name, Path)
    $networkAdapters = @(
        Get-VMNetworkAdapter -VMName $vm_name -ErrorAction Stop |
            Select-Object Name, SwitchName, MacAddress, IPAddresses
    )
    $hardDrives = @(
        Get-VMHardDiskDrive -VMName $vm_name -ErrorAction Stop |
            Select-Object ControllerType, Path
    )
    $checkpointCount = @(Get-VMSnapshot -VMName $vm_name -ErrorAction Stop).Count

    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        name = $vm.Name
        state = [string] $vm.State
        status = $vm.Status
        generation = $vm.Generation
        memory_mb = [Math]::Round($vm.MemoryAssigned / 1MB, 1)
        dynamic_memory = $vm.DynamicMemoryEnabled
        cpu_count = $vm.ProcessorCount
        uptime_seconds = $vm.Uptime.TotalSeconds
        checkpoint_count = $checkpointCount
        com_ports = $comPorts
        network_adapters = $networkAdapters
        hard_drives = $hardDrives
    })
}

function hyperv_start_vm {
    <#
    .SYNOPSIS
    Start a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Start-VM -Name $vm_name -ErrorAction Stop | Out-Null
    $state = [string] (Get-VM -Name $vm_name -ErrorAction Stop).State
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'started'
        vm_name = $vm_name
        state = $state
    })
}

function hyperv_stop_vm {
    <#
    .SYNOPSIS
    Stop, save, or turn off a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [ValidateSet('shutdown', 'save', 'turnoff')]
        [string] $method = 'shutdown'
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    switch ($method) {
        'shutdown' { Stop-VM -Name $vm_name -Force -ErrorAction Stop }
        'save' { Stop-VM -Name $vm_name -Save -ErrorAction Stop }
        'turnoff' { Stop-VM -Name $vm_name -TurnOff -Force -ErrorAction Stop }
    }
    $state = [string] (Get-VM -Name $vm_name -ErrorAction Stop).State
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'stopped'
        vm_name = $vm_name
        method = $method
        state = $state
    })
}

function hyperv_reset_vm {
    <#
    .SYNOPSIS
    Hard-reset a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Stop-VM -Name $vm_name -TurnOff -Force -ErrorAction Stop
    Start-VM -Name $vm_name -ErrorAction Stop | Out-Null
    $state = [string] (Get-VM -Name $vm_name -ErrorAction Stop).State
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'reset'
        vm_name = $vm_name
        state = $state
    })
}

function hyperv_checkpoint_create {
    <#
    .SYNOPSIS
    Create a checkpoint for a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [AllowEmptyString()]
        [string] $checkpoint_name = ''
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    if ([string]::IsNullOrWhiteSpace($checkpoint_name)) {
        $checkpoint_name = "MCP-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    }
    Checkpoint-VM -Name $vm_name -SnapshotName $checkpoint_name -ErrorAction Stop | Out-Null
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'created'
        vm_name = $vm_name
        checkpoint_name = $checkpoint_name
    })
}

function hyperv_checkpoint_list {
    <#
    .SYNOPSIS
    List all checkpoints for a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    $checkpoints = @(
        Get-VMSnapshot -VMName $vm_name -ErrorAction Stop | ForEach-Object {
            [ordered]@{
                name = $_.Name
                type = [string] $_.SnapshotType
                created = $_.CreationTime.ToString('o')
                parent_name = $_.ParentSnapshotName
            }
        }
    )
    ConvertTo-HyperVMcpJson -InputObject $checkpoints
}

function hyperv_checkpoint_restore {
    <#
    .SYNOPSIS
    Restore a Hyper-V virtual machine to a checkpoint, discarding subsequent state.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $checkpoint_name
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $checkpoint_name -Name 'checkpoint_name'
    Restore-VMSnapshot -Name $checkpoint_name -VMName $vm_name -Confirm:$false -ErrorAction Stop
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'restored'
        vm_name = $vm_name
        checkpoint_name = $checkpoint_name
    })
}

function hyperv_checkpoint_remove {
    <#
    .SYNOPSIS
    Remove a checkpoint, optionally including all child checkpoints.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $checkpoint_name,

        [bool] $include_subtree = $false
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $checkpoint_name -Name 'checkpoint_name'
    $parameters = @{
        Name = $checkpoint_name
        VMName = $vm_name
        Confirm = $false
        ErrorAction = 'Stop'
    }
    if ($include_subtree) {
        $parameters.IncludeAllChildSnapshots = $true
    }
    Remove-VMSnapshot @parameters
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'removed'
        vm_name = $vm_name
        checkpoint_name = $checkpoint_name
    })
}

function hyperv_configure_kdnet {
    <#
    .SYNOPSIS
    Configure KDNET kernel debugging in a VM through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $host_ip,

        [ValidateRange(1024, 65535)]
        [int] $port = 50000,

        [AllowEmptyString()]
        [string] $key = '',

        [bool] $reboot = $false,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $host_ip -Name 'host_ip'
    if ([string]::IsNullOrWhiteSpace($key)) {
        $segments = 1..4 | ForEach-Object {
            [System.Security.Cryptography.RandomNumberGenerator]::GetInt32(0, 0x100000).ToString('x5')
        }
        $key = $segments -join '.'
    }

    $credential = New-HyperVGuestCredential -Username $username -Password $password
    $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential -OperationTimeoutMs 60000
    try {
        $result = Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
            param($HostIp, $Port, $Key)
            $settings = & bcdedit.exe /dbgsettings net "hostip:$HostIp" "port:$Port" "key:$Key" 2>&1
            $enabled = & bcdedit.exe /debug on 2>&1
            [ordered]@{
                DbgSettings = "$settings"
                DebugOn = "$enabled"
                Current = (& bcdedit.exe /dbgsettings 2>&1 | Out-String)
            }
        } -ArgumentList $host_ip, $port, $key
    }
    finally {
        Remove-PSSession -Session $session -ErrorAction SilentlyContinue
    }

    if ($reboot) {
        Invoke-HyperVGuestReboot -VMName $vm_name -Credential $credential
    }
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'configured'
        vm_name = $vm_name
        host_ip = $host_ip
        port = $port
        key = $key
        kernel_attach_string = "net:port=$port,key=$key"
        bcdedit_output = $result
        rebooting = $reboot
    })
}

function hyperv_configure_kdcom {
    <#
    .SYNOPSIS
    Configure named-pipe serial kernel debugging for a Hyper-V virtual machine.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [AllowEmptyString()]
        [string] $pipe_name = '',

        [ValidateSet(1, 2)]
        [int] $com_port = 1,

        [bool] $reboot = $false,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    if ([string]::IsNullOrWhiteSpace($pipe_name)) {
        $safeName = $vm_name.Replace(' ', '_').Replace('\', '_').Replace('/', '_')
        $pipe_name = "\\.\pipe\kd_$safeName"
    }

    $credential = New-HyperVGuestCredential -Username $username -Password $password
    Set-VMComPort -VMName $vm_name -Number $com_port -Path $pipe_name -ErrorAction Stop
    $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential -OperationTimeoutMs 60000
    try {
        $result = Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
            param($ComPort)
            $settings = & bcdedit.exe /dbgsettings serial "debugport:$ComPort" baudrate:115200 2>&1
            $enabled = & bcdedit.exe /debug on 2>&1
            [ordered]@{
                DbgSettings = "$settings"
                DebugOn = "$enabled"
                Current = (& bcdedit.exe /dbgsettings 2>&1 | Out-String)
            }
        } -ArgumentList $com_port
    }
    finally {
        Remove-PSSession -Session $session -ErrorAction SilentlyContinue
    }

    if ($reboot) {
        Invoke-HyperVGuestReboot -VMName $vm_name -Credential $credential
    }
    ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
        status = 'configured'
        vm_name = $vm_name
        com_port = $com_port
        pipe_path = $pipe_name
        kernel_attach_string = "com:pipe,port=$pipe_name,resets=0,reconnect"
        bcdedit_output = $result
        rebooting = $reboot
    })
}

function hyperv_guest_run_ps {
    <#
    .SYNOPSIS
    Run a PowerShell script inside a VM through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $script,

        [ValidateRange(1, 2147483647)]
        [int] $timeout_ms = 60000,

        [bool] $elevated = $false,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $script -Name 'script'
    try {
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $result = Invoke-HyperVGuestScriptInternal -VMName $vm_name -Script $script -Credential $credential -TimeoutMs $timeout_ms -Elevated $elevated
        ConvertTo-HyperVMcpJson -InputObject $result
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_guest_run {
    <#
    .SYNOPSIS
    Run an executable inside a VM through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $command,

        [AllowNull()]
        [string[]] $args = $null,

        [AllowNull()]
        [string] $cwd = $null,

        [ValidateRange(1, 2147483647)]
        [int] $timeout_ms = 60000,

        [bool] $elevated = $false,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $command -Name 'command'
    try {
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $commandScript = New-HyperVCommandScript -Command $command -Args $args -Cwd $cwd
        $result = Invoke-HyperVGuestScriptInternal -VMName $vm_name -Script $commandScript -Credential $credential -TimeoutMs $timeout_ms -Elevated $elevated
        ConvertTo-HyperVMcpJson -InputObject $result
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_guest_put {
    <#
    .SYNOPSIS
    Copy a host file into a VM through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $local_path,

        [Parameter(Mandatory)]
        [string] $remote_path,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    try {
        Assert-HyperVValue -Value $vm_name -Name 'vm_name'
        Assert-HyperVValue -Value $local_path -Name 'local_path'
        Assert-HyperVValue -Value $remote_path -Name 'remote_path'
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential -OperationTimeoutMs 300000
        try {
            $remoteDirectory = [System.IO.Path]::GetDirectoryName($remote_path)
            if (-not [string]::IsNullOrWhiteSpace($remoteDirectory)) {
                Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
                    param($Directory)
                    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
                } -ArgumentList $remoteDirectory
            }
            Copy-Item -ToSession $session -LiteralPath $local_path -Destination $remote_path -Force -ErrorAction Stop
            $bytesCopied = (Get-Item -LiteralPath $local_path -ErrorAction Stop).Length
        }
        finally {
            Remove-PSSession -Session $session -ErrorAction SilentlyContinue
        }
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $true; bytes_copied = $bytesCopied })
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_guest_get {
    <#
    .SYNOPSIS
    Copy a file from a VM to the host through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $remote_path,

        [Parameter(Mandatory)]
        [string] $local_path,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    try {
        Assert-HyperVValue -Value $vm_name -Name 'vm_name'
        Assert-HyperVValue -Value $remote_path -Name 'remote_path'
        Assert-HyperVValue -Value $local_path -Name 'local_path'
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential -OperationTimeoutMs 300000
        try {
            Copy-Item -FromSession $session -Path $remote_path -Destination $local_path -Force -ErrorAction Stop
            $bytesCopied = (Get-Item -LiteralPath $local_path -ErrorAction Stop).Length
        }
        finally {
            Remove-PSSession -Session $session -ErrorAction SilentlyContinue
        }
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $true; bytes_copied = $bytesCopied })
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_guest_read_file {
    <#
    .SYNOPSIS
    Read a bounded file from a VM and return its bytes as base64.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $remote_path,

        [ValidateRange(1, 2147483647)]
        [int] $max_bytes = 262144,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    try {
        Assert-HyperVValue -Value $vm_name -Name 'vm_name'
        Assert-HyperVValue -Value $remote_path -Name 'remote_path'
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential
        try {
            $result = Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
                param($Path, $MaximumBytes)
                $bytes = [System.IO.File]::ReadAllBytes($Path)
                $truncated = $bytes.Length -gt $MaximumBytes
                if ($truncated) {
                    $bytes = $bytes[0..($MaximumBytes - 1)]
                }
                [ordered]@{
                    content_b64 = [Convert]::ToBase64String($bytes)
                    bytes_read = $bytes.Length
                    truncated = $truncated
                }
            } -ArgumentList $remote_path, $max_bytes
        }
        finally {
            Remove-PSSession -Session $session -ErrorAction SilentlyContinue
        }
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{
            ok = $true
            content_b64 = $result.content_b64
            bytes_read = $result.bytes_read
            truncated = $result.truncated
        })
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_guest_list_dir {
    <#
    .SYNOPSIS
    List entries in a VM directory through PowerShell Direct.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $remote_path,

        [AllowEmptyString()]
        [string] $username = '',

        [AllowEmptyString()]
        [string] $password = ''
    )

    try {
        Assert-HyperVValue -Value $vm_name -Name 'vm_name'
        Assert-HyperVValue -Value $remote_path -Name 'remote_path'
        $credential = New-HyperVGuestCredential -Username $username -Password $password
        $session = New-HyperVGuestSession -VMName $vm_name -Credential $credential -OperationTimeoutMs 60000
        try {
            $entries = @(
                Invoke-Command -Session $session -ErrorAction Stop -ScriptBlock {
                    param($Path)
                    Get-ChildItem -LiteralPath $Path -ErrorAction Stop | ForEach-Object {
                        [ordered]@{
                            name = $_.Name
                            is_dir = $_.PSIsContainer
                            size_bytes = if ($_.PSIsContainer) { 0 } else { $_.Length }
                            modified = $_.LastWriteTimeUtc.ToString('yyyy-MM-ddTHH:mm:ssZ')
                        }
                    }
                } -ArgumentList $remote_path
            )
        }
        finally {
            Remove-PSSession -Session $session -ErrorAction SilentlyContinue
        }
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $true; entries = $entries })
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_victim_run {
    <#
    .SYNOPSIS
    Run an executable in a VM as the configured unprivileged victim account.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $command,

        [AllowNull()]
        [string[]] $args = $null,

        [AllowNull()]
        [string] $cwd = $null,

        [ValidateRange(1, 2147483647)]
        [int] $timeout_ms = 60000
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $command -Name 'command'
    try {
        $credential = New-HyperVGuestCredential -Victim
        $commandScript = New-HyperVCommandScript -Command $command -Args $args -Cwd $cwd
        $result = Invoke-HyperVGuestScriptInternal -VMName $vm_name -Script $commandScript -Credential $credential -TimeoutMs $timeout_ms
        ConvertTo-HyperVMcpJson -InputObject $result
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

function hyperv_victim_run_ps {
    <#
    .SYNOPSIS
    Run PowerShell in a VM as the configured unprivileged victim account.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $vm_name,

        [Parameter(Mandatory)]
        [string] $script,

        [ValidateRange(1, 2147483647)]
        [int] $timeout_ms = 60000
    )

    Assert-HyperVValue -Value $vm_name -Name 'vm_name'
    Assert-HyperVValue -Value $script -Name 'script'
    try {
        $credential = New-HyperVGuestCredential -Victim
        $result = Invoke-HyperVGuestScriptInternal -VMName $vm_name -Script $script -Credential $credential -TimeoutMs $timeout_ms
        ConvertTo-HyperVMcpJson -InputObject $result
    }
    catch {
        ConvertTo-HyperVMcpJson -InputObject ([ordered]@{ ok = $false; error = $_.Exception.Message })
    }
}

Export-ModuleMember -Function @(
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
