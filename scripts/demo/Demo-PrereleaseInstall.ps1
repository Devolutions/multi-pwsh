param(
    [int]$PauseSeconds = 2,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '_DemoCommon.ps1')

$context = New-DemoContext -DemoName 'prerelease-install' -KeepArtifacts:$KeepArtifacts

try {
    Invoke-DemoStep '1. Create an isolated multi-pwsh home for prerelease installs' 'Show demo directories' {
        Show-DemoContext -Context $context
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '2. Show available 7.6 prerelease entries' 'multi-pwsh list --available --include-prerelease | Select-String 7.6' {
        & multi-pwsh list --available --include-prerelease | Select-String '7\.6|7.6' | ForEach-Object { $_.ToString() } | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '3. Install an exact prerelease build' 'multi-pwsh install 7.6.0-rc.1' {
        & multi-pwsh install 7.6.0-rc.1 | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '4. Show the installed versions and the new 7.6 aliases' 'multi-pwsh list' {
        & multi-pwsh list | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '5. Launch the prerelease line alias directly' 'pwsh-7.6 --version' {
        & pwsh-7.6 --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Invoke-DemoStep '6. Launch the exact prerelease build by exact alias name' 'pwsh-7.6.0-rc.1 --version' {
        & 'pwsh-7.6.0-rc.1' --version | Write-NormalizedOutput
    } -PauseSeconds $PauseSeconds

    Show-Banner 'Demo complete'
    Write-Host 'Artifacts root: ~/.pwsh-demo/prerelease-install' -ForegroundColor Green
    if ($KeepArtifacts) {
        Write-Host 'Keeping demo artifacts for inspection.' -ForegroundColor DarkYellow
    }
}
finally {
    Remove-DemoContext -Context $context
}