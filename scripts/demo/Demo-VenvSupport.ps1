param(
    [int]$PauseSeconds = 2,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = 'Stop'

$multiPwshExe = 'multi-pwsh'
$multiPwshDisplay = 'multi-pwsh'
$pwshSelector = 'pwsh-7.5'
$classicVenv = 'graph-auth'
$modernVenv = 'az-auth'
$copyVenv = 'graph-auth-copy'

function Show-Banner {
    param([string]$Text)

    Write-Host ''
    Write-Host ('=' * 78) -ForegroundColor DarkCyan
    Write-Host $Text -ForegroundColor Cyan
    Write-Host ('=' * 78) -ForegroundColor DarkCyan
}

function Pause-Demo {
    if ($PauseSeconds -gt 0) {
        Start-Sleep -Seconds $PauseSeconds
    }
}

function Invoke-DemoStep {
    param(
        [string]$Caption,
        [string]$DisplayedCommand,
        [scriptblock]$Action
    )

    Show-Banner $Caption
    Write-Host "PS> $DisplayedCommand" -ForegroundColor Yellow
    Pause-Demo
    & $Action
    Pause-Demo
}

function Write-NormalizedOutput {
    param(
        [Parameter(ValueFromPipeline = $true)]
        $InputObject
    )

    process {
        $text = $InputObject.ToString()

        if (-not [string]::IsNullOrWhiteSpace($HOME)) {
            $text = $text -replace [regex]::Escape($HOME), '~'
        }

        Write-Host $text
    }
}

function Show-VenvListing {
    param([string]$Path)

    Push-Location $Path
    try {
        $topLevel = Get-ChildItem -Directory -Name
        $keyFiles = Get-ChildItem -Recurse -File |
            Where-Object {
                $_.Extension -in '.psd1', '.psm1' -or
                $_.Name -in @(
                    'Microsoft.Identity.Client.dll',
                    'Microsoft.IdentityModel.Abstractions.dll',
                    'Microsoft.Graph.Authentication.dll',
                    'Az.Accounts.dll'
                )
            } |
            Select-Object -ExpandProperty FullName |
            ForEach-Object { Resolve-Path -Relative $_ }

        Write-Host '# Top-level module folders' -ForegroundColor DarkCyan
        $topLevel
        Write-Host ''
        Write-Host '# Key module files and identity DLLs' -ForegroundColor DarkCyan
        $keyFiles
    }
    finally {
        Pop-Location
    }
}

$venvRoot = Join-Path $HOME '.pwsh\venv'
$classicRoot = Join-Path $venvRoot $classicVenv
$modernRoot = Join-Path $venvRoot $modernVenv
$copyRoot = Join-Path $venvRoot $copyVenv
$classicModulesRoot = Join-Path $classicRoot 'Modules'
$modernModulesRoot = Join-Path $modernRoot 'Modules'
$copyModulesRoot = Join-Path $copyRoot 'Modules'
$copyArchive = Join-Path $HOME '.pwsh\graph-auth-demo.zip'
$copyArchiveDisplay = '~/.pwsh/graph-auth-demo.zip'

$psGetInstallQuery = @'
$ProgressPreference = 'SilentlyContinue'
Import-Module PowerShellGet -ErrorAction Stop
$venvModulesPath = Join-Path $env:PSMODULE_VENV_PATH 'Modules'
Save-Module -Name Microsoft.Graph.Authentication -Repository PSGallery -Path $venvModulesPath -Force -ErrorAction Stop
Push-Location $venvModulesPath
try {
    Get-Item .\Microsoft.Graph.Authentication
}
finally {
    Pop-Location
}
'@

$psResourceInstallQuery = @'
$ProgressPreference = 'SilentlyContinue'
Install-PSResource -Name Az.Accounts -Repository PSGallery -TrustRepository -Quiet -Reinstall -ErrorAction Stop
$venvModulesPath = Join-Path $env:PSMODULE_VENV_PATH 'Modules'
Push-Location $venvModulesPath
try {
    Get-Item .\Az.Accounts
}
finally {
    Pop-Location
}
'@

$moduleIsolationQuery = @'
Get-Module -ListAvailable Microsoft.Graph.Authentication,Az.Accounts |
    Sort-Object Name |
    Select-Object Name, Version |
    Format-Table -AutoSize
'@

$runtimePathQuery = @'
Import-Module PowerShellGet -ErrorAction Stop
$powerShellGet = Get-Module PowerShellGet -ErrorAction Stop

function Show-HomePath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $Path
    }

    if ($Path.StartsWith($HOME, [System.StringComparison]::OrdinalIgnoreCase)) {
        return ('~' + $Path.Substring($HOME.Length))
    }

    return $Path
}

$modulePathEntries = $env:PSModulePath -split [IO.Path]::PathSeparator | ForEach-Object { Show-HomePath $_ }
[pscustomobject]@{
    VenvPath = Show-HomePath $env:PSMODULE_VENV_PATH
    VenvModulesPath = Show-HomePath (Join-Path $env:PSMODULE_VENV_PATH 'Modules')
    PowerShellGetCurrentUserModules = Show-HomePath ($powerShellGet.SessionState.PSVariable.GetValue("MyDocumentsModulesPath"))
    PowerShellGetPsGetPathCurrentUser = Show-HomePath (($powerShellGet.SessionState.PSVariable.GetValue("PSGetPath")).CurrentUserModules)
    PSModulePathEntries = ($modulePathEntries -join [Environment]::NewLine)
} | Format-List
'@

$importedCopyQuery = @'
Get-Module -ListAvailable Microsoft.Graph.Authentication,Az.Accounts |
    Sort-Object Name |
    Select-Object Name, Version |
    Format-Table -AutoSize
'@

New-Item -ItemType Directory -Force -Path $venvRoot | Out-Null

try {
    foreach ($staleVenv in @($classicVenv, $modernVenv, $copyVenv, 'azure-classic', 'azure-modern', 'azure-classic-copy', 'demo-psget', 'demo-psresource', 'psget-copy')) {
        $stalePath = Join-Path $venvRoot $staleVenv
        if (Test-Path -LiteralPath $stalePath) {
            & $multiPwshExe venv delete $staleVenv | Out-Null
        }
    }

    if (Test-Path -LiteralPath $copyArchive) {
        Remove-Item -LiteralPath $copyArchive -Force
    }

    Invoke-DemoStep '1. Show installed multi-pwsh versions and active venv root' "$multiPwshDisplay list" {
        & $multiPwshExe list | Write-NormalizedOutput
    }

    Invoke-DemoStep '2. Create a venv for Microsoft.Graph.Authentication' "$multiPwshDisplay venv create $classicVenv" {
        & $multiPwshExe venv create $classicVenv | Write-NormalizedOutput
    }

    Invoke-DemoStep '3. Create a venv for Az.Accounts' "$multiPwshDisplay venv create $modernVenv" {
        & $multiPwshExe venv create $modernVenv | Write-NormalizedOutput
    }

    Invoke-DemoStep '4. Verify the demo venvs exist under the default ~/.pwsh/venv root' "$multiPwshDisplay venv list" {
        & $multiPwshExe venv list | Write-NormalizedOutput
    }

    Invoke-DemoStep '5. Download Microsoft.Graph.Authentication into the graph-auth venv with PowerShellGet' "$multiPwshDisplay host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command <Save-Module Microsoft.Graph.Authentication>" {
        & $multiPwshExe host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command $psGetInstallQuery | Write-NormalizedOutput
    }

    Invoke-DemoStep '6. Download Az.Accounts into the az-auth venv with PSResourceGet' "$multiPwshDisplay host $pwshSelector -venv $modernVenv -NoLogo -NoProfile -NonInteractive -Command <Install-PSResource Az.Accounts>" {
        & $multiPwshExe host $pwshSelector -venv $modernVenv -NoLogo -NoProfile -NonInteractive -Command $psResourceInstallQuery | Write-NormalizedOutput
    }

    Invoke-DemoStep '7. cd into the graph-auth venv Modules directory and list the important relative paths' "cd ~/.pwsh/venv/$classicVenv/Modules; <show top-level folders, manifests, psm1 files, and identity DLLs>" {
        Show-VenvListing -Path $classicModulesRoot
    }

    Invoke-DemoStep '8. cd into the az-auth venv Modules directory and list the important relative paths' "cd ~/.pwsh/venv/$modernVenv/Modules; <show top-level folders, manifests, psm1 files, and identity DLLs>" {
        Show-VenvListing -Path $modernModulesRoot
    }

    Invoke-DemoStep '9. Using -venv graph-auth exposes Microsoft.Graph.Authentication without leaking Az.Accounts' "$multiPwshDisplay host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command <module isolation query>" {
        & $multiPwshExe host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command $moduleIsolationQuery | Write-NormalizedOutput
    }

    Invoke-DemoStep '10. Using -venv az-auth exposes Az.Accounts without leaking Microsoft.Graph.Authentication' "$multiPwshDisplay host $pwshSelector -venv $modernVenv -NoLogo -NoProfile -NonInteractive -Command <module isolation query>" {
        & $multiPwshExe host $pwshSelector -venv $modernVenv -NoLogo -NoProfile -NonInteractive -Command $moduleIsolationQuery | Write-NormalizedOutput
    }

    Invoke-DemoStep '11. Show that multi-pwsh rewrites runtime module paths for the selected venv' "$multiPwshDisplay host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command <runtime path query>" {
        & $multiPwshExe host $pwshSelector -venv $classicVenv -NoLogo -NoProfile -NonInteractive -Command $runtimePathQuery | Write-NormalizedOutput
    }

    Invoke-DemoStep '12. Export the graph-auth venv to a zip archive' "$multiPwshDisplay venv export $classicVenv '$copyArchiveDisplay'" {
        & $multiPwshExe venv export $classicVenv $copyArchive | Write-NormalizedOutput
    }

    Invoke-DemoStep '13. Import that archive as graph-auth-copy' "$multiPwshDisplay venv import $copyVenv '$copyArchiveDisplay'" {
        & $multiPwshExe venv import $copyVenv $copyArchive | Write-NormalizedOutput
    }

    Invoke-DemoStep '14. Prove the imported copy still exposes Microsoft.Graph.Authentication' "$multiPwshDisplay host $pwshSelector -venv $copyVenv -NoLogo -NoProfile -NonInteractive -Command <imported copy query>" {
        & $multiPwshExe host $pwshSelector -venv $copyVenv -NoLogo -NoProfile -NonInteractive -Command $importedCopyQuery | Write-NormalizedOutput
    }

    Show-Banner 'Demo complete'
    Write-Host 'Artifacts root: ~/.pwsh/venv' -ForegroundColor Green
    if (-not $KeepArtifacts) {
        Write-Host 'Cleaning up demo artifacts...' -ForegroundColor DarkYellow
        foreach ($demoPath in @($classicRoot, $modernRoot, $copyRoot)) {
            if (Test-Path -LiteralPath $demoPath) {
                Remove-Item -Recurse -Force $demoPath -ErrorAction SilentlyContinue
            }
        }

        if (Test-Path -LiteralPath $copyArchive) {
            Remove-Item -LiteralPath $copyArchive -Force -ErrorAction SilentlyContinue
        }
    }
    else {
        Write-Host 'Keeping demo artifacts for inspection.' -ForegroundColor DarkYellow
    }
}
finally {
}
