param(
    [Parameter(Mandatory = $false)]
    [string]$OutSurfacePath = "$PSScriptRoot/../dotnet/obj/powershell.ps74.surface.json",

    [Parameter(Mandatory = $false)]
    [string]$OutContractPath = "$PSScriptRoot/../dotnet/obj/bindings.ps74.discovered.contract.json",

    [Parameter(Mandatory = $false)]
    [string]$OutCSharpWrappersPath = "$PSScriptRoot/../dotnet/obj/Bindings.Discovered.Generated.cs",

    [Parameter(Mandatory = $false)]
    [switch]$SkipVersionCheck
)

$ErrorActionPreference = "Stop"

function ConvertTo-SnakeCase {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    $withUnderscore = $Value -replace '([a-z0-9])([A-Z])', '$1_$2'
    $withUnderscore = $withUnderscore -replace '[^A-Za-z0-9_]', '_'
    $withUnderscore.ToLowerInvariant()
}

function Get-SafeIdentifier {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    $candidate = $Value -replace '[^A-Za-z0-9_]', '_'
    if ($candidate -match '^[0-9]') {
        $candidate = "arg_$candidate"
    }

    $reserved = @(
        'as','base','bool','break','byte','case','catch','char','checked','class','const','continue',
        'decimal','default','delegate','do','double','else','enum','event','explicit','extern','false',
        'finally','fixed','float','for','foreach','goto','if','implicit','in','int','interface','internal',
        'is','lock','long','namespace','new','null','object','operator','out','override','params','private',
        'protected','public','readonly','ref','return','sbyte','sealed','short','sizeof','stackalloc',
        'static','string','struct','switch','this','throw','true','try','typeof','uint','ulong','unchecked',
        'unsafe','ushort','using','virtual','void','volatile','while'
    )

    if ($reserved -contains $candidate) {
        return "arg_$candidate"
    }

    return $candidate
}

function New-ContractEntry {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$TableField,
        [Parameter(Mandatory = $true)]
        [string]$RustField,
        [Parameter(Mandatory = $true)]
        [string]$RustTypedef,
        [Parameter(Mandatory = $true)]
        [string]$RustSignature,
        [Parameter(Mandatory = $true)]
        [string]$CSharpFunctionPointer
    )

    return [ordered]@{
        name = $Name
        tableField = $TableField
        rustField = $RustField
        rustTypedef = $RustTypedef
        rustSignature = $RustSignature
        csharpFunctionPointer = $CSharpFunctionPointer
    }
}

function Get-BaseContractEntries {
    return @(
        (New-ContractEntry -Name 'PowerShell_Create' -TableField 'PowerShell_Create' -RustField 'create_fn' -RustTypedef 'FnPowerShellCreate' -RustSignature 'unsafe extern "system" fn() -> PowerShellHandle' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr>'),
        (New-ContractEntry -Name 'PowerShell_AddArgument_String' -TableField 'PowerShell_AddArgument_String' -RustField 'add_argument_string_fn' -RustTypedef 'FnPowerShellAddArgumentString' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, argument: *const libc::c_char)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_AddParameter_String' -TableField 'PowerShell_AddParameter_String' -RustField 'add_parameter_string_fn' -RustTypedef 'FnPowerShellAddParameterString' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: *const libc::c_char)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_AddParameter_Int' -TableField 'PowerShell_AddParameter_Int' -RustField 'add_parameter_int_fn' -RustTypedef 'FnPowerShellAddParameterInt' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: i32)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, int, void>'),
        (New-ContractEntry -Name 'PowerShell_AddParameter_Long' -TableField 'PowerShell_AddParameter_Long' -RustField 'add_parameter_long_fn' -RustTypedef 'FnPowerShellAddParameterLong' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char, value: i64)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, long, void>'),
        (New-ContractEntry -Name 'PowerShell_AddCommand' -TableField 'PowerShell_AddCommand' -RustField 'add_command_fn' -RustTypedef 'FnPowerShellAddCommand' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, command: *const libc::c_char)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_AddScript' -TableField 'PowerShell_AddScript' -RustField 'add_script_fn' -RustTypedef 'FnPowerShellAddScript' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, script: *const libc::c_char)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_AddStatement' -TableField 'PowerShell_AddStatement' -RustField 'add_statement_fn' -RustTypedef 'FnPowerShellAddStatement' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_Invoke' -TableField 'PowerShell_Invoke' -RustField 'invoke_fn' -RustTypedef 'FnPowerShellInvoke' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_Clear' -TableField 'PowerShell_Clear' -RustField 'clear_fn' -RustTypedef 'FnPowerShellClear' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, void>'),
        (New-ContractEntry -Name 'PowerShell_ExportToXml' -TableField 'PowerShell_ExportToXml' -RustField 'export_to_xml_fn' -RustTypedef 'FnPowerShellExportToXml' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, IntPtr>'),
        (New-ContractEntry -Name 'PowerShell_ExportToJson' -TableField 'PowerShell_ExportToJson' -RustField 'export_to_json_fn' -RustTypedef 'FnPowerShellExportToJson' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, IntPtr>'),
        (New-ContractEntry -Name 'PowerShell_ExportToString' -TableField 'PowerShell_ExportToString' -RustField 'export_to_string_fn' -RustTypedef 'FnPowerShellExportToString' -RustSignature 'unsafe extern "system" fn(handle: PowerShellHandle, name: *const libc::c_char) -> *const libc::c_char' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, IntPtr, IntPtr>'),
        (New-ContractEntry -Name 'Marshal_FreeCoTaskMem' -TableField 'Marshal_FreeCoTaskMem' -RustField 'marshal_free_co_task_mem_fn' -RustTypedef 'FnMarshalFreeCoTaskMem' -RustSignature 'unsafe extern "system" fn(ptr: *mut libc::c_void)' -CSharpFunctionPointer 'delegate* unmanaged<IntPtr, void>')
    )
}

function Get-ParameterMapping {
    param(
        [Parameter(Mandatory = $true)]
        [System.Reflection.ParameterInfo]$Parameter
    )

    if ($Parameter.IsOut -or $Parameter.ParameterType.IsByRef) {
        return [ordered]@{ Supported = $false; Reason = "out/ref parameter" }
    }

    $typeName = $Parameter.ParameterType.FullName
    $safeName = Get-SafeIdentifier -Value $Parameter.Name
    $managedName = "${safeName}_managed"

    switch ($typeName) {
        'System.String' {
            return [ordered]@{
                Supported = $true
                CsType = 'IntPtr'
                FpType = 'IntPtr'
                RustType = '*const libc::c_char'
                RustName = $safeName
                CsName = $safeName
                Token = 'String'
                ConvertLine = "            string $managedName = Marshal.PtrToStringUTF8($safeName);"
                CallArg = $managedName
            }
        }
        'System.Int32' {
            return [ordered]@{
                Supported = $true
                CsType = 'int'
                FpType = 'int'
                RustType = 'i32'
                RustName = $safeName
                CsName = $safeName
                Token = 'Int'
                ConvertLine = $null
                CallArg = $safeName
            }
        }
        'System.Int64' {
            return [ordered]@{
                Supported = $true
                CsType = 'long'
                FpType = 'long'
                RustType = 'i64'
                RustName = $safeName
                CsName = $safeName
                Token = 'Long'
                ConvertLine = $null
                CallArg = $safeName
            }
        }
        'System.Boolean' {
            return [ordered]@{
                Supported = $true
                CsType = 'int'
                FpType = 'int'
                RustType = 'i32'
                RustName = $safeName
                CsName = $safeName
                Token = 'Bool'
                ConvertLine = "            bool $managedName = $safeName != 0;"
                CallArg = $managedName
            }
        }
        default {
            return [ordered]@{ Supported = $false; Reason = "unsupported parameter type $typeName" }
        }
    }
}

function Get-ReturnMapping {
    param(
        [Parameter(Mandatory = $true)]
        [Type]$ReturnType
    )

    $typeName = $ReturnType.FullName
    switch ($typeName) {
        'System.Void' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'void'
                FpReturnType = 'void'
                RustReturnType = $null
                Kind = 'void'
            }
        }
        'System.Int32' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'int'
                FpReturnType = 'int'
                RustReturnType = 'i32'
                Kind = 'direct'
            }
        }
        'System.Int64' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'long'
                FpReturnType = 'long'
                RustReturnType = 'i64'
                Kind = 'direct'
            }
        }
        'System.Boolean' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'int'
                FpReturnType = 'int'
                RustReturnType = 'i32'
                Kind = 'bool-int'
            }
        }
        'System.String' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'IntPtr'
                FpReturnType = 'IntPtr'
                RustReturnType = '*const libc::c_char'
                Kind = 'string'
            }
        }
        'System.Management.Automation.PowerShell' {
            return [ordered]@{
                Supported = $true
                CsReturnType = 'IntPtr'
                FpReturnType = 'IntPtr'
                RustReturnType = 'PowerShellHandle'
                Kind = 'handle'
            }
        }
        default {
            return [ordered]@{ Supported = $false; Reason = "unsupported return type $typeName" }
        }
    }
}

function Get-MethodSignature {
    param(
        [Parameter(Mandatory = $true)]
        [System.Reflection.MethodInfo]$Method
    )

    $paramsText = ($Method.GetParameters() | ForEach-Object {
        "$($_.ParameterType.Name) $($_.Name)"
    }) -join ', '

    return "$($Method.ReturnType.Name) $($Method.Name)($paramsText)"
}

if (-not $SkipVersionCheck) {
    if ($PSVersionTable.PSVersion.Major -ne 7 -or $PSVersionTable.PSVersion.Minor -ne 4) {
        throw "Discover-Bindings.ps1 must run with PowerShell 7.4.x. Current version: $($PSVersionTable.PSVersion)"
    }
}

$contractName = 'PS74'
$bindingTypeName = 'NativeHost.Bindings, Bindings'
$bootstrapMethod = 'Bindings_GetApiPS74'
$csharpApiStructName = 'ApiPS74'
$rustApiStructName = 'ApiPs74'
$rustGetApiFunctionType = 'FnBindingsGetApiPs74'

$baseEntries = @(Get-BaseContractEntries)
$existingNames = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($entry in $baseEntries) {
    [void]$existingNames.Add([string]$entry.name)
}

$bindingFlags = [System.Reflection.BindingFlags]::Public -bor [System.Reflection.BindingFlags]::Instance -bor [System.Reflection.BindingFlags]::DeclaredOnly
$methods = [System.Management.Automation.PowerShell].GetMethods($bindingFlags) |
    Where-Object { -not $_.IsSpecialName -and -not $_.ContainsGenericParameters } |
    Sort-Object Name, @{ Expression = { $_.GetParameters().Count } }, @{ Expression = { $_.ToString() } }

$surface = New-Object System.Collections.Generic.List[object]
$discoveredEntries = New-Object System.Collections.Generic.List[object]
$wrapperMethods = New-Object System.Collections.Generic.List[string]
$wrapperNameCounts = @{}

foreach ($method in $methods) {
    $signature = Get-MethodSignature -Method $method
    $parameterMappings = New-Object System.Collections.Generic.List[object]
    $unsupportedReason = $null

    foreach ($parameter in $method.GetParameters()) {
        $mapping = Get-ParameterMapping -Parameter $parameter
        if (-not $mapping.Supported) {
            $unsupportedReason = $mapping.Reason
            break
        }
        [void]$parameterMappings.Add($mapping)
    }

    if (-not $unsupportedReason) {
        $returnMapping = Get-ReturnMapping -ReturnType $method.ReturnType
        if (-not $returnMapping.Supported) {
            $unsupportedReason = $returnMapping.Reason
        }
    }

    if ($unsupportedReason) {
        [void]$surface.Add([ordered]@{
            method = $signature
            supported = $false
            reason = $unsupportedReason
        })
        continue
    }

    $tokenSuffix = if ($parameterMappings.Count -eq 0) {
        'NoArgs'
    }
    else {
        ($parameterMappings | ForEach-Object { $_.Token }) -join '_'
    }

    $wrapperBaseName = "PowerShell_Auto_{0}_{1}" -f $method.Name, $tokenSuffix
    if (-not $wrapperNameCounts.ContainsKey($wrapperBaseName)) {
        $wrapperNameCounts[$wrapperBaseName] = 0
    }
    $wrapperNameCounts[$wrapperBaseName] += 1

    $wrapperName = if ($wrapperNameCounts[$wrapperBaseName] -eq 1) {
        $wrapperBaseName
    }
    else {
        "{0}_{1}" -f $wrapperBaseName, $wrapperNameCounts[$wrapperBaseName]
    }

    if ($existingNames.Contains($wrapperName)) {
        [void]$surface.Add([ordered]@{
            method = $signature
            supported = $false
            reason = "wrapper name collision: $wrapperName"
        })
        continue
    }

    [void]$existingNames.Add($wrapperName)

    $csharpParams = New-Object System.Collections.Generic.List[string]
    [void]$csharpParams.Add('IntPtr ptrHandle')

    $convertLines = New-Object System.Collections.Generic.List[string]
    $callArgs = New-Object System.Collections.Generic.List[string]

    $rustSigParams = New-Object System.Collections.Generic.List[string]
    [void]$rustSigParams.Add('handle: PowerShellHandle')

    $fpArgTypes = New-Object System.Collections.Generic.List[string]
    [void]$fpArgTypes.Add('IntPtr')

    foreach ($parameterMapping in $parameterMappings) {
        [void]$csharpParams.Add("$($parameterMapping.CsType) $($parameterMapping.CsName)")
        if ($parameterMapping.ConvertLine) {
            [void]$convertLines.Add($parameterMapping.ConvertLine)
        }
        [void]$callArgs.Add($parameterMapping.CallArg)
        [void]$rustSigParams.Add("$($parameterMapping.RustName): $($parameterMapping.RustType)")
        [void]$fpArgTypes.Add($parameterMapping.FpType)
    }

    [void]$fpArgTypes.Add($returnMapping.FpReturnType)

    $rustSignature = "unsafe extern `"system`" fn(" + ($rustSigParams -join ', ') + ")"
    if ($returnMapping.RustReturnType) {
        $rustSignature += " -> $($returnMapping.RustReturnType)"
    }

    $csharpFunctionPointer = "delegate* unmanaged<" + ($fpArgTypes -join ', ') + ">"
    $rustField = (ConvertTo-SnakeCase -Value $wrapperName) + '_fn'
    $rustTypedef = "Fn$wrapperName"

    [void]$discoveredEntries.Add([ordered]@{
            name = $wrapperName
            tableField = $wrapperName
            rustField = $rustField
            rustTypedef = $rustTypedef
            rustSignature = $rustSignature
            csharpFunctionPointer = $csharpFunctionPointer
            sourceMethod = $signature
        })

    $callArgsText = $callArgs -join ', '
    $callExpression = "ps.$($method.Name)($callArgsText)"

    $bodyLines = New-Object System.Collections.Generic.List[string]
    [void]$bodyLines.Add('            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);')
    [void]$bodyLines.Add('            PowerShell ps = (PowerShell)gch.Target;')
    foreach ($line in $convertLines) {
        [void]$bodyLines.Add($line)
    }

    switch ($returnMapping.Kind) {
        'void' {
            [void]$bodyLines.Add("            $callExpression;")
        }
        'direct' {
            [void]$bodyLines.Add("            return $callExpression;")
        }
        'bool-int' {
            [void]$bodyLines.Add("            return $callExpression ? 1 : 0;")
        }
        'string' {
            [void]$bodyLines.Add("            string result = $callExpression;")
            [void]$bodyLines.Add('            return Marshal.StringToCoTaskMemUTF8(result ?? string.Empty);')
        }
        'handle' {
            [void]$bodyLines.Add("            PowerShell result = $callExpression;")
            [void]$bodyLines.Add('            return result == null ? IntPtr.Zero : ptrHandle;')
        }
        default {
            throw "Unsupported return mapping kind: $($returnMapping.Kind)"
        }
    }

    $methodText = @"
        [UnmanagedCallersOnly]
        public static $($returnMapping.CsReturnType) $wrapperName($($csharpParams -join ', '))
        {
$($bodyLines -join "`n")
        }
"@

    [void]$wrapperMethods.Add($methodText)

    [void]$surface.Add([ordered]@{
            method = $signature
            supported = $true
            wrapper = $wrapperName
        })
}

$mergedEntries = @()
$mergedEntries += $baseEntries
$mergedEntries += $discoveredEntries

$mergedContract = [ordered]@{
    contractName = $contractName
    bindingTypeName = $bindingTypeName
    bootstrapMethod = $bootstrapMethod
    csharpApiStructName = $csharpApiStructName
    rustApiStructName = $rustApiStructName
    rustGetApiFunctionType = $rustGetApiFunctionType
    entries = $mergedEntries
}

$surfaceReport = [ordered]@{
    runtimePsVersion = [string]$PSVersionTable.PSVersion
    generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    baseEntryCount = $baseEntries.Count
    discoveredEntryCount = $discoveredEntries.Count
    totalEntryCount = $mergedEntries.Count
    methodCount = $methods.Count
    methods = $surface
}

$csharpWrapperContent = @"
// <auto-generated />
using System;
using System.Runtime.InteropServices;
using System.Management.Automation;

namespace NativeHost
{
    public static partial class Bindings
    {
$($wrapperMethods -join "`n")
    }
}
"@

$outSurfaceDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($OutSurfacePath))
$outContractDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($OutContractPath))
$outWrapperDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($OutCSharpWrappersPath))

if (-not (Test-Path -Path $outSurfaceDirectory)) {
    New-Item -Path $outSurfaceDirectory -ItemType Directory -Force | Out-Null
}
if (-not (Test-Path -Path $outContractDirectory)) {
    New-Item -Path $outContractDirectory -ItemType Directory -Force | Out-Null
}
if (-not (Test-Path -Path $outWrapperDirectory)) {
    New-Item -Path $outWrapperDirectory -ItemType Directory -Force | Out-Null
}

Set-Content -Path $OutSurfacePath -Value ($surfaceReport | ConvertTo-Json -Depth 20) -Encoding UTF8
Set-Content -Path $OutContractPath -Value ($mergedContract | ConvertTo-Json -Depth 20) -Encoding UTF8
Set-Content -Path $OutCSharpWrappersPath -Value $csharpWrapperContent -Encoding UTF8

Write-Output "Discovered methods: $($methods.Count)"
Write-Output "Supported wrappers generated: $($discoveredEntries.Count)"
Write-Output "Surface report: $OutSurfacePath"
Write-Output "Merged contract: $OutContractPath"
Write-Output "Generated wrappers: $OutCSharpWrappersPath"
