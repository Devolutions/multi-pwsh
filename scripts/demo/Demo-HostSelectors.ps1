param(
    [int]$PauseSeconds = 2,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '_DemoCommon.ps1')

$context = New-DemoContext -DemoName 'host-selectors' -KeepArtifacts:$KeepArtifacts
$hostQuery = '$PSVersionTable.PSVersion.ToString()'

try {
    Invoke-DemoStep '1. Create an isolated multi-pwsh home for host selector demos' 'Show demo directories' {
        Show-DemoContext -Context $context
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '2. Install the 7.5 line for selector tests' 'multi-pwsh install 7.5' {
        & multi-pwsh install 7.5 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '3. Show the generated host shims in the demo bin directory' 'Get-ChildItem ~/.pwsh-demo/host-selectors/bin -Name' {
        Get-ChildItem -LiteralPath $context.BinDir -Name | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '4. Host selector 7 resolves the latest installed 7.x release' 'multi-pwsh host 7 -NoLogo -NoProfile -NonInteractive -Command "$PSVersionTable.PSVersion.ToString()"' {
        & multi-pwsh host 7 -NoLogo -NoProfile -NonInteractive -Command $hostQuery | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '5. Host selector 7.5 resolves the latest installed 7.5.x release' 'multi-pwsh host 7.5 -NoLogo -NoProfile -NonInteractive -Command "$PSVersionTable.PSVersion.ToString()"' {
        & multi-pwsh host 7.5 -NoLogo -NoProfile -NonInteractive -Command $hostQuery | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '6. Host selector pwsh-7.5 works too' 'multi-pwsh host pwsh-7.5 -NoLogo -NoProfile -NonInteractive -Command "$PSVersionTable.PSVersion.ToString()"' {
        & multi-pwsh host pwsh-7.5 -NoLogo -NoProfile -NonInteractive -Command $hostQuery | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '7. The pwsh-7.5 shim also enters host mode directly' 'pwsh-7.5 --version' {
        & pwsh-7.5 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Show-Banner 'Demo complete'
    Write-Host 'Artifacts root: ~/.pwsh-demo/host-selectors' -ForegroundColor Green
    if ($KeepArtifacts) {
        Write-Host 'Keeping demo artifacts for inspection.' -ForegroundColor DarkYellow
    }
}
finally {
    Remove-DemoContext -Context $context
}