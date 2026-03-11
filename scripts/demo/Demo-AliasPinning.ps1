param(
    [int]$PauseSeconds = 2,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '_DemoCommon.ps1')

$context = New-DemoContext -DemoName 'alias-pinning' -KeepArtifacts:$KeepArtifacts

try {
    Invoke-DemoStep '1. Create an isolated multi-pwsh home for alias pinning' 'Show demo directories' {
        Show-DemoContext -Context $context
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '2. Install two exact 7.4 patch releases side by side' 'multi-pwsh install 7.4.12; multi-pwsh install 7.4.13' {
        & multi-pwsh install 7.4.12 | Write-NormalizedOutput
        & multi-pwsh install 7.4.13 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '3. Show the installed versions and aliases in this isolated home' 'multi-pwsh list' {
        & multi-pwsh list | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '4. The pwsh-7.4 shim resolves to the latest installed 7.4 patch by default' 'pwsh-7.4 --version' {
        & pwsh-7.4 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '5. Pin the 7.4 line alias to 7.4.12' 'multi-pwsh alias set 7.4 7.4.12' {
        & multi-pwsh alias set 7.4 7.4.12 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '6. The same pwsh-7.4 shim now resolves to the pinned target' 'pwsh-7.4 --version' {
        & pwsh-7.4 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '7. Unpin the alias so it goes back to tracking latest' 'multi-pwsh alias unset 7.4' {
        & multi-pwsh alias unset 7.4 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '8. The pwsh-7.4 shim now resolves to the latest installed 7.4 patch again' 'pwsh-7.4 --version' {
        & pwsh-7.4 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Show-Banner 'Demo complete'
    Write-Host 'Artifacts root: ~/.pwsh-demo/alias-pinning' -ForegroundColor Green
    if ($KeepArtifacts) {
        Write-Host 'Keeping demo artifacts for inspection.' -ForegroundColor DarkYellow
    }
}
finally {
    Remove-DemoContext -Context $context
}