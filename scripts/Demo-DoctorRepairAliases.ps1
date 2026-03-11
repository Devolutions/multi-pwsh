param(
    [int]$PauseSeconds = 2,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '_DemoCommon.ps1')

$context = New-DemoContext -DemoName 'doctor-repair' -KeepArtifacts:$KeepArtifacts
$shimPath = Join-Path $context.BinDir 'pwsh-7.5.exe'

try {
    Invoke-DemoStep '1. Create an isolated multi-pwsh home for alias repair' 'Show demo directories' {
        Show-DemoContext -Context $context
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '2. Install the 7.5 line so alias shims are created in the demo bin directory' 'multi-pwsh install 7.5' {
        & multi-pwsh install 7.5 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '3. Show the demo bin contents before any damage' 'Get-ChildItem ~/.pwsh-demo/doctor-repair/bin -Name' {
        Get-ChildItem -LiteralPath $context.BinDir -Name | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '4. Simulate a broken host shim by deleting pwsh-7.5.exe' 'Remove-Item ~/.pwsh-demo/doctor-repair/bin/pwsh-7.5.exe' {
        Remove-Item -LiteralPath $shimPath -Force
        Get-ChildItem -LiteralPath $context.BinDir -Name | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '5. Repair aliases and host shims with doctor' 'multi-pwsh doctor --repair-aliases' {
        & multi-pwsh doctor --repair-aliases | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '6. Show the restored demo bin contents' 'Get-ChildItem ~/.pwsh-demo/doctor-repair/bin -Name' {
        Get-ChildItem -LiteralPath $context.BinDir -Name | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '7. The repaired pwsh-7.5 shim launches again' 'pwsh-7.5 --version' {
        & pwsh-7.5 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Show-Banner 'Demo complete'
    Write-Host 'Artifacts root: ~/.pwsh-demo/doctor-repair' -ForegroundColor Green
    if ($KeepArtifacts) {
        Write-Host 'Keeping demo artifacts for inspection.' -ForegroundColor DarkYellow
    }
}
finally {
    Remove-DemoContext -Context $context
}