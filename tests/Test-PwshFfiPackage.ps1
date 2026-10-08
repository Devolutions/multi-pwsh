[CmdletBinding()]
param(
    [string]$PackageSource,

    [string]$PackageVersion,

    [string]$PowerShellPayloadDirectory = $env:PWSH_FFI_PAYLOAD,

    [string]$Configuration = 'Release',

    [string[]]$ExpectedRuntimeIdentifiers = @('win-x64'),

    [switch]$AllowPreviewVersionMismatch,

    [switch]$AllowPowerShellVersionMismatch,

    [ValidateSet('PayloadHooksMissing', 'TransportImplementationMissing')]
    [string]$ExpectedRemoteProviderUnavailableReason = 'PayloadHooksMissing',

    [switch]$KeepWorkspace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'The FFI package smoke test currently requires a Windows x64 host.'
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$packageId = 'Devolutions.MultiPwsh.Sdk'

function Get-MultiPwshVersion {
    $manifestPath = Join-Path $repoRoot 'crates\multi-pwsh\Cargo.toml'
    $match = Select-String -Path $manifestPath -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
    if ($null -eq $match) {
        throw "Unable to read the multi-pwsh version from $manifestPath"
    }

    return $match.Matches[0].Groups[1].Value
}

function Assert-RestoredPackageMatchesInspectedNupkg {
    param(
        [Parameter(Mandatory)][string]$NugetCache,
        [Parameter(Mandatory)][string]$PackageId,
        [Parameter(Mandatory)][string]$PackageVersion,
        [Parameter(Mandatory)][string]$InspectedPackagePath
    )

    $restoredRoot = Join-Path $NugetCache ("{0}\{1}" -f $PackageId.ToLowerInvariant(), $PackageVersion)
    if (-not (Test-Path -LiteralPath $restoredRoot -PathType Container)) {
        throw "Restore did not install $PackageId $PackageVersion into the isolated NuGet cache at $restoredRoot."
    }

    $expectedHash = (Get-FileHash -Algorithm SHA512 -LiteralPath $InspectedPackagePath).Hash
    $shaFile = Join-Path $restoredRoot "$PackageId.$PackageVersion.nupkg.sha512"
    if (Test-Path -LiteralPath $shaFile -PathType Leaf) {
        $actualBase64 = (Get-Content -LiteralPath $shaFile -Raw).Trim()
        $actualBytes = [Convert]::FromBase64String($actualBase64)
        $actualHash = [BitConverter]::ToString($actualBytes).Replace('-', '')
        if (-not $actualHash.Equals($expectedHash, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Restored $PackageId $PackageVersion does not match the inspected local nupkg hash."
        }

        return
    }

    $restoredNupkg = Join-Path $restoredRoot "$PackageId.$PackageVersion.nupkg"
    if (-not (Test-Path -LiteralPath $restoredNupkg -PathType Leaf)) {
        throw "Restore did not materialize $PackageId $PackageVersion nupkg metadata for hash verification."
    }

    $actualHash = (Get-FileHash -Algorithm SHA512 -LiteralPath $restoredNupkg).Hash
    if (-not $actualHash.Equals($expectedHash, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Restored $PackageId $PackageVersion does not match the inspected local nupkg hash."
    }
}

$multiPwshVersion = Get-MultiPwshVersion
$sdkNativeAssets = @{
    'win-x64' = 'multi-pwsh-sdk.dll'
    'win-arm64' = 'multi-pwsh-sdk.dll'
    'linux-x64' = 'libmulti-pwsh-sdk.so'
    'linux-arm64' = 'libmulti-pwsh-sdk.so'
    'linux-arm' = 'libmulti-pwsh-sdk.so'
    'osx-x64' = 'libmulti-pwsh-sdk.dylib'
    'osx-arm64' = 'libmulti-pwsh-sdk.dylib'
}
foreach ($runtimeIdentifier in $ExpectedRuntimeIdentifiers) {
    if (-not $sdkNativeAssets.ContainsKey($runtimeIdentifier)) {
        throw "Unsupported SDK runtime identifier expected by this test: $runtimeIdentifier"
    }
}
if ([string]::IsNullOrWhiteSpace($PackageSource)) {
    $PackageSource = Join-Path $repoRoot 'artifacts\sdk-nuget'
}
elseif (-not [System.IO.Path]::IsPathRooted($PackageSource)) {
    $PackageSource = Join-Path $repoRoot $PackageSource
}

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList
    )

    Write-Host ">> $FilePath $($ArgumentList -join ' ')"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $FilePath $($ArgumentList -join ' ')"
    }
}

function Resolve-PowerShellPayloadDirectory {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = (Resolve-Path $Path).Path
    if (-not (Test-Path (Join-Path $resolved 'pwsh.dll') -PathType Leaf) -or
        -not (Test-Path (Join-Path $resolved 'pwsh.runtimeconfig.json') -PathType Leaf) -or
        -not (Test-Path (Join-Path $resolved 'System.Management.Automation.dll') -PathType Leaf) -or
        -not (Test-Path (Join-Path $resolved 'pwsh.exe') -PathType Leaf)) {
        throw "The PowerShell payload is missing pwsh.dll, pwsh.runtimeconfig.json, System.Management.Automation.dll, or pwsh.exe: $resolved"
    }

    $version = & (Join-Path $resolved 'pwsh.exe') -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'
    if ($LASTEXITCODE -ne 0) {
        throw "The FFI package smoke could not start the PowerShell payload at '$resolved'."
    }
    if ($version -notmatch '^7\.4\.' -and -not $AllowPowerShellVersionMismatch) {
        throw "The FFI package smoke requires a PowerShell 7.4 payload, but '$resolved' reported '$version'."
    }
    if ($version -notmatch '^7\.4\.') {
        Write-Warning "Testing an explicit non-7.4 PowerShell payload: $version."
    }

    $script:QualifiedPowerShellVersion = $version.Trim()

    return $resolved
}

$packageSource = (Resolve-Path $PackageSource).Path
$package = if ([string]::IsNullOrWhiteSpace($PackageVersion)) {
    Get-ChildItem -Path $packageSource -Filter "$packageId.*.nupkg" |
        Sort-Object Name -Descending |
        Select-Object -First 1
}
else {
    Get-ChildItem -Path $packageSource -Filter "$packageId.$PackageVersion.nupkg" |
        Select-Object -First 1
}

if ($null -eq $package) {
    throw "$packageId package was not found in $packageSource"
}

if ([string]::IsNullOrWhiteSpace($PackageVersion)) {
    $PackageVersion = $package.BaseName.Substring("$packageId.".Length)
}
if ($PackageVersion -ne $multiPwshVersion -and -not $AllowPreviewVersionMismatch) {
    throw "$packageId version $PackageVersion must match multi-pwsh version $multiPwshVersion"
}
if ($PackageVersion -ne $multiPwshVersion) {
    Write-Warning "Testing explicit preview package version $PackageVersion against multi-pwsh version $multiPwshVersion."
}

$archive = [System.IO.Compression.ZipFile]::OpenRead($package.FullName)
try {
    $archivePaths = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]($archive.Entries | ForEach-Object FullName),
        [System.StringComparer]::OrdinalIgnoreCase)
    $requiredPaths = @(
        'README.md',
        'buildTransitive/Devolutions.MultiPwsh.Sdk.targets',
        'buildTransitive/Devolutions.MultiPwsh.LiveContracts/PowerShellFiniteOperations.cs',
        'lib/net10.0/Devolutions.MultiPwsh.Sdk.dll')
    foreach ($runtimeIdentifier in $ExpectedRuntimeIdentifiers) {
        $requiredPaths += "runtimes/$runtimeIdentifier/native/$($sdkNativeAssets[$runtimeIdentifier])"
    }
    foreach ($requiredPath in $requiredPaths) {
        if (-not $archivePaths.Contains($requiredPath)) {
            throw "Package is missing required entry: $requiredPath"
        }
    }
    $retiredPayloadTemplates = @(
        $archivePaths | Where-Object {
            $_ -like 'contentFiles/*/devolutions-pwsh-payload*'
        })
    if ($retiredPayloadTemplates.Count -ne 0) {
        throw "Package must not include retired PowerShell payload templates: $($retiredPayloadTemplates -join ', ')"
    }

    $nativeEntry = $archive.GetEntry('runtimes/win-x64/native/multi-pwsh-sdk.dll')
    if ($null -eq $nativeEntry) {
        throw 'Package native FFI asset could not be opened.'
    }

    $nativeStream = $nativeEntry.Open()
    $nativeBytes = [System.IO.MemoryStream]::new()
    try {
        $nativeStream.CopyTo($nativeBytes)
    }
    finally {
        $nativeStream.Dispose()
    }

    $nativeImports = [System.Text.Encoding]::ASCII.GetString($nativeBytes.ToArray())
    if ($nativeImports -match 'VCRUNTIME[0-9_]*\.DLL' -or $nativeImports -match 'MSVCP[0-9_]*\.DLL') {
        throw 'The packaged native FFI asset must use the static MSVC runtime.'
    }
}
finally {
    $archive.Dispose()
}

$payloadDirectory = Resolve-PowerShellPayloadDirectory -Path $PowerShellPayloadDirectory
$smokeId = [guid]::NewGuid().ToString('N')
$workspace = Join-Path (Join-Path $repoRoot 'artifacts') "ffi-package-smoke-$smokeId"
# Keep the isolated package cache on a short path so NativeAOT link inputs stay well
# under legacy MAX_PATH limits while still excluding every fallback folder.
$nugetCache = Join-Path ([System.IO.Path]::GetTempPath()) "mpwsh-nupkg-$smokeId"
$oldNugetPackages = $env:NUGET_PACKAGES
$oldNugetFallbackPackages = $env:NUGET_FALLBACK_PACKAGES
$env:NUGET_PACKAGES = $nugetCache
# Do not point fallback folders at the user global cache: a pre-existing
# Devolutions.MultiPwsh.Sdk/<version> copy there would satisfy restore without
# exercising the inspected local nupkg.
$env:NUGET_FALLBACK_PACKAGES = ''

try {
    New-Item -Path $nugetCache -ItemType Directory -Force | Out-Null
    New-Item -Path $workspace -ItemType Directory -Force | Out-Null
    $nugetConfig = Join-Path $workspace 'NuGet.Config'
    @"
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="local" value="$packageSource" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
"@ | Set-Content -Path $nugetConfig -Encoding utf8

    $inertProjectDirectory = Join-Path $workspace 'inert'
    New-Item -Path $inertProjectDirectory -ItemType Directory -Force | Out-Null
    $inertProject = Join-Path $inertProjectDirectory 'Inert.csproj'
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="$packageId" Version="$PackageVersion" PrivateAssets="all" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $inertProject -Encoding utf8
    'using System; Console.WriteLine("inert");' | Set-Content -Path (Join-Path $inertProjectDirectory 'Program.cs') -Encoding utf8

    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('restore', $inertProject, '--configfile', $nugetConfig)
    Assert-RestoredPackageMatchesInspectedNupkg -NugetCache $nugetCache -PackageId $packageId -PackageVersion $PackageVersion -InspectedPackagePath $package.FullName
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('build', $inertProject, '--no-restore', '-c', $Configuration)
    $inertNativeAsset = Join-Path $inertProjectDirectory "bin\$Configuration\net10.0\win-x64\multi-pwsh-sdk.dll"
    if (Test-Path $inertNativeAsset -PathType Leaf) {
        throw "FFI native assets must be inert by default, but found $inertNativeAsset"
    }

    $finitePayloadDirectory = Join-Path $workspace 'finite-payload'
    New-Item -Path $finitePayloadDirectory -ItemType Directory -Force | Out-Null
    $finitePayloadProject = Join-Path $finitePayloadDirectory 'FfiPackageFinitePayload.csproj'
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <LiveContractMode>Payload</LiveContractMode>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="$packageId" Version="$PackageVersion" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $finitePayloadProject -Encoding utf8
    @"
using Devolutions.PowerShell.Ffi.LiveObjects;
using Devolutions.PowerShell.Ffi.LiveObjects.FiniteOperations;

internal static class Program
{
    private sealed class Codec : IPowerShellFinitePageCodec<int>
    {
        public bool TryCopy(int source, out int copy, out int itemCount, out int byteCount)
        {
            copy = source;
            itemCount = 1;
            byteCount = sizeof(int);
            return true;
        }
    }

    private sealed class Validator : IPowerShellFinitePageAccessValidator
    {
        public PowerShellFinitePageValidation Validate(in PowerShellFiniteOperationBinding binding) =>
            PowerShellFinitePageValidation.Allowed;
    }

    private static void Main()
    {
        var contract = new PowerShellFinitePageContract<int>(
            new Guid("59B43B9D-2D29-43EE-B25D-8706E3AA50C5"),
            maximumPages: 1,
            maximumItemsPerPage: 1,
            maximumPageBytes: sizeof(int),
            new Codec(),
            new Validator());
        using var registry = new PowerShellFiniteOperationRegistry<int>(contract, maximumOperations: 1);
        using PowerShellFiniteOperationOwner owner = registry.CreateOwner();
        PowerShellFiniteOperationResult result = registry.TryStart(
            owner,
            new PowerShellFiniteOperationBinding(contract.SchemaId, snapshotRevision: 1, permissionRevision: 1),
            new PowerShellFiniteOperationOptions(TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(1)),
            out _);

        if (result.Status != PowerShellFiniteOperationStatus.Active)
        {
            throw new InvalidOperationException("The finite payload source was not usable.");
        }
    }
}
"@ | Set-Content -Path (Join-Path $finitePayloadDirectory 'Program.cs') -Encoding utf8
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('restore', $finitePayloadProject, '--configfile', $nugetConfig)
    Assert-RestoredPackageMatchesInspectedNupkg -NugetCache $nugetCache -PackageId $packageId -PackageVersion $PackageVersion -InspectedPackagePath $package.FullName
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('build', $finitePayloadProject, '--no-restore', '-c', $Configuration)

    $consumerDirectory = Join-Path $workspace 'consumer'
    New-Item -Path $consumerDirectory -ItemType Directory -Force | Out-Null
    $consumerProject = Join-Path $consumerDirectory 'FfiPackageConsumer.csproj'
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <SelfContained>true</SelfContained>
    <PublishAot>true</PublishAot>
    <InvariantGlobalization>true</InvariantGlobalization>
    <DevolutionsMultiPwshSdkEnabled>true</DevolutionsMultiPwshSdkEnabled>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="$packageId" Version="$PackageVersion" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $consumerProject -Encoding utf8
    @"
#nullable enable
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Devolutions.PowerShell.Ffi;

void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

void VerifyCopiedValueReaders()
{
    Guid expectedGuid = Guid.Parse("3e0a9e49-2cc4-44d0-b4ce-65fc29f5d8b1");
    Uri expectedUri = new("https://example.test/sdk");
    DateTime expectedDateTime = new(2026, 7, 18, 10, 30, 0, DateTimeKind.Utc);
    DateTimeOffset expectedDateTimeOffset = new(2026, 7, 18, 10, 30, 0, TimeSpan.FromHours(-4));
    TimeSpan expectedTimeSpan = TimeSpan.FromTicks(-123456789);
    PowerShellValue bytes = PowerShellValue.Bytes(new byte[] { 1, 2, 3 });
    PowerShellValue value = PowerShellValue.PropertyBag(new[]
    {
        new KeyValuePair<string, PowerShellValue>("Text", PowerShellValue.String("copied-reader")),
        new KeyValuePair<string, PowerShellValue>("Switch", PowerShellValue.Switch()),
        new KeyValuePair<string, PowerShellValue>("Boolean", PowerShellValue.Boolean(true)),
        new KeyValuePair<string, PowerShellValue>("Signed", PowerShellValue.SignedInteger(-42)),
        new KeyValuePair<string, PowerShellValue>("Unsigned", PowerShellValue.UnsignedInteger(42)),
        new KeyValuePair<string, PowerShellValue>("Double", PowerShellValue.Double(1.5)),
        new KeyValuePair<string, PowerShellValue>("Decimal", PowerShellValue.Decimal(3.25m)),
        new KeyValuePair<string, PowerShellValue>("Bytes", bytes),
        new KeyValuePair<string, PowerShellValue>("DateTime", PowerShellValue.DateTime(expectedDateTime)),
        new KeyValuePair<string, PowerShellValue>("DateTimeOffset", PowerShellValue.DateTimeOffset(expectedDateTimeOffset)),
        new KeyValuePair<string, PowerShellValue>("TimeSpan", PowerShellValue.From(expectedTimeSpan)),
        new KeyValuePair<string, PowerShellValue>("Guid", PowerShellValue.Guid(expectedGuid)),
        new KeyValuePair<string, PowerShellValue>("Uri", PowerShellValue.Uri(expectedUri)),
        new KeyValuePair<string, PowerShellValue>(
            "Array",
            PowerShellValue.Array(new[]
            {
                PowerShellValue.SignedInteger(1),
                PowerShellValue.String("two"),
            })),
    });

    Require(
        PowerShellValue.Null.IsNull &&
        value.GetPropertyBag().Count == 14 &&
        value.TryGetProperty("text", out PowerShellValue? text) &&
        text!.TryGetString(out string? textValue) &&
        textValue == "copied-reader" &&
        value.TryGetProperty("Switch", out PowerShellValue? switchValue) &&
        switchValue!.TryGetSwitch(out bool switchPresent) &&
        switchPresent &&
        value.TryGetProperty("Boolean", out PowerShellValue? booleanValue) &&
        booleanValue!.TryGetBoolean(out bool boolean) &&
        boolean &&
        value.TryGetProperty("Signed", out PowerShellValue? signedValue) &&
        signedValue!.TryGetSignedInteger(out long signed) &&
        signed == -42 &&
        value.TryGetProperty("Unsigned", out PowerShellValue? unsignedValue) &&
        unsignedValue!.TryGetUnsignedInteger(out ulong unsigned) &&
        unsigned == 42 &&
        value.TryGetProperty("Double", out PowerShellValue? doubleValue) &&
        doubleValue!.TryGetDouble(out double doubleResult) &&
        doubleResult == 1.5 &&
        value.TryGetProperty("Decimal", out PowerShellValue? decimalValue) &&
        decimalValue!.TryGetDecimal(out decimal decimalResult) &&
        decimalResult == 3.25m &&
        value.TryGetProperty("Bytes", out PowerShellValue? bytesValue) &&
        bytesValue!.TryGetBytes(out byte[]? byteResult) &&
        byteResult!.SequenceEqual(new byte[] { 1, 2, 3 }) &&
        value.TryGetProperty("DateTime", out PowerShellValue? dateTimeValue) &&
        dateTimeValue!.TryGetDateTime(out DateTime dateTimeResult) &&
        dateTimeResult == expectedDateTime &&
        value.TryGetProperty("DateTimeOffset", out PowerShellValue? dateTimeOffsetValue) &&
        dateTimeOffsetValue!.TryGetDateTimeOffset(out DateTimeOffset dateTimeOffsetResult) &&
        dateTimeOffsetResult == expectedDateTimeOffset &&
        value.TryGetProperty("TimeSpan", out PowerShellValue? timeSpanValue) &&
        timeSpanValue!.Kind == PowerShellValueKind.TimeSpan &&
        timeSpanValue.TryGetTimeSpan(out TimeSpan timeSpanResult) &&
        timeSpanResult.Ticks == expectedTimeSpan.Ticks &&
        value.TryGetProperty("Guid", out PowerShellValue? guidValue) &&
        guidValue!.TryGetGuid(out Guid guidResult) &&
        guidResult == expectedGuid &&
        value.TryGetProperty("Uri", out PowerShellValue? uriValue) &&
        uriValue!.TryGetUri(out Uri? uriResult) &&
        uriResult == expectedUri &&
        value.TryGetProperty("Array", out PowerShellValue? arrayValue) &&
        arrayValue!.GetArray().Count == 2 &&
        arrayValue.GetArray()[0].TryGetSignedInteger(out long firstArrayValue) &&
        firstArrayValue == 1 &&
        !value.TryGetProperty("missing", out _) &&
        !value.TryGetString(out _),
        "Copied PowerShellValue readers did not preserve the documented DTO graph.");

    bool rejectedArrayRead = false;
    try
    {
        _ = value.GetArray();
    }
    catch (InvalidOperationException)
    {
        rejectedArrayRead = true;
    }
    Require(rejectedArrayRead, "Property bags must not be read as arrays.");
}

void VerifyTimeSpanNativeManagedCopies()
{
    TimeSpan expected = TimeSpan.FromTicks(-123456789);
    using PowerShell command = PowerShell.Create();
    PowerShellInvocationResult result = command
        .AddScript("param([TimeSpan]`$Value) if (`$Value.Ticks -ne -123456789) { throw 'TimeSpan parameter ticks changed' }; `$Value")
        .AddParameter("Value", PowerShellValue.TimeSpan(expected))
        .InvokeWithDiagnostics();
    PowerShellObjectSnapshot output = result.Output.Records.Single();
    PowerShellValue? scalar = output.ScalarValue;
    Require(
        result.Errors.Records.Count == 0 &&
        scalar?.Kind == PowerShellValueKind.TimeSpan &&
        scalar.TryGetTimeSpan(out TimeSpan copied) &&
        copied.Ticks == expected.Ticks,
        "Managed/native TimeSpan copies did not preserve the tagged kind and exact ticks.");
}

void VerifyCopiedExceptionHResultAndSnapshotSerialization(
    PowerShellInvocationResult copiedResult,
    byte[] serialized,
    PowerShellInvocationResult restoredResult)
{
    const int ExpectedHResult = -2128394905;
    Require(
        copiedResult.Errors.Records.Single().ExceptionHResult == ExpectedHResult,
        "The copied PowerShell invocation error did not preserve Exception.HResult.");
    Require(
        Encoding.UTF8.GetString(serialized).Contains(
            "\"exceptionHResult\":-2128394905",
            StringComparison.Ordinal) &&
        Encoding.UTF8.GetString(serialized).StartsWith("{\"version\":2,", StringComparison.Ordinal),
        "Snapshot JSON did not expose the copied exception HResult in format v2.");
    Require(
        restoredResult.Errors.Records.Single().ExceptionHResult == ExpectedHResult,
        "Snapshot deserialization did not restore the copied exception HResult.");
    string legacyJson = Encoding.UTF8.GetString(serialized)
        .Replace("\"version\":2,", "\"version\":1,", StringComparison.Ordinal)
        .Replace("\"exceptionHResult\":-2128394905,", string.Empty, StringComparison.Ordinal);
    PowerShellInvocationResult legacyResult = PowerShellSnapshotSerializer.Deserialize(Encoding.UTF8.GetBytes(legacyJson));
    Require(
        legacyResult.Errors.Records.Single().ExceptionHResult == 0 &&
        legacyResult.Errors.Records.Single().Message == copiedResult.Errors.Records.Single().Message &&
        legacyResult.Output.TotalRecordCount == copiedResult.Output.TotalRecordCount,
        "NativeAOT snapshot deserialization did not preserve legacy v1 data with an unavailable HResult.");
}

void VerifyScriptParameterMetadata(PowerShellRuntime runtime)
{
    PowerShellScriptParseResult metadata = runtime.ParseScriptParameters(
        "param([Alias('N')][Parameter(Mandatory = `$true, HelpMessage = 'help', ParameterSetName = 'ByName', Position = 1, ValueFromPipelineByPropertyName = `$true)][Description('description')][ValidateSet('one', 'two')][ValidatePattern('^[a-z]+`$')][ValidateRange(1, 10)][string]`$Name = 'default')");
    PowerShellScriptParameterMetadata parameter = metadata.Parameters.Single();
    Require(
        !metadata.HasErrors &&
        parameter.Name == "Name" &&
        (parameter.TypeName == "string" || parameter.TypeName == "System.String") &&
        parameter.DefaultValueExpression == "'default'" &&
        parameter.IsMandatory &&
        parameter.Description == "description" &&
        parameter.HelpMessage == "help" &&
        parameter.ValidateSetValues.SequenceEqual(new[] { "one", "two" }) &&
        parameter.Aliases.SequenceEqual(new[] { "N" }) &&
        parameter.ParameterSets.Count == 1 &&
        parameter.ParameterSets[0].Name == "ByName" &&
        parameter.ParameterSets[0].Position == 1 &&
        parameter.ParameterSets[0].ValueFromPipelineByPropertyName &&
        parameter.Validations.Any(validation =>
            validation.Name == "ValidatePattern" &&
            validation.Arguments.SequenceEqual(new[] { "^[a-z]+$" })) &&
        parameter.Validations.Any(validation =>
            validation.Name == "ValidateRange" &&
            validation.Arguments.SequenceEqual(new[] { "1", "10" })),
        "Copied script parameter metadata did not preserve the declared parameter contract.");

    PowerShellScriptParseResult nonExecuting = runtime.ParseScriptParameters(
        "param([string]`$Value) throw 'caller source must not execute'");
    Require(
        !nonExecuting.HasErrors &&
        nonExecuting.Parameters.Count == 1 &&
        nonExecuting.Parameters[0].Name == "Value",
        "Script metadata parsing executed caller-provided source.");

    PowerShellScriptParseResult invalid = runtime.ParseScriptParameters("param([string]`$Name");
    Require(
        invalid.HasErrors &&
        invalid.Parameters.Count == 0 &&
        invalid.Errors.Count != 0 &&
        invalid.Errors[0].EndOffset >= invalid.Errors[0].StartOffset,
        "Script parser errors were not returned as bounded copied DTOs.");

    try
    {
        _ = runtime.ParseScriptParameters(new string('x', 64 * 1024 + 1));
        throw new InvalidOperationException("Oversized script metadata input was accepted.");
    }
    catch (ArgumentOutOfRangeException)
    {
    }
}

void VerifyProgressUpdate()
{
    PowerShellProgressUpdate progress = PowerShellHostInteraction.ParseProgressUpdate(
        PowerShellValue.PropertyBag(
        [
            new KeyValuePair<string, PowerShellValue>("ActivityId", PowerShellValue.SignedInteger(7)),
            new KeyValuePair<string, PowerShellValue>("ParentActivityId", PowerShellValue.SignedInteger(-1)),
            new KeyValuePair<string, PowerShellValue>("Activity", PowerShellValue.String("Deploying")),
            new KeyValuePair<string, PowerShellValue>("StatusDescription", PowerShellValue.String("Loading payload")),
            new KeyValuePair<string, PowerShellValue>("CurrentOperation", PowerShellValue.String("Copy")),
            new KeyValuePair<string, PowerShellValue>("PercentComplete", PowerShellValue.SignedInteger(50)),
            new KeyValuePair<string, PowerShellValue>("SecondsRemaining", PowerShellValue.SignedInteger(12)),
            new KeyValuePair<string, PowerShellValue>("IsCompleted", PowerShellValue.Boolean(false)),
        ]));
    Require(
        progress.ActivityId == 7 &&
        progress.ParentActivityId == -1 &&
        progress.Activity == "Deploying" &&
        progress.PercentComplete == 50 &&
        progress.SecondsRemaining == 12 &&
        !progress.IsCompleted,
        "Typed host progress validation did not preserve the copied progress update.");

    try
    {
        _ = PowerShellHostInteraction.ParseProgressUpdate(
            PowerShellValue.PropertyBag(
            [
                new KeyValuePair<string, PowerShellValue>("ActivityId", PowerShellValue.SignedInteger(1)),
                new KeyValuePair<string, PowerShellValue>("Activity", PowerShellValue.String("Invalid")),
                new KeyValuePair<string, PowerShellValue>("PercentComplete", PowerShellValue.SignedInteger(101)),
            ]));
        throw new InvalidOperationException("Out-of-range host progress was accepted.");
    }
    catch (ArgumentException)
    {
    }
}

void RequireProjectionFailure(
    Action projection,
    PowerShellCompleteResultProjectionFailure expectedFailure,
    string description)
{
    try
    {
        projection();
        throw new InvalidOperationException("Projection unexpectedly succeeded: " + description);
    }
    catch (PowerShellCompleteResultProjectionException exception)
        when (exception.Failure == expectedFailure)
    {
    }
}

PowerShellInvocationResult InvokeProjectionScript(
    PowerShellRuntime runtime,
    string script,
    string description)
{
    using PowerShell builder = CreatePowerShellWhenAvailable(runtime, description);
    return builder.AddScript(script).Invoke();
}

void VerifyRuntimeDiagnostics(PowerShellRuntime runtime, string payloadDirectory)
{
    PowerShellRuntimeDiagnosticReport report = runtime.Diagnostics;
    Require(
        Path.IsPathFullyQualified(report.PayloadDirectory) &&
        Directory.Exists(report.PayloadDirectory) &&
        File.Exists(Path.Combine(report.PayloadDirectory, "pwsh.dll")) &&
        report.BindingsAbiVersion == 1 &&
        report.PayloadTableShape == PowerShellPayloadTableShape.V1 &&
        report.PayloadTableSlotCount != 0 &&
        report.PayloadTableSize >= (nuint)report.PayloadTableSlotCount * (nuint)IntPtr.Size &&
        report.FeatureFlags == runtime.FeatureFlags &&
        (report.FeatureFlags & (1UL << 24)) != 0 &&
        (report.FeatureFlags & (1UL << 32)) != 0 &&
        report.RegisteredLiveObjectContractPacks.Count == 0 &&
        (report.PowerShellFileVersion is null ||
            (!string.IsNullOrWhiteSpace(report.PowerShellFileVersion) &&
             report.PowerShellFileVersion.Length <= 128)) &&
        typeof(PowerShellRuntimeDiagnosticReport)
            .GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .All(property => property.SetMethod is null) &&
        typeof(PowerShellLiveObjectContractPackIdentity)
            .GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .All(property => property.SetMethod is null),
        "Runtime diagnostics did not expose the documented immutable, descriptive payload facts.");
    PowerShellRemoteProviderDiagnostic provider = report.ManagedWsManProvider;
    Require(
        provider.ProviderId == PowerShellRemoteProviderDiagnostic.DevolutionsManagedWsManProviderId &&
        provider.Status == PowerShellRemoteProviderStatus.Unavailable &&
        provider.UnavailableReason == PowerShellRemoteProviderUnavailableReason.$ExpectedRemoteProviderUnavailableReason &&
        provider.Capabilities == PowerShellRemoteProviderCapability.None &&
        typeof(PowerShellRemoteProviderDiagnostic)
            .GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .All(property => property.SetMethod is null),
        "PowerShell did not report the expected managed WSMan provider availability state.");
    // The report exposes the runtime's canonicalized payload directory, which on Windows
    // is extended-length prefixed and therefore is not string-equal to the activation
    // argument. It must still resolve to the same directory.
    static string NormalizePayloadDirectory(string path)
    {
        string trimmed = path.StartsWith(@"\\?\", StringComparison.Ordinal) ? path[4..] : path;
        return Path.TrimEndingDirectorySeparator(Path.GetFullPath(trimmed));
    }

    Require(
        NormalizePayloadDirectory(report.PayloadDirectory)
            .Equals(NormalizePayloadDirectory(payloadDirectory), StringComparison.OrdinalIgnoreCase),
        $"Runtime diagnostics reported payload directory '{report.PayloadDirectory}' which does not resolve to '{payloadDirectory}'.");
    Console.WriteLine(
        "FFI package consumer PowerShell file version: " +
        (report.PowerShellFileVersion ?? "unreported"));
}

void VerifyCompleteResultProjection(PowerShellRuntime runtime)
{
    const string DtoScript =
        "[pscustomobject]@{ '`$version' = [uint64]1; Name = 'generated-projection'; Count = [int64]7 }";
    PowerShellInvocationResult validResult = InvokeProjectionScript(
        runtime,
        DtoScript,
        "the generated DTO result builder");
    PowerShellObjectSnapshot validRecord = validResult.Output.Records.Single();
    Require(
        validRecord.PropertyBag is not null,
        $"The generated DTO source result did not retain a property bag ({validRecord.PropertyEntryCount} retained, {validRecord.DroppedPropertyEntryCount} dropped).");
    PackageProjectionDto invocationDto = PowerShellCompleteResultProjection.Read(
        validResult,
        PackageProjectionDtoPowerShellDtoProjection.Read);
    Require(
        invocationDto.Name == "generated-projection" &&
        invocationDto.Count == 7,
        "The complete invocation result was not projected through the explicit generated DTO mapper.");

    RequireProjectionFailure(
        () => _ = PowerShellCompleteResultProjection.Read(
            InvokeProjectionScript(runtime, string.Empty, "the zero-result projection builder"),
            PackageProjectionDtoPowerShellDtoProjection.Read),
        PowerShellCompleteResultProjectionFailure.ZeroResults,
        "zero results");
    RequireProjectionFailure(
        () => _ = PowerShellCompleteResultProjection.Read(
            InvokeProjectionScript(
                runtime,
                DtoScript + "; " + DtoScript,
                "the multiple-result projection builder"),
            PackageProjectionDtoPowerShellDtoProjection.Read),
        PowerShellCompleteResultProjectionFailure.MultipleResults,
        "multiple results");
    RequireProjectionFailure(
        () => _ = PowerShellCompleteResultProjection.Read(
            InvokeProjectionScript(
                runtime,
                "[pscustomobject]@{ '`$version' = [uint64]1; Name = 'truncated'; Count = [int64]7; Nested = @{ Value = 1 } }",
                "the truncated-result projection builder"),
            PackageProjectionDtoPowerShellDtoProjection.Read),
        PowerShellCompleteResultProjectionFailure.IncompleteOrTruncated,
        "truncated result");
    RequireProjectionFailure(
        () => _ = PowerShellCompleteResultProjection.Read(
            InvokeProjectionScript(
                runtime,
                "[pscustomobject]@{ '`$version' = [uint64]1; Name = 'mapper-failure' }",
                "the mapper-failure projection builder"),
            PackageProjectionDtoPowerShellDtoProjection.Read),
        PowerShellCompleteResultProjectionFailure.MapperFailure,
        "mapper failure");

    using (PowerShell typedBuilder = CreatePowerShellWhenAvailable(runtime, "the typed DTO result builder"))
    using (PowerShellTypedResultInvocation typed = typedBuilder
        .AddScript(DtoScript)
        .BeginTypedResultInvocation())
    {
        PowerShellValuePage first = WaitForTypedPage(
            typed,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the typed DTO result page");
        PowerShellValuePage complete = WaitForTypedPage(
            typed,
            acknowledgedThrough: first.NextSequence,
            maximumRecords: 1,
            page => page.IsComplete,
            "the typed DTO completion page");
        PackageProjectionDto typedDto = PowerShellCompleteResultProjection.Read(
            new[] { first, complete },
            PackageProjectionDtoPowerShellDtoProjection.Read);
        Require(
            typedDto.Name == "generated-projection" &&
            typedDto.Count == 7,
            "The complete typed result sequence was not projected through the explicit generated DTO mapper.");
    }

    using (PowerShell incompleteBuilder = CreatePowerShellWhenAvailable(runtime, "the incomplete typed DTO result builder"))
    using (PowerShellTypedResultInvocation incomplete = incompleteBuilder
        .AddScript(DtoScript)
        .BeginTypedResultInvocation())
    {
        PowerShellValuePage partial = WaitForTypedPage(
            incomplete,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the incomplete typed DTO result page");
        RequireProjectionFailure(
            () => _ = PowerShellCompleteResultProjection.Read(
                new[] { partial },
                PackageProjectionDtoPowerShellDtoProjection.Read),
            PowerShellCompleteResultProjectionFailure.IncompleteOrTruncated,
            "incomplete typed result sequence");
    }

    using (PowerShell observedBuilder = CreatePowerShellWhenAvailable(runtime, "the observed DTO result builder"))
    using (PowerShellObservedInvocation observed = observedBuilder
        .AddScript(DtoScript)
        .BeginObservedInvocation())
    {
        PowerShellValuePage first = WaitForObservedResultPage(
            observed,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the observed DTO result page");
        var resultPages = new List<PowerShellValuePage> { first };
        ulong resultAcknowledgement = first.NextSequence;
        ulong diagnosticAcknowledgement = 0;
        DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        PowerShellValuePage resultPage = first;
        PowerShellObservedDiagnosticPage diagnosticPage = null!;
        while (DateTime.UtcNow < deadline)
        {
            resultPage = observed.ReadResults(resultAcknowledgement, maximumRecords: 1);
            resultAcknowledgement = resultPage.NextSequence;
            resultPages.Add(resultPage);
            diagnosticPage = observed.ReadDiagnostics(diagnosticAcknowledgement, maximumRecords: 1);
            diagnosticAcknowledgement = diagnosticPage.NextSequence;
            if (resultPage.IsComplete && diagnosticPage.IsComplete)
            {
                break;
            }

            Thread.Sleep(10);
        }

        Require(
            resultPage.IsComplete &&
            diagnosticPage is not null &&
            diagnosticPage.IsComplete,
            "The observed DTO result and diagnostics did not reach complete terminal pages.");
        PackageProjectionDto observedDto = PowerShellCompleteResultProjection.Read(
            resultPages,
            PackageProjectionDtoPowerShellDtoProjection.Read);
        Require(
            observedDto.Name == "generated-projection" &&
            observedDto.Count == 7,
            "The complete observed result sequence was not projected through the explicit generated DTO mapper.");
    }

    using (PowerShell presentationBuilder = CreatePowerShellWhenAvailable(runtime, "the observed presentation builder"))
    using (PowerShellObservedInvocation presentation = presentationBuilder
        .AddScript(@"
            Write-Output 'package-presentation-result'
            Write-Information 'package-presentation-text' -InformationAction Continue
            Write-Progress -Id 17 -ParentId 5 -Activity 'package-progress' -Status 'running' -CurrentOperation 'packaging' -PercentComplete 58 -SecondsRemaining 12
        ")
        .BeginObservedInvocation(new PowerShellObservedInvocationOptions(
            maximumBufferedResultRecords: 4,
            maximumResultPageRecords: 2,
            maximumBufferedDiagnosticRecords: 4,
            maximumDiagnosticPageRecords: 2)))
    {
        PowerShellObservedTranscript transcript = presentation.CreateTranscript();
        var records = new List<PowerShellObservedPresentationRecord>();
        PowerShellValuePage resultPage = null!;
        PowerShellObservedPresentationPage presentationPage = null!;
        DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (DateTime.UtcNow < deadline)
        {
            resultPage = transcript.ReadResults();
            Require(
                ReferenceEquals(resultPage, transcript.ReadResults()),
                "The observed transcript did not retain an uncommitted result page.");
            presentationPage = transcript.ReadPresentation();
            Require(
                ReferenceEquals(presentationPage, transcript.ReadPresentation()),
                "The observed transcript did not retain an uncommitted presentation page.");
            records.AddRange(presentationPage.Records);
            ulong presentationAcknowledgement = transcript.PresentationAcknowledgedThrough;
            transcript.CommitResults(resultPage);
            Require(
                transcript.ResultAcknowledgedThrough == resultPage.NextSequence &&
                transcript.PresentationAcknowledgedThrough == presentationAcknowledgement,
                "The observed transcript did not commit result and presentation cursors independently.");
            bool rejectedStaleResultPage = false;
            try
            {
                transcript.CommitResults(resultPage);
            }
            catch (InvalidOperationException)
            {
                rejectedStaleResultPage = true;
            }

            Require(rejectedStaleResultPage, "The observed transcript accepted a stale result page.");
            transcript.CommitPresentation(presentationPage);
            bool rejectedStalePresentationPage = false;
            try
            {
                transcript.CommitPresentation(presentationPage);
            }
            catch (InvalidOperationException)
            {
                rejectedStalePresentationPage = true;
            }

            Require(rejectedStalePresentationPage, "The observed transcript accepted a stale presentation page.");
            if (resultPage.IsComplete && presentationPage.IsComplete)
            {
                break;
            }

            Thread.Sleep(10);
        }

        PowerShellObservedPresentationRecord? progress = records.FirstOrDefault(
            static record => record.Progress is { ActivityId: 17, IsCompleted: false });
        bool hasInformation = records.Any(static record =>
            record.Stream == PowerShellStreamKind.Information &&
            record.Text.Contains("package-presentation-text", StringComparison.Ordinal));
        bool hasProgress = progress?.Progress is
        {
            ActivityId: 17,
            ParentActivityId: 5,
            Activity: "package-progress",
            StatusDescription: "running",
            CurrentOperation: "packaging",
            PercentComplete: 58,
            SecondsRemaining: 12,
            IsCompleted: false,
        };
        Require(
            resultPage is not null &&
            presentationPage is not null &&
            resultPage.IsComplete &&
            presentationPage.IsComplete &&
            resultPage.TotalRecordCount == 1 &&
            resultPage.DroppedRecordCount == 0 &&
            !resultPage.IsTruncated &&
            presentationPage.DroppedRecordCount == 0 &&
            !presentationPage.IsTruncated &&
            transcript.ResultAcknowledgedThrough == resultPage.NextSequence &&
            transcript.PresentationAcknowledgedThrough == presentationPage.NextSequence &&
            hasInformation &&
            hasProgress,
            string.Format(
                "The package-only NativeAOT consumer did not preserve bounded text and typed progress presentation (information: {0}; progress: {1}; records: {2}).",
                hasInformation,
                hasProgress,
                records.Count));
    }
}

async Task VerifyRecipesSchemasAndPoliciesAsync(PowerShellRuntime runtime)
{
    var outputSchema = new PowerShellResultSchema(
        minimumOutputRecords: 1,
        maximumOutputRecords: 1,
        allowedScalarKinds: [PowerShellValueKind.String]);
    PowerShellInvocationResult commandResult = runtime.Invoke(
        new PowerShellCommandRecipe(
            "Write-Output",
            [new KeyValuePair<string, PowerShellValue>("InputObject", PowerShellValue.String("recipe-output"))],
            outputSchema));
    Require(
        commandResult.Output.Records.Count == 1 &&
        commandResult.Output.Records[0].DisplayText == "recipe-output",
        "A bounded command recipe did not produce its declared copied result.");

    PowerShellInvocationResult objectResult = runtime.Invoke(
        new PowerShellScriptRecipe("[pscustomobject]@{ Name = 'snapshot'; Count = 2 }"));
    PowerShellObjectSnapshot objectSnapshot = objectResult.Output.Records.Single();
    IReadOnlyDictionary<string, PowerShellValue> properties = PowerShellSnapshotReader.GetCompleteProperties(objectSnapshot);
    PowerShellDisplaySnapshot display = PowerShellSnapshotReader.CreateDisplaySnapshot(objectResult);
    Require(
        properties["Name"].TryGetString(out string? name) &&
        name == "snapshot" &&
        properties["Count"].TryGetSignedInteger(out long count) &&
        count == 2 &&
        display.IsComplete &&
        display.Output.Single() == objectSnapshot.DisplayText,
        "Snapshot readers did not preserve a complete copied property bag and display DTO.");

    try
    {
        _ = runtime.Invoke(
            new PowerShellScriptRecipe(
                "'wrong-scalar'",
                new PowerShellResultSchema(allowedScalarKinds: [PowerShellValueKind.SignedInteger])));
        throw new InvalidOperationException("A result schema accepted an incompatible copied scalar.");
    }
    catch (InvalidOperationException)
    {
    }
    try
    {
        _ = new PowerShellResultSchema(allowedScalarKinds: [PowerShellValueKind.PropertyBag]);
        throw new InvalidOperationException("A result schema accepted a non-scalar value kind.");
    }
    catch (ArgumentException)
    {
    }

    var policy = new PowerShellCommandPolicy(allowedCommands: ["Write-Output"]);
    try
    {
        _ = runtime.Invoke(new PowerShellCommandRecipe("Get-Date"), policy);
        throw new InvalidOperationException("An advisory command policy accepted a non-allowlisted command.");
    }
    catch (InvalidOperationException)
    {
    }
    try
    {
        _ = runtime.Invoke(new PowerShellScriptRecipe("'blocked script'"), policy);
        throw new InvalidOperationException("An advisory command policy accepted script source by default.");
    }
    catch (InvalidOperationException)
    {
    }
    try
    {
        _ = await runtime.InvokeAsync(
            new PowerShellScriptRecipe(
                "Start-Sleep -Seconds 5",
                timeout: TimeSpan.FromMilliseconds(50)));
        throw new InvalidOperationException("A timed recipe returned a successful result.");
    }
    catch (OperationCanceledException)
    {
    }
}

async Task VerifyCancellationAndDisposeAsync()
{
    using (var cancellation = new CancellationTokenSource())
    using (PowerShell cancellationBuilder = PowerShell.Create())
    {
        Task<PowerShellInvocationResult> invocation = cancellationBuilder
            .AddScript("1..50 | ForEach-Object { Start-Sleep -Milliseconds 100; Write-Output `$_ }")
            .InvokeAsync(cancellation.Token);
        await Task.Delay(75);
        cancellation.Cancel();
        try
        {
            await invocation;
            throw new InvalidOperationException("Cancelled InvokeAsync returned a successful partial result.");
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested)
        {
        }
    }

    using (PowerShell stopBuilder = PowerShell.Create())
    using (PowerShellInvocationOperation operation = stopBuilder
        .AddScript("Start-Sleep -Seconds 5; 'must-not-be-returned'")
        .BeginInvoke())
    {
        operation.Stop();
        operation.Stop();
        PowerShellInvocationOperationStatus status = operation.Wait(TimeSpan.FromSeconds(5));
        Require(
            status.State == PowerShellOperationState.Cancelled &&
            status.TerminalStatus == PowerShellFfiStatus.OperationCancelled,
            "Repeated Stop did not reach the deterministic cancelled terminal state.");
        try
        {
            operation.GetResult();
            throw new InvalidOperationException("Cancelled operation exposed a successful result.");
        }
        catch (PowerShellFfiException exception)
            when (exception.Status == PowerShellFfiStatus.OperationCancelled)
        {
        }

        operation.Dispose();
        operation.Dispose();
    }
}

async Task VerifySafeHandleDisposeRacesAsync()
{
    for (int iteration = 0; iteration < 4; iteration++)
    {
        PowerShell builder = PowerShell.Create()
            .AddScript("Start-Sleep -Milliseconds 150; 'safe-handle-lease'");
        using var enteredInvocation = new ManualResetEventSlim(false);
        Task<PowerShellInvocationResult?> invoke = Task.Run(() =>
        {
            enteredInvocation.Set();
            try
            {
                return builder.Invoke();
            }
            catch (ObjectDisposedException)
            {
                return null;
            }
        });
        Task dispose = Task.Run(() =>
        {
            enteredInvocation.Wait();
            Thread.Sleep(10);
            builder.Dispose();
            builder.Dispose();
        });

        PowerShellInvocationResult? result = await invoke;
        await dispose;
        if (result is not null)
        {
            Require(
                result.Output.Records.Count == 1 &&
                result.Output.Records[0].DisplayText == "safe-handle-lease",
                "An in-flight SafeHandle lease returned an unexpected result.");
        }
    }
}

PowerShellValuePage WaitForTypedPage(
    PowerShellTypedResultInvocation invocation,
    ulong acknowledgedThrough,
    int maximumRecords,
    Func<PowerShellValuePage, bool> predicate,
    string description)
{
    DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (DateTime.UtcNow < deadline)
    {
        PowerShellValuePage page = invocation.Read(acknowledgedThrough, maximumRecords);
        if (predicate(page))
        {
            return page;
        }

        Thread.Sleep(10);
    }

    throw new TimeoutException("Timed out waiting for " + description + ".");
}

PowerShellValuePage WaitForObservedResultPage(
    PowerShellObservedInvocation invocation,
    ulong acknowledgedThrough,
    int maximumRecords,
    Func<PowerShellValuePage, bool> predicate,
    string description)
{
    DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (DateTime.UtcNow < deadline)
    {
        PowerShellValuePage page = invocation.ReadResults(acknowledgedThrough, maximumRecords);
        if (predicate(page))
        {
            return page;
        }

        Thread.Sleep(10);
    }

    throw new TimeoutException("Timed out waiting for " + description + ".");
}

PowerShellObservedDiagnosticPage WaitForObservedDiagnosticPage(
    PowerShellObservedInvocation invocation,
    ulong acknowledgedThrough,
    int maximumRecords,
    Func<PowerShellObservedDiagnosticPage, bool> predicate,
    string description)
{
    DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (DateTime.UtcNow < deadline)
    {
        PowerShellObservedDiagnosticPage page = invocation.ReadDiagnostics(acknowledgedThrough, maximumRecords);
        if (predicate(page))
        {
            return page;
        }

        Thread.Sleep(10);
    }

    throw new TimeoutException("Timed out waiting for " + description + ".");
}

PowerShell CreatePowerShellWhenAvailable(PowerShellRuntime runtime, string description)
{
    DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
    while (DateTime.UtcNow < deadline)
    {
        try
        {
            return runtime.Create();
        }
        catch (PowerShellFfiException exception)
            when (exception.Status == PowerShellFfiStatus.Backpressure)
        {
            Thread.Sleep(10);
        }
    }

    throw new TimeoutException("Timed out waiting for the typed result pipeline to release " + description + ".");
}

async Task VerifyTypedResultPagingAsync(PowerShellRuntime runtime)
{
    Require(
        (runtime.FeatureFlags & (1UL << 21)) != 0,
        "The packaged NativeAOT consumer did not negotiate typed result paging.");

    using (PowerShell builder = CreatePowerShellWhenAvailable(runtime, "the first typed result builder"))
    using (PowerShellTypedResultInvocation invocation = builder
        .AddScript("1, 2, [pscustomobject]@{ Name = 'third'; Count = 3 }")
        .BeginTypedResultInvocation(new PowerShellValuePagerOptions(
            maximumBufferedRecords: 2,
            maximumPageRecords: 2)))
    {
        PowerShellValuePage firstPage = WaitForTypedPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 2,
            page => page.Records.Count == 2,
            "the backpressure-limited first typed result page");
        Require(
            !firstPage.IsTerminal &&
            firstPage.TotalRecordCount == 2 &&
            firstPage.NextSequence == 2 &&
            firstPage.Records[0].Value.TryGetSignedInteger(out long firstValue) &&
            firstValue == 1 &&
            firstPage.Records[1].Value.TryGetSignedInteger(out long secondValue) &&
            secondValue == 2,
            "Typed result paging did not retain the first bounded page before acknowledgement.");

        PowerShellValuePage replayPage = invocation.Read(acknowledgedThrough: 0, maximumRecords: 2);
        Require(
            replayPage.AcknowledgedSequence == 0 &&
            replayPage.Records.Select(record => record.Sequence).SequenceEqual(firstPage.Records.Select(record => record.Sequence)),
            "Typed result paging implicitly acknowledged records without the caller cursor.");

        PowerShellValuePage secondPage = WaitForTypedPage(
            invocation,
            acknowledgedThrough: firstPage.NextSequence,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the record released by typed result acknowledgement");
        Require(
            secondPage.AcknowledgedSequence == firstPage.NextSequence &&
            secondPage.TotalRecordCount == 3 &&
            secondPage.Records[0].Value.TryGetProperty("Name", out PowerShellValue? name) &&
            name!.TryGetString(out string? nameText) &&
            nameText == "third" &&
            secondPage.Records[0].Value.TryGetProperty("Count", out PowerShellValue? count) &&
            count!.TryGetSignedInteger(out long countValue) &&
            countValue == 3,
            "Acknowledging the first typed page did not release the blocked copied property-bag result.");

        PowerShellValuePage completePage = WaitForTypedPage(
            invocation,
            acknowledgedThrough: secondPage.NextSequence,
            maximumRecords: 1,
            page => page.IsTerminal,
            "typed result completion");
        Require(
            completePage.IsComplete &&
            completePage.TerminalStatus == PowerShellFfiStatus.Success &&
            completePage.TotalRecordCount == 3 &&
            completePage.DroppedRecordCount == 0 &&
            !completePage.IsTruncated,
            "Typed result paging did not report fully acknowledged success.");
    }

    using (PowerShell unsupportedBuilder = CreatePowerShellWhenAvailable(runtime, "the unsupported result builder"))
    using (PowerShellTypedResultInvocation unsupportedInvocation = unsupportedBuilder
        .AddScript("Write-Output -NoEnumerate ([version]'1.2.3.4')")
        .BeginTypedResultInvocation())
    {
        PowerShellValuePage unsupportedPage = WaitForTypedPage(
            unsupportedInvocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.IsTerminal,
            "an unsupported typed result terminal state");
        Require(
            unsupportedPage.TerminalStatus == PowerShellFfiStatus.UnsupportedValue &&
            !unsupportedPage.IsComplete &&
            unsupportedPage.Records.Count == 0,
            "Unsupported typed PowerShell output was not surfaced as UnsupportedValue.");
    }

    using (PowerShell cancellationBuilder = CreatePowerShellWhenAvailable(runtime, "the cancellation builder"))
    using (PowerShellTypedResultInvocation cancellationInvocation = cancellationBuilder
        .AddScript("Start-Sleep -Seconds 5; 'must-not-complete'")
        .BeginTypedResultInvocation())
    {
        Thread.Sleep(50);
        cancellationInvocation.Stop();
        cancellationInvocation.Stop();
        PowerShellValuePage cancelledPage = WaitForTypedPage(
            cancellationInvocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.IsTerminal,
            "typed result cancellation");
        Require(
            cancelledPage.TerminalStatus == PowerShellFfiStatus.OperationCancelled &&
            !cancelledPage.IsComplete,
            "Stopping a typed result invocation did not produce the cancelled terminal state.");
    }

    for (int iteration = 0; iteration < 4; iteration++)
    {
        PowerShell lifecycleBuilder = CreatePowerShellWhenAvailable(runtime, "the lifecycle race builder")
            .AddScript("Start-Sleep -Milliseconds 150; 'typed-safe-handle-lease'");
        PowerShellTypedResultInvocation lifecycleInvocation = lifecycleBuilder.BeginTypedResultInvocation();
        using var readStarted = new ManualResetEventSlim(false);
        Task reader = Task.Run(() =>
        {
            readStarted.Set();
            try
            {
                for (int read = 0; read < 32; read++)
                {
                    _ = lifecycleInvocation.Read(0, 1);
                    Thread.Sleep(1);
                }
            }
            catch (ObjectDisposedException)
            {
            }
        });
        Task disposer = Task.Run(() =>
        {
            readStarted.Wait();
            Thread.Sleep(5);
            lifecycleInvocation.Dispose();
            lifecycleInvocation.Dispose();
        });
        await Task.WhenAll(reader, disposer).WaitAsync(TimeSpan.FromSeconds(10));
        lifecycleBuilder.Dispose();
    }

    using PowerShell availabilityCheck = CreatePowerShellWhenAvailable(runtime, "subsequent facade calls");
}

async Task VerifyObservedInvocationAsync(PowerShellRuntime runtime)
{
    Require(
        (runtime.FeatureFlags & (1UL << 22)) != 0,
        "The packaged NativeAOT consumer did not negotiate observed invocation support.");

    using (PowerShell builder = CreatePowerShellWhenAvailable(runtime, "the observed backpressure builder"))
    using (PowerShellObservedInvocation invocation = builder
        .AddScript("1; 2")
        .BeginObservedInvocation(new PowerShellObservedInvocationOptions(
            maximumBufferedResultRecords: 1,
            maximumResultPageRecords: 1,
            maximumBufferedDiagnosticRecords: 1,
            maximumDiagnosticPageRecords: 1)))
    {
        PowerShellValuePage firstResult = WaitForObservedResultPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the first observed result page");
        Require(
            firstResult.Records[0].Value.TryGetSignedInteger(out long firstValue) &&
            firstValue == 1 &&
            firstResult.TotalRecordCount == 1,
            "Observed results did not retain the first bounded copied value.");

        PowerShellValuePage replayedResult = invocation.ReadResults(acknowledgedThrough: 0, maximumRecords: 1);
        Require(
            replayedResult.Records.Select(record => record.Sequence)
                .SequenceEqual(firstResult.Records.Select(record => record.Sequence)),
            "Observed results implicitly acknowledged a result page.");

        PowerShellValuePage blockedByDiagnostics = invocation.ReadResults(
            acknowledgedThrough: firstResult.NextSequence,
            maximumRecords: 1);
        Require(
            blockedByDiagnostics.Records.Count == 0 &&
            blockedByDiagnostics.TotalRecordCount == 1,
            "A full observed diagnostic queue did not backpressure the same invocation.");

        PowerShellObservedDiagnosticPage firstDiagnostic = WaitForObservedDiagnosticPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the first observed diagnostic page");
        Require(
            firstDiagnostic.Records[0].Stream == PowerShellStreamKind.Output &&
            firstDiagnostic.Records[0].Text == "1",
            "Observed output diagnostics were not copied losslessly.");

        PowerShellObservedDiagnosticPage secondDiagnostic = WaitForObservedDiagnosticPage(
            invocation,
            acknowledgedThrough: firstDiagnostic.NextSequence,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the diagnostic released by acknowledgement");
        PowerShellValuePage secondResult = WaitForObservedResultPage(
            invocation,
            acknowledgedThrough: firstResult.NextSequence,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the result released by diagnostic acknowledgement");
        Require(
            secondResult.Records[0].Value.TryGetSignedInteger(out long secondValue) &&
            secondValue == 2 &&
            secondDiagnostic.Records[0].Text == "2",
            "Observed channel acknowledgement did not independently release the blocked producer.");

        PowerShellValuePage resultTerminal = WaitForObservedResultPage(
            invocation,
            acknowledgedThrough: secondResult.NextSequence,
            maximumRecords: 1,
            page => page.IsTerminal,
            "observed result terminal metadata");
        Require(
            !resultTerminal.IsComplete &&
            resultTerminal.TerminalStatus == PowerShellFfiStatus.Success,
            "Observed result acknowledgement completed before diagnostics were acknowledged.");
        PowerShellObservedDiagnosticPage diagnosticTerminal = WaitForObservedDiagnosticPage(
            invocation,
            acknowledgedThrough: secondDiagnostic.NextSequence,
            maximumRecords: 1,
            page => page.IsTerminal,
            "observed diagnostic terminal metadata");
        Require(
            diagnosticTerminal.IsComplete &&
            diagnosticTerminal.TerminalStatus == PowerShellFfiStatus.Success,
            "Observed invocation did not complete after both channels were acknowledged.");
    }

    using (PowerShell builder = CreatePowerShellWhenAvailable(runtime, "the observed diagnostics builder"))
    using (PowerShellObservedInvocation invocation = builder
        .AddScript(
            "Write-Output 'observed-output'; Write-Error 'observed-error' -ErrorAction Continue; " +
            "Write-Warning 'observed-warning'; Write-Verbose 'observed-verbose' -Verbose; " +
            "Write-Debug 'observed-debug' -Debug; Write-Information 'observed-information' -InformationAction Continue; " +
            "Write-Progress -Activity 'observed-progress' -Status 'running'")
        .BeginObservedInvocation())
    {
        ulong resultAcknowledgement = 0;
        ulong diagnosticAcknowledgement = 0;
        PowerShellValuePage resultPage = null!;
        PowerShellObservedDiagnosticPage diagnosticPage = null!;
        var streams = new HashSet<PowerShellStreamKind>();
        DateTime deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
        while (DateTime.UtcNow < deadline)
        {
            resultPage = invocation.ReadResults(resultAcknowledgement);
            resultAcknowledgement = resultPage.NextSequence;
            diagnosticPage = invocation.ReadDiagnostics(diagnosticAcknowledgement);
            diagnosticAcknowledgement = diagnosticPage.NextSequence;
            foreach (PowerShellObservedDiagnosticRecord record in diagnosticPage.Records)
            {
                streams.Add(record.Stream);
            }

            if (resultPage.IsTerminal && diagnosticPage.IsTerminal)
            {
                break;
            }

            Thread.Sleep(10);
        }

        Require(
            resultPage.IsTerminal &&
            diagnosticPage.IsTerminal &&
            streams.SetEquals(Enum.GetValues<PowerShellStreamKind>()) &&
            resultPage.TerminalStatus == PowerShellFfiStatus.ManagedFailure &&
            diagnosticPage.TerminalStatus == PowerShellFfiStatus.ManagedFailure &&
            !resultPage.IsComplete &&
            !diagnosticPage.IsComplete,
            "Observed invocation did not copy every stream or mark an error stream incomplete.");
    }

    using (PowerShell builder = CreatePowerShellWhenAvailable(runtime, "the observed cancellation builder"))
    using (PowerShellObservedInvocation invocation = builder
        .AddScript("Start-Sleep -Seconds 5; 'must-not-complete'")
        .BeginObservedInvocation())
    {
        Thread.Sleep(50);
        invocation.Stop();
        PowerShellValuePage cancelled = WaitForObservedResultPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.IsTerminal,
            "observed invocation cancellation");
        Require(
            cancelled.TerminalStatus == PowerShellFfiStatus.OperationCancelled &&
            !cancelled.IsComplete,
            "Stopping an observed invocation did not produce incomplete cancellation.");
    }

    using (PowerShell builder = CreatePowerShellWhenAvailable(runtime, "the observed terminating error builder"))
    using (PowerShellObservedInvocation invocation = builder
        .AddScript("throw 'observed-terminating-error'")
        .BeginObservedInvocation())
    {
        PowerShellObservedDiagnosticPage terminatingDiagnostic = WaitForObservedDiagnosticPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.Records.Count == 1,
            "the observed terminating error diagnostic");
        PowerShellValuePage terminatingResult = WaitForObservedResultPage(
            invocation,
            acknowledgedThrough: 0,
            maximumRecords: 1,
            page => page.IsTerminal,
            "the observed terminating error result");
        Require(
            terminatingDiagnostic.Records[0].Stream == PowerShellStreamKind.Error &&
            terminatingDiagnostic.Records[0].Text.Contains("observed-terminating-error", StringComparison.Ordinal) &&
            terminatingDiagnostic.IsTerminal &&
            terminatingDiagnostic.TerminalStatus == PowerShellFfiStatus.ManagedFailure &&
            terminatingResult.IsTerminal &&
            terminatingResult.TerminalStatus == PowerShellFfiStatus.ManagedFailure &&
            !terminatingResult.IsComplete &&
            !terminatingDiagnostic.IsComplete,
            "Observed terminating errors were not copied before incomplete terminal failure.");
    }

    using PowerShell availabilityCheck = CreatePowerShellWhenAvailable(runtime, "the observed invocation pipeline");
    await Task.CompletedTask;
}

void VerifyTransactionAndHostCapabilities(PowerShellSession session)
{
    using var stagedIntent = new StagedIntentHandler();
    using var alternateStagedIntent = new StagedIntentHandler();
    var stagedSchema = new PowerShellStagedIntentSchema(
    [
        new PowerShellStagedIntentProperty("Id", [PowerShellValueKind.String]),
        new PowerShellStagedIntentProperty("Name", [PowerShellValueKind.String]),
    ],
    maximumPayloadBytes: 512);
    var stagedDefinition = new PowerShellStagedIntentDefinition(
        "example.intent",
        stagedSchema,
        stagedIntent,
        deadline: TimeSpan.FromSeconds(2));
    var alternateStagedDefinition = new PowerShellStagedIntentDefinition(
        "example.intent-alternate",
        stagedSchema,
        alternateStagedIntent,
        deadline: TimeSpan.FromSeconds(2));
    using PowerShellStagedIntentCoordinator stagedIntents =
        PowerShellStagedIntentCoordinator.Register([stagedDefinition, alternateStagedDefinition]);

    using (PowerShell isolated = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = isolated
            .AddScript(@"
                `$stage = `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-isolated'
                    intent = [pscustomobject]@{ Id = 'intent-isolated'; Name = 'owned-by-primary' }
                })
                `$wrongActive = `$DpsCapabilities.Invoke('example.intent-alternate.validate', 'intent-isolated')
                `$validation = `$DpsCapabilities.Invoke('example.intent.validate', 'intent-isolated')
                `$commit = `$DpsCapabilities.Invoke('example.intent.commit', 'intent-isolated')
                `$wrongTerminal = `$DpsCapabilities.Invoke('example.intent-alternate.abort', 'intent-isolated')
                ""`$(`$stage.status)|`$(`$wrongActive.status)|`$(`$validation.status)|`$(`$commit.status)|`$(`$wrongTerminal.status)""
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "staged|unknown-stage|validated|committed|unknown-stage" &&
            alternateStagedIntent.RetainedStageCount == 0,
            "A staged intent was visible to a capability definition that does not own it.");
    }

    using (PowerShell successful = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = successful
            .AddScript(@"
                `$stage = `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-1'
                    intent = [pscustomobject]@{ Id = 'intent-1'; Name = 'committed' }
                })
                `$validation = `$DpsCapabilities.Invoke('example.intent.validate', 'intent-1')
                `$commit = `$DpsCapabilities.Invoke('example.intent.commit', 'intent-1')
                ""`$(`$stage.status)|`$(`$validation.status)|`$(`$commit.status)""
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "staged|validated|committed" &&
            stagedIntent.CommittedName == "committed",
            "The staged intent coordinator did not complete stage, validate, and commit.");
    }

    using (PowerShell postCommit = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = postCommit
            .AddScript(@"
                `$duplicate = `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-1'
                    intent = [pscustomobject]@{ Id = 'intent-1'; Name = 'duplicate' }
                })
                `$afterTerminal = `$DpsCapabilities.Invoke('example.intent.abort', 'intent-1')
                ""`$(`$duplicate.status)|`$(`$afterTerminal.status)""
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "rejected|terminal",
            "The staged intent coordinator accepted a duplicate identifier or an operation after commit.");
    }

    using (PowerShell abort = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = abort
            .AddScript(@"
                `$stage = `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-2'
                    intent = [pscustomobject]@{ Id = 'intent-2'; Name = 'discarded' }
                })
                `$abort = `$DpsCapabilities.Invoke('example.intent.abort', 'intent-2')
                `$afterTerminal = `$DpsCapabilities.Invoke('example.intent.validate', 'intent-2')
                ""`$(`$stage.status)|`$(`$abort.status)|`$(`$afterTerminal.status)""
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "staged|aborted|terminal" &&
            stagedIntent.AbortedName == "discarded",
            "The staged intent coordinator did not abort or reject an operation after abort.");
    }

    int stageCallsBeforeInvalidPayload = stagedIntent.StageCalls;
    using (PowerShell invalid = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = invalid
            .AddScript(@"
                (`$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-invalid'
                    intent = [pscustomobject]@{ Id = 'intent-invalid' }
                })).status
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "rejected" &&
            stagedIntent.StageCalls == stageCallsBeforeInvalidPayload,
            "An invalid staged intent reached the application handler.");
    }

    using (PowerShell stageCommitRace = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = stageCommitRace
            .AddScript(@"
                `$stage = `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-commit-expiry'
                    intent = [pscustomobject]@{ Id = 'intent-commit-expiry'; Name = 'commit-wins' }
                })
                `$validation = `$DpsCapabilities.Invoke('example.intent.validate', 'intent-commit-expiry')
                ""`$(`$stage.status)|`$(`$validation.status)""
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "staged|validated",
            "The commit/expiry race intent did not stage and validate.");
    }
    int abortCallsBeforeCommitRace = stagedIntent.AbortCalls;
    using (PowerShell commitRace = session.CreatePowerShell())
    using (PowerShellInvocationOperation operation = commitRace
        .AddScript("(`$DpsCapabilities.Invoke('example.intent.commit', 'intent-commit-expiry')).status")
        .WithCapabilities(stagedIntents.Capabilities)
        .BeginInvoke())
    {
        Require(
            stagedIntent.CommitStarted.Wait(TimeSpan.FromSeconds(5)),
            "The commit/expiry race handler did not begin.");
        Thread.Sleep(2500);
        stagedIntent.ReleaseCommit.Set();
        Require(
            operation.Wait(TimeSpan.FromSeconds(5)).State == PowerShellOperationState.Completed,
            "The commit/expiry race operation did not complete.");
        PowerShellInvocationResult result = operation.GetResult();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "committed" &&
            stagedIntent.CommittedName == "commit-wins" &&
            stagedIntent.AbortCalls == abortCallsBeforeCommitRace &&
            stagedIntent.RetainedStageCount == 0,
            "Expiry overrode an accepted commit or delivered an unexpected cleanup abort.");
    }

    int abortCallsBeforeExpiry = stagedIntent.AbortCalls;
    using (PowerShell expired = session.CreatePowerShell())
    {
        PowerShellInvocationResult stageResult = expired
            .AddScript(@"
                (`$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-expired'
                    intent = [pscustomobject]@{ Id = 'intent-expired'; Name = 'expired' }
                })).status
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            stageResult.Output.Records.Count == 1 &&
            stageResult.Output.Records[0].DisplayText == "staged",
            "The expiring intent did not stage.");
    }
    Thread.Sleep(2500);
    using (PowerShell expired = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = expired
            .AddScript("(`$DpsCapabilities.Invoke('example.intent.validate', 'intent-expired')).status")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "expired" &&
            stagedIntent.AbortCalls == abortCallsBeforeExpiry + 1 &&
            stagedIntent.AbortedName == "expired" &&
            stagedIntent.RetainedStageCount == 0,
            "The staged intent deadline did not notify the handler and clean up the expired stage.");
    }

    using (PowerShell cancelled = session.CreatePowerShell())
    using (PowerShellInvocationOperation operation = cancelled
        .AddScript(@"
            `$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                stageId = 'intent-cancelled'
                intent = [pscustomobject]@{ Id = 'intent-cancelled'; Name = 'cancelled' }
            })
        ")
        .WithCapabilities(stagedIntents.Capabilities)
        .BeginInvoke())
    {
        Require(
            stagedIntent.CancellationStarted.Wait(TimeSpan.FromSeconds(5)),
            "The staged intent cancellation handler did not begin.");
        operation.Stop();
        Require(
            operation.Wait(TimeSpan.FromSeconds(5)).State == PowerShellOperationState.Cancelled &&
            stagedIntent.CancellationObserved,
            "Stopping a staged intent invocation did not cancel the handler.");
    }
    using (PowerShell cancelled = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = cancelled
            .AddScript("(`$DpsCapabilities.Invoke('example.intent.validate', 'intent-cancelled')).status")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "cancelled",
            "Cancellation did not clean up the staged intent.");
    }

    using (PowerShell disposal = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = disposal
            .AddScript(@"
                (`$DpsCapabilities.Invoke('example.intent.stage', [pscustomobject]@{
                    stageId = 'intent-dispose'
                    intent = [pscustomobject]@{ Id = 'intent-dispose'; Name = 'dispose' }
                })).status
            ")
            .WithCapabilities(stagedIntents.Capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "staged",
            "The disposal intent did not stage.");
    }
    int abortCallsBeforeDispose = stagedIntent.AbortCalls;
    stagedIntents.Dispose();
    Require(
        stagedIntent.AbortCalls == abortCallsBeforeDispose + 1 &&
        stagedIntent.AbortedName == "dispose" &&
        stagedIntent.RetainedStageCount == 0,
        "Disposing a coordinator did not deliver best-effort abort cleanup.");

    var hostInteractions = new HostInteractionCapability();
    using PowerShellCapabilitySet capabilities = PowerShellCapabilitySet.Register(
    [
        new PowerShellCapabilityBinding(PowerShellHostInteraction.WriteText, hostInteractions),
        new PowerShellCapabilityBinding(PowerShellHostInteraction.ReportProgress, hostInteractions),
        new PowerShellCapabilityBinding(PowerShellHostInteraction.PromptChoice, hostInteractions),
    ]);

    using (PowerShell successful = session.CreatePowerShell())
    {
        PowerShellInvocationResult result = successful
            .AddScript(
                "`$null = `$DpsCapabilities.Invoke('host.write-text', 'host-text')\n" +
                "`$null = `$DpsCapabilities.Invoke('host.report-progress', [pscustomobject]@{ ActivityId = 9; ParentActivityId = -1; Activity = 'Copy'; StatusDescription = 'Running'; PercentComplete = 50; SecondsRemaining = 3; IsCompleted = `$false })\n" +
                "`$DpsCapabilities.Invoke('host.prompt-choice', [pscustomobject]@{ Caption = 'Caption'; Message = 'Message'; Choices = @('first', 'second'); DefaultChoice = 0 })")
            .WithCapabilities(capabilities)
            .Invoke();
        Require(
            result.Output.Records.Count == 1 &&
            result.Output.Records[0].DisplayText == "1" &&
            hostInteractions.Text == "host-text" &&
            hostInteractions.Progress is { ActivityId: 9, PercentComplete: 50 } &&
            hostInteractions.PromptCount == 1,
            "Declared host capabilities did not round-trip through the copied capability bridge.");
    }

    using (PowerShell denied = session.CreatePowerShell())
    {
        int hostCalls = hostInteractions.CallCount;
        try
        {
            denied
                .AddScript("`$DpsCapabilities.Invoke('host.read-line', 'not-registered')")
                .WithCapabilities(capabilities)
                .Invoke();
            throw new InvalidOperationException("An unregistered capability was accepted.");
        }
        catch (PowerShellInvocationException)
        {
        }

        Require(
            hostInteractions.CallCount == hostCalls,
            "An unregistered capability reached a registered handler.");
    }

}

if (args.Length != 1)
{
   return 2;
}

PowerShellRuntime runtime = PowerShellRuntime.Activate(args[0]);
VerifyRuntimeDiagnostics(runtime, args[0]);
VerifyCopiedValueReaders();
VerifyTimeSpanNativeManagedCopies();
VerifyScriptParameterMetadata(runtime);
VerifyProgressUpdate();
VerifyCompleteResultProjection(runtime);
await VerifyRecipesSchemasAndPoliciesAsync(runtime);
await VerifyTypedResultPagingAsync(runtime);
await VerifyObservedInvocationAsync(runtime);
Require(
   PowerShellSecretTransfer.Policy == PowerShellSecretTransferPolicy.Rejected &&
   typeof(PowerShellSecretTransfer).GetMethod(nameof(PowerShellSecretTransfer.ThrowNotSupported), BindingFlags.Public | BindingFlags.Static)!.GetParameters().Length == 0,
   "The explicit secret-transfer boundary is unavailable.");
try
{
   PowerShellSecretTransfer.ThrowNotSupported();
   return 1;
}
catch (PowerShellSecretTransferNotSupportedException exception)
{
   Require(
       !exception.Message.Contains("fixture", StringComparison.OrdinalIgnoreCase),
       "The legacy secret-transfer exception leaked a caller marker into diagnostics.");
}

char[] secretCharacters =
[
   (char)102, (char)105, (char)120, (char)116, (char)117, (char)114, (char)101,
   (char)45, (char)115, (char)101, (char)99, (char)114, (char)101, (char)116,
];
string secretMarker = new(secretCharacters);
using (PowerShellSecret secret = PowerShellSecret.Create(secretCharacters))
using (PowerShellCredential credential = new("fixture-user", secret))
{
   Require(
       !secret.ToString().Contains(secretMarker, StringComparison.Ordinal) &&
       !credential.ToString().Contains(secretMarker, StringComparison.Ordinal),
       "Secret lease diagnostics exposed the fixture secret.");

   using (PowerShellSession secretSession = runtime.CreateSession(
       new PowerShellSessionOptions(
           historyMode: PowerShellSessionHistoryMode.Disabled,
           errorPreference: PowerShellSessionPreference.Stop,
           warningPreference: PowerShellSessionPreference.Continue)))
   {
   using (PowerShell discardedSecretOutputPipeline = secretSession.CreatePowerShell())
   {
       using PowerShellSecretResult discardedSecretOutput = discardedSecretOutputPipeline
           .AddScript(
               "param([System.Security.SecureString]`$Secret) " +
               "`$Secret")
           .AddParameter("Secret", secret)
           .InvokeWithSecretBindings();
       Require(
           discardedSecretOutput.Kind == PowerShellSecretResultKind.None &&
           discardedSecretOutput.Secret is null &&
           discardedSecretOutput.Credential is null,
           "The explicit no-output secret invocation contract failed.");
   }

   using (PowerShell sessionSecureStringResultPipeline = secretSession.CreatePowerShell())
   {
       PowerShellSessionSnapshot beforeSecretResult = secretSession.GetSnapshot();
       using PowerShellSecretResult sessionSecureStringResult = sessionSecureStringResultPipeline
           .AddScript(
               "param([System.Security.SecureString]`$Secret) " +
               "if (`$ErrorActionPreference -ne [System.Management.Automation.ActionPreference]::Stop) " +
               "{ throw 'secret invocation settings were not applied' }; " +
               "`$Secret")
           .AddParameter("Secret", secret)
           .InvokeSecretResult(PowerShellSecretResultKind.SecureString);
       PowerShellSessionSnapshot afterSecretResult = secretSession.GetSnapshot();
       Require(
           sessionSecureStringResult.Kind == PowerShellSecretResultKind.SecureString &&
           sessionSecureStringResult.Secret is not null &&
           !sessionSecureStringResult.ToString().Contains(secretMarker, StringComparison.Ordinal) &&
           !sessionSecureStringResult.Secret.ToString().Contains(secretMarker, StringComparison.Ordinal),
           "The persistent-session SecureString secret result contract failed.");
       Require(
           afterSecretResult.InvocationCount == beforeSecretResult.InvocationCount + 1 &&
           afterSecretResult.ActivePipelineCount == 0,
           "The persistent-session SecureString secret result bypassed session lifecycle accounting.");
   }

   using (PowerShell sessionCredentialResultPipeline = secretSession.CreatePowerShell())
   {
       using PowerShellSecretResult sessionCredentialResult = sessionCredentialResultPipeline
           .AddScript(
               "param([System.Security.SecureString]`$Secret) " +
               "[System.Management.Automation.PSCredential]::new('native-aot-user', `$Secret)")
           .AddParameter("Secret", secret)
           .InvokeSecretResult(PowerShellSecretResultKind.Credential);
       Require(
           sessionCredentialResult.Kind == PowerShellSecretResultKind.Credential &&
           sessionCredentialResult.Credential is not null &&
           sessionCredentialResult.Credential.UserName == "native-aot-user" &&
           !sessionCredentialResult.ToString().Contains(secretMarker, StringComparison.Ordinal) &&
           !sessionCredentialResult.Credential.ToString().Contains(secretMarker, StringComparison.Ordinal),
           "The persistent-session PSCredential secret result contract failed.");
   }

   using (PowerShell sessionMultipleSecretResultsPipeline = secretSession.CreatePowerShell())
   {
       PowerShellSessionSnapshot beforeMultipleResults = secretSession.GetSnapshot();
       try
       {
           sessionMultipleSecretResultsPipeline
               .AddScript(
                   "param([System.Security.SecureString]`$Secret) " +
                   "`$Secret; `$Secret")
               .AddParameter("Secret", secret)
               .InvokeSecretResult(PowerShellSecretResultKind.SecureString);
           return 1;
       }
       catch (PowerShellFfiException exception)
       {
           Require(
               !exception.Message.Contains(secretMarker, StringComparison.Ordinal),
               "The rejected multi-result secret invocation leaked the fixture secret.");
       }

       PowerShellSessionSnapshot afterMultipleResults = secretSession.GetSnapshot();
       Require(
           afterMultipleResults.InvocationCount == beforeMultipleResults.InvocationCount + 1 &&
           afterMultipleResults.ActivePipelineCount == 0,
           "The rejected multi-result secret invocation left the session active.");
   }
   }

   using (PowerShell secretPipeline = runtime.Create())
   {
       using PowerShellSecretResult secretResult = secretPipeline
           .AddScript("param([PSCredential]`$Credential) `$Credential.Password")
           .AddParameter("Credential", credential)
           .InvokeSecretResult(PowerShellSecretResultKind.SecureString);
       Require(
           secretResult.Kind == PowerShellSecretResultKind.SecureString &&
           secretResult.Secret is not null &&
           !secretResult.ToString().Contains(secretMarker, StringComparison.Ordinal) &&
           !secretResult.Secret.ToString().Contains(secretMarker, StringComparison.Ordinal),
           "The explicit credential input or redacted SecureString output contract failed.");
   }

   using (PowerShell credentialResultPipeline = runtime.Create())
   {
       using PowerShellSecretResult credentialResult = credentialResultPipeline
           .AddScript(
               "param([System.Security.SecureString]`$Secret) " +
               "[System.Management.Automation.PSCredential]::new('native-aot-user', `$Secret)")
           .AddParameter("Secret", secret)
           .InvokeSecretResult(PowerShellSecretResultKind.Credential);
       Require(
           credentialResult.Kind == PowerShellSecretResultKind.Credential &&
           credentialResult.Credential is not null &&
           credentialResult.Credential.UserName == "native-aot-user" &&
           !credentialResult.ToString().Contains(secretMarker, StringComparison.Ordinal) &&
           !credentialResult.Credential.ToString().Contains(secretMarker, StringComparison.Ordinal),
           "The explicit SecureString input or redacted PSCredential output contract failed.");
   }

   using (PowerShell normalResultPipeline = runtime.Create())
   {
       try
       {
           normalResultPipeline
               .AddScript("param([PSCredential]`$Credential) 'normal-output'")
               .AddParameter("Credential", credential)
               .InvokeWithDiagnostics();
           return 1;
       }
       catch (PowerShellFfiException exception)
       {
           Require(
               !exception.Message.Contains(secretMarker, StringComparison.Ordinal),
               "Secret-bound normal invocation leaked the fixture secret.");
       }
   }

   try
   {
       _ = PowerShellValue.From(secret);
       return 1;
   }
   catch (PowerShellValueConversionException)
   {
   }
}
Array.Clear(secretCharacters);
using (var secureString = new SecureString())
{
   secureString.AppendChar((char)120);
   secureString.MakeReadOnly();
   try
   {
       _ = PowerShellValue.From(secureString);
       return 1;
   }
   catch (PowerShellValueConversionException)
   {
   }
}
using (PowerShellSecret credentialUserNameSecret = PowerShellSecret.Create("credential-user-name-secret"))
{
   try
   {
       _ = new PowerShellCredential(new string('u', 257), credentialUserNameSecret);
       return 1;
   }
   catch (ArgumentException)
   {
   }
}
try
{
   _ = PowerShellValue.From((Action)(() => { }));
    return 1;
}
catch (PowerShellValueConversionException)
{
}
Require(
    !typeof(PowerShell).GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static)
       .SelectMany(method => method.GetParameters())
       .Any(parameter => parameter.ParameterType == typeof(SecureString)),
    "The facade unexpectedly exposes SecureString parameter transfer.");

using PowerShell powerShell = PowerShell.Create();
PowerShellInvocationResult result = powerShell
    .AddScript("Write-Error -Message 'package-non-terminating-error'")
    .InvokeWithDiagnostics();
if (result.Errors.Records.Count != 1 || !result.Errors.Records[0].Message.Contains("package-non-terminating-error"))
{
    return 1;
}

using (PowerShell projectionBuilder = PowerShell.Create())
{
    PowerShellInvocationResult projectionResult = projectionBuilder
        .AddScript("[pscustomobject]@{ Name = 'package-projection'; Count = 2; Nested = @{ Value = 1 }; Items = 1, 2 }; `$exception = [System.Runtime.InteropServices.COMException]::new('package-projection-error', -2128394905); Write-Error -Exception `$exception -Category InvalidOperation -TargetObject 42")
        .InvokeWithDiagnostics();
    PowerShellObjectSnapshot projection = projectionResult.Output.Records[0];
    PowerShellInvocationError projectionError = projectionResult.Errors.Records[0];
    PowerShellValue? projectionBag = projection.PropertyBag;
    PowerShellValue? projectionTarget = projectionError.TargetValue;
    Require(
        projectionResult.Output.TotalRecordCount == 1 &&
        projectionResult.Output.DroppedRecordCount == 0 &&
        projectionBag?.Kind == PowerShellValueKind.PropertyBag &&
        projection.PropertyEntryCount == 2 &&
        projection.DroppedPropertyEntryCount == 2 &&
        projection.ScalarValue is null &&
        projectionBag is not null &&
        projectionBag.TryGetProperty("Name", out PowerShellValue? projectionName) &&
        projectionName!.TryGetString(out string? projectionNameText) &&
        projectionNameText == "package-projection" &&
        projectionBag.TryGetProperty("Count", out PowerShellValue? projectionCount) &&
        projectionCount!.TryGetSignedInteger(out long projectionCountValue) &&
        projectionCountValue == 2 &&
        projectionTarget?.Kind == PowerShellValueKind.SignedInteger &&
        projectionTarget is not null &&
        projectionTarget.TryGetSignedInteger(out long projectionTargetValue) &&
        projectionTargetValue == 42 &&
        projectionResult.Errors.TotalRecordCount == 1,
        "Package consumer did not preserve bounded snapshot projections.");

    byte[] stored = PowerShellSnapshotSerializer.Serialize(projectionResult);
    Require(
        !Encoding.UTF8.GetString(stored).Contains(secretMarker, StringComparison.Ordinal),
        "Snapshot serialization leaked the explicit secret fixture.");
    PowerShellInvocationResult restored = PowerShellSnapshotSerializer.Deserialize(stored);
    VerifyCopiedExceptionHResultAndSnapshotSerialization(projectionResult, stored, restored);
    PowerShellValue? restoredBag = restored.Output.Records[0].PropertyBag;
    Require(
        restoredBag?.Kind == PowerShellValueKind.PropertyBag &&
        restored.Output.Records[0].DroppedPropertyEntryCount == 2 &&
        restoredBag is not null &&
        restoredBag.TryGetProperty("Count", out PowerShellValue? restoredCount) &&
        restoredCount!.TryGetSignedInteger(out long restoredCountValue) &&
        restoredCountValue == 2,
        "Package consumer did not round-trip the bounded storage/display snapshot.");

    try
    {
        _ = PowerShellSnapshotSerializer.Deserialize(new byte[PowerShellSnapshotSerializer.MaxDocumentBytes + 1]);
        return 1;
    }
    catch (ArgumentOutOfRangeException)
    {
    }

    try
    {
        _ = PowerShellSnapshotSerializer.Deserialize(Encoding.UTF8.GetBytes("{\"version\":99,\"result\":null}"));
        return 1;
    }
    catch (ArgumentException)
    {
    }

    try
    {
        string withUnknownMember = Encoding.UTF8.GetString(stored)[..^1] + ",\"unexpected\":true}";
        _ = PowerShellSnapshotSerializer.Deserialize(Encoding.UTF8.GetBytes(withUnknownMember));
        return 1;
    }
    catch (ArgumentException)
    {
    }

    try
    {
        _ = PowerShellSnapshotSerializer.Deserialize(Encoding.UTF8.GetBytes(new string('[', 17) + new string(']', 17)));
        return 1;
    }
    catch (ArgumentException)
    {
    }
}

using (PowerShell terminating = PowerShell.Create())
{
    try
    {
        terminating.AddScript("throw 'package-terminating-error'").Invoke();
        return 1;
    }
    catch (PowerShellInvocationException exception)
    {
        Require(
            exception.Errors.Count == 1 &&
            exception.Errors[0].Message.Contains("package-terminating-error"),
            "Terminating invocation did not preserve its semantic error snapshot.");
    }
}

using (PowerShell nulBuilder = PowerShell.Create())
{
    try
    {
        nulBuilder.AddScript("Write-Output 'nul'\0");
        return 1;
    }
    catch (ArgumentException)
    {
    }
}

try
{
    _ = PowerShellSessionConfiguration.CreateWithModuleImports(
        [new PowerShellModuleImport("SimpleModuleImportOverflow")],
        moduleImports: Enumerable.Range(0, 32).Select(static index => $"SimpleModule{index}"));
    return 1;
}
catch (ArgumentException)
{
}

try
{
    _ = new PowerShellSessionConfiguration(workingDirectory: "relative-directory");
    return 1;
}
catch (ArgumentException)
{
}

string preflightRoot = Path.Combine(Path.GetTempPath(), $"pwsh-sdk-ffi-preflight-{Guid.NewGuid():N}");
string preflightModuleName = "PreflightOnly";
string preflightModuleDirectory = Path.Combine(preflightRoot, preflightModuleName);
string preflightManifestPath = Path.Combine(preflightModuleDirectory, $"{preflightModuleName}.psd1");
string preflightModulePath = Path.Combine(preflightModuleDirectory, $"{preflightModuleName}.psm1");
string preflightSideEffectPath = Path.Combine(preflightRoot, "module-executed.txt");
try
{
    Directory.CreateDirectory(preflightModuleDirectory);
    string longVersion = new('1', 160);
    string longCommand = new('x', 80);
    File.WriteAllText(
        preflightManifestPath,
        $"@{{ RootModule = '{preflightModuleName}.psm1'; ModuleVersion = '{longVersion}'; FunctionsToExport = @('Get-PreflightOne', 'Get-PreflightTwo', 'Get-PreflightThree', '{longCommand}', 'Get-PreflightFive') }}");
    File.WriteAllText(
        preflightModulePath,
        $"Set-Content -LiteralPath '{preflightSideEffectPath.Replace("'", "''", StringComparison.Ordinal)}' -Value 'executed'");

    var preflightConfiguration = new PowerShellSessionConfiguration(
        moduleImports: new[] { preflightModuleName },
        allowedModulePaths: new[] { preflightRoot });
    PowerShellSessionPreflightReport preflight = runtime.ValidateSessionConfiguration(preflightConfiguration);
    PowerShellSessionModuleImportDiagnostic preflightImport = preflight.ModuleImports.Single();
    Require(
        preflight.Status == PowerShellSessionPreflightStatus.Valid &&
        preflight.ModuleRoots.Count == 1 &&
        preflight.ModuleRoots[0].Status == PowerShellSessionModuleRootStatus.Valid &&
        preflightImport.Status == PowerShellSessionModuleImportStatus.Resolved &&
        preflightImport.DeclaredVersion.Length == 128 &&
        preflightImport.DeclaredCommands.Count == 4 &&
        preflightImport.DeclaredCommands[3].Length == 64 &&
        preflightImport.DeclaredCommandsTruncated &&
        !File.Exists(preflightSideEffectPath),
        "Session preflight did not return bounded static module declarations without executing module code.");

    PowerShellSessionPreflightReport missingRoot = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            allowedModulePaths: new[] { Path.Combine(preflightRoot, "missing-root") }));
    Require(
        missingRoot.Status == PowerShellSessionPreflightStatus.InvalidModuleRoots &&
        missingRoot.ModuleRoots.Single().Status == PowerShellSessionModuleRootStatus.Missing,
        "Session preflight did not report a missing module root.");

    PowerShellSessionPreflightReport invalidRoot = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            allowedModulePaths: new[] { preflightRoot, preflightRoot + Path.DirectorySeparatorChar }));
    Require(
        invalidRoot.Status == PowerShellSessionPreflightStatus.InvalidModuleRoots &&
        invalidRoot.ModuleRoots.Any(root => root.Status == PowerShellSessionModuleRootStatus.Invalid),
        "Session preflight did not report duplicate canonical module roots.");

    PowerShellSessionPreflightReport unresolvableImport = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            moduleImports: new[] { "MissingPreflightModule" },
            allowedModulePaths: new[] { preflightRoot }));
    Require(
        unresolvableImport.Status == PowerShellSessionPreflightStatus.UnresolvableModuleImports &&
        unresolvableImport.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.Unresolvable,
        "Session preflight did not report an unresolvable module import.");

    string invalidManifestName = "InvalidPreflightModule";
    File.WriteAllText(Path.Combine(preflightRoot, $"{invalidManifestName}.psd1"), "not a manifest hashtable");
    PowerShellSessionPreflightReport invalidManifest = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            moduleImports: new[] { invalidManifestName },
            allowedModulePaths: new[] { preflightRoot }));
    Require(
        invalidManifest.Status == PowerShellSessionPreflightStatus.InvalidModuleManifest &&
        invalidManifest.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.ManifestInvalid,
        "Session preflight did not report an invalid module manifest.");

    string externalModuleName = "ExternalPreflightModule";
    string externalModuleDirectory = Path.Combine(preflightRoot, externalModuleName);
    string externalTargetPath = Path.Combine(Path.GetTempPath(), "pwsh-sdk-ffi-external-module.psm1");
    Directory.CreateDirectory(externalModuleDirectory);
    File.WriteAllText(
        Path.Combine(externalModuleDirectory, $"{externalModuleName}.psd1"),
        $"@{{ ModuleVersion = '1.0'; RootModule = '{externalTargetPath.Replace("'", "''", StringComparison.Ordinal)}' }}");
    var externalConfiguration = new PowerShellSessionConfiguration(
        moduleImports: new[] { externalModuleName },
        allowedModulePaths: new[] { preflightRoot });
    PowerShellSessionPreflightReport externalDeclaration = runtime.ValidateSessionConfiguration(externalConfiguration);
    Require(
        externalDeclaration.Status == PowerShellSessionPreflightStatus.ExternalModuleDeclarations &&
        externalDeclaration.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.ManifestDeclaresExternalPath,
        "Session preflight did not report a manifest loading code from outside its approved module root.");

    try
    {
        using PowerShellSession externalSession = runtime.CreateSession(
            new PowerShellSessionOptions(configuration: externalConfiguration));
        return 1;
    }
    catch (PowerShellFfiException)
    {
    }

    string dynamicDeclarationName = "DynamicDeclarationPreflightModule";
    string dynamicDeclarationDirectory = Path.Combine(preflightRoot, dynamicDeclarationName);
    Directory.CreateDirectory(dynamicDeclarationDirectory);
    File.WriteAllText(
        Path.Combine(dynamicDeclarationDirectory, $"{dynamicDeclarationName}.psd1"),
        "@{ ModuleVersion = '1.0'; RootModule = `$PSScriptRoot + '\\dynamic-preflight.psm1' }");
    PowerShellSessionPreflightReport dynamicDeclaration = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            moduleImports: new[] { dynamicDeclarationName },
            allowedModulePaths: new[] { preflightRoot }));
    Require(
        dynamicDeclaration.Status == PowerShellSessionPreflightStatus.InvalidModuleManifest &&
        dynamicDeclaration.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.ManifestInvalid,
        "Session preflight accepted a non-static module-loading declaration.");

    string junctionModuleName = "JunctionEscapePreflightModule";
    string junctionModuleDirectory = Path.Combine(preflightRoot, junctionModuleName);
    string junctionPath = Path.Combine(junctionModuleDirectory, "linked");
    string junctionTargetDirectory = Path.Combine(Path.GetTempPath(), $"pwsh-sdk-ffi-junction-{Guid.NewGuid():N}");
    try
    {
        Directory.CreateDirectory(junctionModuleDirectory);
        Directory.CreateDirectory(junctionTargetDirectory);
        File.WriteAllText(Path.Combine(junctionTargetDirectory, "outside.psm1"), "function Get-JunctionEscape { 1 }");
        using (System.Diagnostics.Process junctionProcess = System.Diagnostics.Process.Start(
            new System.Diagnostics.ProcessStartInfo(
                "cmd.exe",
                $"/d /c mklink /J \"{junctionPath}\" \"{junctionTargetDirectory}\"")
            {
                CreateNoWindow = true,
                UseShellExecute = false,
            }) ?? throw new InvalidOperationException("Could not create the junction test process."))
        {
            junctionProcess.WaitForExit();
            Require(junctionProcess.ExitCode == 0, "Could not create the junction fixture for module-root validation.");
        }

        File.WriteAllText(
            Path.Combine(junctionModuleDirectory, $"{junctionModuleName}.psd1"),
            "@{ ModuleVersion = '1.0'; RootModule = 'linked\\outside.psm1' }");
        PowerShellSessionPreflightReport junctionEscape = runtime.ValidateSessionConfiguration(
            new PowerShellSessionConfiguration(
                moduleImports: new[] { junctionModuleName },
                allowedModulePaths: new[] { preflightRoot }));
        Require(
            junctionEscape.Status == PowerShellSessionPreflightStatus.ExternalModuleDeclarations &&
            junctionEscape.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.ManifestDeclaresExternalPath,
            "Session preflight accepted a module-loading path beneath an in-root junction that targets an external directory.");
    }
    finally
    {
        if (Directory.Exists(junctionPath))
        {
            Directory.Delete(junctionPath);
        }
        if (Directory.Exists(junctionTargetDirectory))
        {
            Directory.Delete(junctionTargetDirectory, recursive: true);
        }
    }

    string nestedEscapeName = "NestedEscapePreflightModule";
    string nestedEscapeDirectory = Path.Combine(preflightRoot, nestedEscapeName);
    Directory.CreateDirectory(nestedEscapeDirectory);
    File.WriteAllText(
        Path.Combine(nestedEscapeDirectory, $"{nestedEscapeName}.psd1"),
        "@{ ModuleVersion = '1.0'; NestedModules = @('..\\..\\escaped-preflight.psm1') }");
    PowerShellSessionPreflightReport nestedEscape = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            moduleImports: new[] { nestedEscapeName },
            allowedModulePaths: new[] { preflightRoot }));
    Require(
        nestedEscape.Status == PowerShellSessionPreflightStatus.ExternalModuleDeclarations &&
        nestedEscape.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.ManifestDeclaresExternalPath,
        "Session preflight did not report a relative nested-module path escaping its approved module root.");

    string containedName = "ContainedPreflightModule";
    string containedDirectory = Path.Combine(preflightRoot, containedName);
    Directory.CreateDirectory(containedDirectory);
    File.WriteAllText(Path.Combine(containedDirectory, $"{containedName}.psm1"), "function Get-Contained { 1 }");
    File.WriteAllText(
        Path.Combine(containedDirectory, $"{containedName}.psd1"),
        $"@{{ ModuleVersion = '1.0'; RootModule = '{containedName}.psm1'; RequiredModules = @('Microsoft.PowerShell.Utility'); NestedModules = @(@{{ ModuleName = 'Microsoft.PowerShell.Management'; ModuleVersion = '1.0' }}); FunctionsToExport = @('Get-Contained') }}");
    PowerShellSessionPreflightReport contained = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            moduleImports: new[] { containedName },
            allowedModulePaths: new[] { preflightRoot }));
    Require(
        contained.Status == PowerShellSessionPreflightStatus.Valid &&
        contained.ModuleImports.Single().Status == PowerShellSessionModuleImportStatus.Resolved,
        "Session preflight rejected a manifest whose module references are contained or name-based.");

    string declaredModuleName = "DeclaredModuleImport";
    string declaredModuleDirectory = Path.Combine(preflightRoot, declaredModuleName);
    Directory.CreateDirectory(declaredModuleDirectory);
    File.WriteAllText(
        Path.Combine(declaredModuleDirectory, $"{declaredModuleName}.psd1"),
        $"@{{ RootModule = '{declaredModuleName}.psm1'; ModuleVersion = '1.0'; FunctionsToExport = @('Get-DeclaredModuleValue') }}");
    File.WriteAllText(
        Path.Combine(declaredModuleDirectory, $"{declaredModuleName}.psm1"),
        "function Get-DeclaredModuleValue { 'declared-module-value' }");
    PowerShellSessionConfiguration declaredModuleConfiguration =
        PowerShellSessionConfiguration.CreateWithModuleImports(
            new[]
            {
                new PowerShellModuleImport(
                    declaredModuleName,
                    new Version(1, 0),
                    PowerShellModuleImportOptions.Force),
            },
            allowedModulePaths: new[] { preflightRoot });
    using (PowerShellSession declaredModuleSession = runtime.CreateSession(
        new PowerShellSessionOptions(configuration: declaredModuleConfiguration)))
    using (PowerShell declaredModuleCommand = declaredModuleSession.CreatePowerShell())
    {
        PowerShellInvocationResult declaredModuleResult = declaredModuleCommand
            .AddCommand("Get-DeclaredModuleValue")
            .InvokeWithDiagnostics();
        Require(
            declaredModuleResult.Output.Records.Count == 1 &&
            declaredModuleResult.Output.Records[0].DisplayText == "declared-module-value",
            "A declared module import did not remain within its approved module root.");
    }

    PowerShellSessionPreflightReport missingWorkingDirectory = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            allowedModulePaths: new[] { preflightRoot },
            workingDirectory: Path.Combine(preflightRoot, "missing-working-directory")));
    Require(
        missingWorkingDirectory.Status == PowerShellSessionPreflightStatus.InvalidWorkingDirectory &&
        missingWorkingDirectory.Diagnostic.Length != 0,
        "Session preflight did not report a nonexistent working directory.");

    PowerShellSessionPreflightReport validWorkingDirectory = runtime.ValidateSessionConfiguration(
        new PowerShellSessionConfiguration(
            allowedModulePaths: new[] { preflightRoot },
            workingDirectory: preflightRoot));
    Require(
        validWorkingDirectory.Status == PowerShellSessionPreflightStatus.Valid,
        "Session preflight rejected an existing working directory.");

    PowerShellSessionPreflightReport currentRunspaceConfiguration = runtime.ValidateSessionConfiguration(
        new PowerShellSessionOptions(
            runspaceMode: PowerShellRunspaceMode.CurrentRunspace,
            configuration: preflightConfiguration));
    Require(
        currentRunspaceConfiguration.Status == PowerShellSessionPreflightStatus.InvalidConfiguration,
        "Session preflight did not reject configured current-runspace sessions.");

    try
    {
        using PowerShellSession invalidCurrentRunspaceSession = runtime.CreateSession(
            new PowerShellSessionOptions(
                runspaceMode: PowerShellRunspaceMode.CurrentRunspace,
                configuration: preflightConfiguration));
        return 1;
    }
    catch (PowerShellFfiException exception)
        when (exception.Status == PowerShellFfiStatus.UnsupportedCapability)
    {
    }
}
finally
{
    if (Directory.Exists(preflightRoot))
    {
        Directory.Delete(preflightRoot, recursive: true);
    }
}

var sessionConfiguration = new PowerShellSessionConfiguration(
    initialVariables: new Dictionary<string, PowerShellValue>
    {
        ["FfiMarker"] = PowerShellValue.String("session-marker"),
        ["FfiNumber"] = PowerShellValue.SignedInteger(7),
    },
    moduleImports: new[] { "Microsoft.PowerShell.Utility" },
    allowedModulePaths: new[] { Path.Combine(args[0], "Modules") },
    workingDirectory: args[0],
    environment: new Dictionary<string, string>
    {
        ["DPS_FFI_TEST"] = "session-environment",
    });
using PowerShellSession session = runtime.CreateSession(
    new PowerShellSessionOptions(
        historyMode: PowerShellSessionHistoryMode.Enabled,
        errorPreference: PowerShellSessionPreference.Stop,
        configuration: sessionConfiguration));
string? remoteComputer = Environment.GetEnvironmentVariable("PWSH_FFI_REMOTE_COMPUTER");
if (!string.IsNullOrWhiteSpace(remoteComputer))
{
    using PowerShellRemoteSession remoteSession = PowerShellRemoteSession.Create(
        session,
        new PowerShellRemoteSessionOptions(remoteComputer));
    PowerShellRemoteSessionMetadata remoteMetadata = remoteSession.GetMetadata();
    PowerShellInvocationResult remoteResult = remoteSession.InvokeScript("'remote-package-smoke'");
    Require(
        remoteMetadata.Id > 0 &&
        !string.IsNullOrWhiteSpace(remoteMetadata.State) &&
        !string.IsNullOrWhiteSpace(remoteMetadata.ComputerName) &&
        remoteResult.Output.Records.Count == 1 &&
        remoteResult.Output.Records[0].DisplayText == "remote-package-smoke",
        "The environment-configured PSSession adapter smoke test failed.");
}
using PowerShell sessionPowerShell = session.CreatePowerShell();
PowerShellInvocationResult sessionResult = sessionPowerShell
    .AddScript("@(`$FfiMarker, `$FfiNumber, `$env:DPS_FFI_TEST, (Get-Location).Path, `$ErrorActionPreference)")
    .Invoke();
using PowerShell reusedSessionPowerShell = session.CreatePowerShell();
PowerShellInvocationResult moduleResult = reusedSessionPowerShell
    .AddScript("(Get-Module Microsoft.PowerShell.Utility).Name")
    .Invoke();
PowerShellSessionSnapshot snapshot = session.GetSnapshot();
if (sessionResult.Output.Records.Count != 5 ||
    sessionResult.Output.Records[0].DisplayText != "session-marker" ||
    sessionResult.Output.Records[1].DisplayText != "7" ||
    sessionResult.Output.Records[2].DisplayText != "session-environment" ||
!string.Equals(sessionResult.Output.Records[3].DisplayText, args[0], StringComparison.OrdinalIgnoreCase) ||
!File.Exists(Path.Combine(sessionResult.Output.Records[3].DisplayText, "pwsh.dll")) ||
sessionResult.Output.Records[4].DisplayText != "Stop" ||
    moduleResult.Output.Records.Count != 1 ||
    moduleResult.Output.Records[0].DisplayText != "Microsoft.PowerShell.Utility" ||
    snapshot.InvocationCount != 2 ||
    snapshot.HistoryCount != 2 ||
    session.GetEvents().Count < 3)
{
    return 1;
}

PowerShellCapabilityDefinition labelDefinition = new(
    "example.get-label",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.String },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 1024,
    deadline: TimeSpan.FromSeconds(5));
using (PowerShellCapabilitySet capabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(labelDefinition, new LabelCapability()),
}))
using (PowerShell capabilityPowerShell = session.CreatePowerShell())
{
    PowerShellInvocationResult capabilityResult = capabilityPowerShell
        .AddScript("`$DpsCapabilities.Invoke('example.get-label')")
        .WithCapabilities(capabilities)
        .Invoke();
    Require(
        capabilityResult.Output.Records.Count == 1 &&
        capabilityResult.Output.Records[0].DisplayText == "nativeaot-label",
        "The bounded capability callback did not round-trip through the payload bridge.");
}
try
{
    using PowerShellCapabilitySet duplicateCapabilities = runtime.RegisterCapabilities(new[]
    {
        new PowerShellCapabilityBinding(labelDefinition, new LabelCapability()),
        new PowerShellCapabilityBinding(labelDefinition, new LabelCapability()),
    });
    return 1;
}
catch (ArgumentException)
{
}
using (PowerShell unknownCapabilityPowerShell = session.CreatePowerShell())
{
    using PowerShellCapabilitySet unknownCapabilities = runtime.RegisterCapabilities(new[]
    {
        new PowerShellCapabilityBinding(labelDefinition, new LabelCapability()),
    });
    try
    {
        unknownCapabilityPowerShell
            .AddScript("`$DpsCapabilities.Invoke('example.unknown')")
            .WithCapabilities(unknownCapabilities)
            .Invoke();
        return 1;
    }
    catch (PowerShellInvocationException)
    {
    }
}
PowerShellCapabilityDefinition failingDefinition = new(
    "example.fail",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.String },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 256,
    deadline: TimeSpan.FromSeconds(5));
using (PowerShellCapabilitySet failingCapabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(failingDefinition, new ThrowingCapability()),
}))
using (PowerShell failingCapabilityPowerShell = session.CreatePowerShell())
{
    try
    {
        failingCapabilityPowerShell
            .AddScript("`$DpsCapabilities.Invoke('example.fail')")
            .WithCapabilities(failingCapabilities)
            .Invoke();
        return 1;
    }
    catch (PowerShellInvocationException)
    {
    }
}
PowerShellCapabilityDefinition timeoutDefinition = new(
    "example.timeout",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.String },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 256,
    deadline: TimeSpan.FromMilliseconds(10));
using (PowerShellCapabilitySet timeoutCapabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(timeoutDefinition, new TimeoutCapability()),
}))
using (PowerShell timeoutCapabilityPowerShell = session.CreatePowerShell())
{
    try
    {
        timeoutCapabilityPowerShell
            .AddScript("`$DpsCapabilities.Invoke('example.timeout')")
            .WithCapabilities(timeoutCapabilities)
            .Invoke();
        return 1;
    }
    catch (PowerShellInvocationException)
    {
    }
}
PowerShellCapabilityDefinition reentryDefinition = new(
    "example.reentry",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.String },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 256,
    deadline: TimeSpan.FromSeconds(5));
using (PowerShellCapabilitySet reentryCapabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(reentryDefinition, new ReentryCapability()),
}))
using (PowerShell reentryCapabilityPowerShell = session.CreatePowerShell())
{
    PowerShellInvocationResult reentryResult = reentryCapabilityPowerShell
        .AddScript("`$DpsCapabilities.Invoke('example.reentry')")
        .WithCapabilities(reentryCapabilities)
        .Invoke();
    Require(
        reentryResult.Output.Records.Count == 1 &&
        reentryResult.Output.Records[0].DisplayText == "reentry-blocked",
        "A capability handler re-entered the FFI instead of receiving backpressure.");
}
PowerShellCapabilityDefinition cancelledDefinition = new(
    "example.wait-for-cancellation",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.Null },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 64,
    deadline: TimeSpan.FromSeconds(5));
using var cancellableHandler = new CancellableCapability();
using (PowerShellCapabilitySet cancelledCapabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(cancelledDefinition, cancellableHandler),
}))
using (PowerShell cancelledCapabilityPowerShell = session.CreatePowerShell())
using (PowerShellInvocationOperation cancelledCapabilityOperation = cancelledCapabilityPowerShell
    .AddScript("`$DpsCapabilities.Invoke('example.wait-for-cancellation')")
    .WithCapabilities(cancelledCapabilities)
    .BeginInvoke())
{
    Require(
        cancellableHandler.Started.Wait(TimeSpan.FromSeconds(5)),
        "The cancellable capability handler did not begin before the operation was stopped.");
    cancelledCapabilityOperation.Stop();
    Require(
        cancelledCapabilityOperation.Wait(TimeSpan.FromSeconds(5)).State == PowerShellOperationState.Cancelled,
        "Stopping an active capability invocation did not cancel the handler and operation.");
    Require(
        cancellableHandler.CancellationObserved,
        "Stopping an active capability invocation did not signal the handler cancellation token.");
}
VerifyTransactionAndHostCapabilities(session);

PowerShellValue copiedVariable = PowerShellValue.PropertyBag(new[]
{
    new KeyValuePair<string, PowerShellValue>("Marker", PowerShellValue.String("copied-variable")),
    new KeyValuePair<string, PowerShellValue>(
        "Items",
        PowerShellValue.Array(new[]
        {
            PowerShellValue.SignedInteger(3),
            PowerShellValue.String("four"),
        })),
});
session.SetVariable("FfiCopied", copiedVariable);
Require(
    session.TryGetVariable("FfiCopied", out PowerShellValue? copiedSnapshot) &&
    copiedSnapshot?.Kind == PowerShellValueKind.PropertyBag &&
    copiedSnapshot is not null &&
    copiedSnapshot.TryGetProperty("Marker", out PowerShellValue? copiedMarker) &&
    copiedMarker!.TryGetString(out string? copiedMarkerText) &&
    copiedMarkerText == "copied-variable" &&
    copiedSnapshot.TryGetProperty("Items", out PowerShellValue? copiedItems) &&
    copiedItems!.GetArray().Count == 2 &&
    copiedItems.GetArray()[0].TryGetSignedInteger(out long copiedFirstItem) &&
    copiedFirstItem == 3,
    "Session variable storage did not return a copied property-bag snapshot.");
using (PowerShell copiedVariableBuilder = session.CreatePowerShell())
{
    PowerShellInvocationResult copiedVariableResult = copiedVariableBuilder
        .AddScript("@(`$FfiCopied.Marker, `$FfiCopied.Items[0], `$FfiCopied.Items[1])")
        .Invoke();
    Require(
        copiedVariableResult.Output.Records.Count == 3 &&
        copiedVariableResult.Output.Records[0].DisplayText == "copied-variable" &&
        copiedVariableResult.Output.Records[1].DisplayText == "3" &&
        copiedVariableResult.Output.Records[2].DisplayText == "four",
        "Copied session variables were not retained as tagged PowerShell values.");
}
session.SetVariable(
    "FfiResult",
    PowerShellValue.PropertyBag(
    [
        new KeyValuePair<string, PowerShellValue>("Status", PowerShellValue.String("pending")),
    ]));
using (PowerShell resultVariableBuilder = session.CreatePowerShell())
{
    _ = resultVariableBuilder
        .AddScript("`$FfiResult = [pscustomobject]@{ Status = 'completed'; Count = 2 }")
        .Invoke();
}
Require(
    session.TryGetVariable("FfiResult", out PowerShellValue? resultSnapshot) &&
    resultSnapshot?.Kind == PowerShellValueKind.PropertyBag &&
    resultSnapshot is not null &&
    resultSnapshot.TryGetProperty("Status", out PowerShellValue? resultStatus) &&
    resultStatus!.TryGetString(out string? resultStatusText) &&
    resultStatusText == "completed" &&
    resultSnapshot.TryGetProperty("Count", out PowerShellValue? resultCount) &&
    resultCount!.TryGetSignedInteger(out long resultCountValue) &&
    resultCountValue == 2,
    "A value-only script result did not return as a copied session-variable DTO.");
Require(
    session.TryGetPropertyBag("FfiResult", out IReadOnlyDictionary<string, PowerShellValue>? resultProperties) &&
    resultProperties is not null &&
    resultProperties["Status"].TryGetString(out string? copiedResultStatus) &&
    copiedResultStatus == "completed",
    "The session property-bag convenience API did not return a copied DTO.");
PowerShellSessionScriptResult recipeVariableResult = session.InvokeAndReadVariable(
    new PowerShellScriptRecipe("`$FfiRecipeResult = [pscustomobject]@{ Status = 'recipe'; Count = 3 }"),
    "FfiRecipeResult",
    new PowerShellCommandPolicy(allowScripts: true));
Require(
    recipeVariableResult.Invocation.Output.Records.Count == 0 &&
    recipeVariableResult.HasValue &&
    recipeVariableResult.Value is { Kind: PowerShellValueKind.PropertyBag } &&
    recipeVariableResult.Value.TryGetProperty("Status", out PowerShellValue? recipeStatus) &&
    recipeStatus!.TryGetString(out string? recipeStatusText) &&
    recipeStatusText == "recipe",
    "The session recipe result-variable helper did not return a copied result DTO.");
Require(
    session.RemoveVariable("FfiCopied") &&
    !session.RemoveVariable("FfiCopied") &&
    !session.TryGetVariable("FfiCopied", out _),
    "Session variable removal or absence reporting is incorrect.");

using (PowerShell unsupportedVariableBuilder = session.CreatePowerShell())
{
    unsupportedVariableBuilder
        .AddScript("`$FfiUnsupported = [System.Collections.ArrayList]::new(); `$null")
        .Invoke();
}
try
{
    _ = session.TryGetVariable("FfiUnsupported", out _);
    return 1;
}
catch (PowerShellFfiException exception) when (exception.Status == PowerShellFfiStatus.UnsupportedValue)
{
}
Require(session.RemoveVariable("FfiUnsupported"), "An unsupported session variable could not be removed.");

using (PowerShell pendingVariableBuilder = session.CreatePowerShell())
using (PowerShellInvocationOperation pendingVariableOperation = pendingVariableBuilder
    .AddScript("Start-Sleep -Seconds 5")
    .BeginInvoke())
{
    try
    {
        session.SetVariable("FfiBlocked", PowerShellValue.String("must-not-mutate"));
        return 1;
    }
    catch (PowerShellFfiException exception) when (exception.Status == PowerShellFfiStatus.Backpressure)
    {
    }

    pendingVariableOperation.Stop();
    Require(
        pendingVariableOperation.Wait(TimeSpan.FromSeconds(5)).State == PowerShellOperationState.Cancelled,
        "Stopping a session operation did not release the variable-mutation gate.");
}

var cyclicValue = new List<object?>();
cyclicValue.Add(cyclicValue);
try
{
    _ = PowerShellValue.From(cyclicValue);
    return 1;
}
catch (ArgumentException)
{
}

object? nestedValue = "leaf";
for (int depth = 0; depth <= 8; depth++)
{
    nestedValue = new object?[] { nestedValue };
}
try
{
    _ = PowerShellValue.From(nestedValue);
    return 1;
}
catch (ArgumentException)
{
}

using (PowerShellSession restrictedSession = runtime.CreateSession(
    new PowerShellSessionOptions(
        configuration: new PowerShellSessionConfiguration(
            moduleImports: new[] { "Microsoft.PowerShell.Security" },
            allowedModulePaths: new[] { Path.Combine(args[0], "Modules") },
            executionPolicy: PowerShellSessionExecutionPolicy.Restricted))))
using (PowerShell restrictedPowerShell = restrictedSession.CreatePowerShell())
{
    PowerShellInvocationResult restrictedResult = restrictedPowerShell.AddCommand("Get-ExecutionPolicy").Invoke();
    Require(
        restrictedResult.Output.Records.Count == 1 &&
        restrictedResult.Output.Records[0].DisplayText == "Restricted",
        "The restricted execution-policy subset was not applied.");
    string unapprovedScript = Path.Combine(Path.GetTempPath(), $"pwsh-sdk-ffi-{Guid.NewGuid():N}.ps1");
    File.WriteAllText(unapprovedScript, "'unapproved external script'");
    try
    {
        using PowerShell unapprovedScriptPowerShell = restrictedSession.CreatePowerShell();
        bool unapprovedScriptRejected;
        try
        {
            PowerShellInvocationResult unapprovedResult = unapprovedScriptPowerShell
                .AddScript($"& '{unapprovedScript.Replace("'", "''", StringComparison.Ordinal)}'")
                .Invoke();
            unapprovedScriptRejected = unapprovedResult.HadErrors;
        }
        catch (PowerShellInvocationException)
        {
            unapprovedScriptRejected = true;
        }
        Require(
            unapprovedScriptRejected,
            "The restricted session ran an external script outside its approved staged module roots.");
    }
    finally
    {
        File.Delete(unapprovedScript);
    }
}

PowerShellSession leasedSession = runtime.CreateSession(
    new PowerShellSessionOptions());
PowerShell leasedBuilder = leasedSession.CreatePowerShell();
leasedSession.Dispose();
leasedSession.Dispose();
try
{
    leasedSession.GetSnapshot();
    return 1;
}
catch (ObjectDisposedException)
{
}

PowerShellInvocationResult leasedResult = leasedBuilder
    .AddScript("'builder-outlives-session'")
    .Invoke();
Require(
    leasedResult.Output.Records.Count == 1 &&
    leasedResult.Output.Records[0].DisplayText == "builder-outlives-session",
    "A builder did not retain its session lease after public session disposal.");
leasedBuilder.Dispose();
leasedBuilder.Dispose();

try
{
    runtime.CreateSessionPool(new PowerShellSessionPoolOptions(1, 1));
    return 1;
}
catch (PowerShellFfiException exception)
    when (exception.Status == PowerShellFfiStatus.UnsupportedCapability)
{
}

await VerifyCancellationAndDisposeAsync();
await VerifySafeHandleDisposeRacesAsync();
await VerifyDuplexBrokerChannelAsync();

Console.WriteLine("FFI package consumer: Success");
return 0;

async Task VerifyDuplexBrokerChannelAsync()
{
    try
    {
        _ = new PowerShellBrokerChannelOptions(defaultDeadline: TimeSpan.FromTicks(1));
        throw new InvalidOperationException("A sub-millisecond broker deadline was accepted.");
    }
    catch (ArgumentOutOfRangeException)
    {
    }

    using PowerShellBrokerChannel channel = runtime.CreateBrokerChannel(
        new PowerShellBrokerChannelOptions(
            maximumInflightFrames: 8,
            maximumBodyBytes: 4096,
            defaultDeadline: TimeSpan.FromSeconds(20)));

    int observedRequests = 0;
    int observedEvents = 0;
    Exception? pumpFailure = null;
    using var stopping = new CancellationTokenSource();
    using var pumpReady = new ManualResetEventSlim();

    var pump = new Thread(() =>
    {
        try
        {
            // Register the consumer before the payload starts. Thread.Start
            // alone does not guarantee that broker_wait has run.
            if (!channel.TryReceive(TimeSpan.Zero, out _))
            {
                throw new InvalidOperationException("The broker channel closed before the pump attached.");
            }

            pumpReady.Set();
            while (!stopping.IsCancellationRequested &&
                channel.TryReceive(TimeSpan.FromMilliseconds(100), out PowerShellBrokerRequest? received))
            {
                if (received is null)
                {
                    continue;
                }

                if (received.IsOneWay)
                {
                    Interlocked.Increment(ref observedEvents);
                    continue;
                }

                Interlocked.Increment(ref observedRequests);

                // Dispatch-only: hand the copied request to an ordinary worker
                // and return to waiting. The reply happens on another thread.
                PowerShellBrokerRequest captured = received;
                _ = Task.Run(() =>
                {
                    byte[] reply = new byte[captured.Body.Length];
                    for (int index = 0; index < captured.Body.Length; index++)
                    {
                        reply[index] = (byte)(captured.Body[index] + 1);
                    }

                    channel.TryReply(captured.CorrelationId, reply);
                });
            }
        }
        catch (Exception exception)
        {
            pumpFailure = exception;
            pumpReady.Set();
        }
    })
    {
        IsBackground = true,
        Name = "package-broker-pump",
    };
    pump.Start();

    try
    {
        Require(
            pumpReady.Wait(TimeSpan.FromSeconds(5)),
            "The packaged broker pump did not attach.");
        Require(pumpFailure is null, "The packaged broker pump failed: " + pumpFailure?.Message);

        using (PowerShell synchronous = PowerShell.Create())
        {
            synchronous.AddScript("'unused'").WithBroker(channel);
            try
            {
                synchronous.Invoke();
                throw new InvalidOperationException(
                    "A synchronous invocation with a broker attached was not rejected.");
            }
            catch (PowerShellFfiException exception)
                when (exception.Status == PowerShellFfiStatus.UnsupportedCapability)
            {
            }
        }

        using PowerShell command = PowerShell.Create();
        PowerShellInvocationResult brokerResult = await command
            .AddScript(
                "`$DpsBroker.Post(2, [byte[]]@(9));" +
                "`$reply = `$DpsBroker.Request(1, [byte[]]@(1,2,3));" +
                "[string]::Join('-', `$reply)")
            .WithBroker(channel)
            .InvokeAsync(CancellationToken.None);

        Require(pumpFailure is null, "The packaged broker pump failed: " + pumpFailure?.Message);
        Require(
            brokerResult.Output.Records.Count == 1 &&
                brokerResult.Output.Records[0].DisplayText == "2-3-4",
            "The packaged duplex broker request did not receive its dispatched reply.");
        Require(
            Volatile.Read(ref observedRequests) == 1,
            "The packaged broker pump did not observe exactly one request.");
        Require(
            Volatile.Read(ref observedEvents) == 1,
            "The packaged broker pump did not observe the one-way event.");

        // A reply for an unknown correlation must fail closed rather than
        // resurrecting or reaching another frame.
        Require(
            !channel.TryReply(ulong.MaxValue, ReadOnlySpan<byte>.Empty),
            "The packaged broker accepted a reply for an unknown correlation.");
    }
    finally
    {
        stopping.Cancel();
        channel.Dispose();
        pump.Join(TimeSpan.FromSeconds(5));
    }

    // A disposed channel is inert rather than throwing.
    Require(
        !channel.TryReply(1, ReadOnlySpan<byte>.Empty),
        "A disposed broker channel accepted a reply.");

    Console.WriteLine("FFI package consumer duplex broker channel: Success");
}

[PowerShellDtoContract(1)]
public sealed class PackageProjectionDto
{
    [PowerShellDtoMember(MaximumStringLength = 64)]
    public string Name { get; set; } = string.Empty;

    [PowerShellDtoMember]
    public long Count { get; set; }
}

sealed class LabelCapability : IPowerShellCapabilityHandler
{
    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        if (invocation.Definition.Name != "example.get-label" ||
            arguments.Count != 0 ||
            invocation.CancellationToken.IsCancellationRequested)
        {
            throw new InvalidOperationException("Capability contract was not preserved.");
        }

        return PowerShellValue.String("nativeaot-label");
    }
}

sealed class ThrowingCapability : IPowerShellCapabilityHandler
{
    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        throw new InvalidOperationException("The callback exception must not escape to native code.");
    }
}

sealed class TimeoutCapability : IPowerShellCapabilityHandler
{
    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        Thread.Sleep(100);
        return PowerShellValue.String("late-response");
    }
}

sealed class ReentryCapability : IPowerShellCapabilityHandler
{
    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        try
        {
            using PowerShell reentrant = PowerShell.Create();
            throw new InvalidOperationException("The capability handler unexpectedly re-entered the FFI.");
        }
        catch (PowerShellFfiException exception)
            when (exception.Status == PowerShellFfiStatus.Backpressure)
        {
            return PowerShellValue.String("reentry-blocked");
        }
    }
}

sealed class CancellableCapability : IPowerShellCapabilityHandler, IDisposable
{
    public ManualResetEventSlim Started { get; } = new(false);

    public bool CancellationObserved { get; private set; }

    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        Started.Set();
        while (!invocation.CancellationToken.IsCancellationRequested)
        {
            Thread.Sleep(10);
        }

        CancellationObserved = true;
        invocation.CancellationToken.ThrowIfCancellationRequested();
        return PowerShellValue.Null;
    }

    public void Dispose()
    {
        Started.Dispose();
    }
}

sealed class StagedIntentHandler : IPowerShellStagedIntentHandler, IDisposable
{
    private readonly object gate = new();
    private readonly Dictionary<string, string> staged = new(StringComparer.Ordinal);
    private bool cancellationObserved;
    private int stageCalls;
    private int abortCalls;
    private string? abortedName;
    private string? committedName;

    public string? CommittedName
    {
        get
        {
            lock (gate)
            {
                return committedName;
            }
        }
    }

    public string? AbortedName
    {
        get
        {
            lock (gate)
            {
                return abortedName;
            }
        }
    }

    public int StageCalls
    {
        get
        {
            lock (gate)
            {
                return stageCalls;
            }
        }
    }

    public int AbortCalls
    {
        get
        {
            lock (gate)
            {
                return abortCalls;
            }
        }
    }

    public int RetainedStageCount
    {
        get
        {
            lock (gate)
            {
                return staged.Count;
            }
        }
    }

    public ManualResetEventSlim CancellationStarted { get; } = new(false);

    public ManualResetEventSlim CommitStarted { get; } = new(false);

    public ManualResetEventSlim ReleaseCommit { get; } = new(false);

    public bool CancellationObserved
    {
        get
        {
            lock (gate)
            {
                return cancellationObserved;
            }
        }
    }

    public PowerShellStagedIntentHandlerResult Invoke(PowerShellStagedIntentInvocation invocation)
    {
        IReadOnlyDictionary<string, PowerShellValue> properties = invocation.Intent.Intent.GetPropertyBag();
        if (invocation.Intent.OperationName != "example.intent" ||
            properties.Count != 2 ||
            !properties.TryGetValue("Id", out PowerShellValue? identifier) ||
            !identifier.TryGetString(out string? identifierText) ||
            identifierText != invocation.Intent.StageIdentifier ||
            !properties.TryGetValue("Name", out PowerShellValue? name) ||
            !name.TryGetString(out string? nameText) ||
            string.IsNullOrWhiteSpace(nameText))
        {
            return PowerShellStagedIntentHandlerResult.Reject("The staged intent payload is invalid.");
        }

        if (invocation.Operation == PowerShellStagedIntentOperation.Stage &&
            invocation.Intent.StageIdentifier == "intent-cancelled")
        {
            CancellationStarted.Set();
            while (!invocation.CancellationToken.IsCancellationRequested)
            {
                Thread.Sleep(10);
            }

            lock (gate)
            {
                cancellationObserved = true;
            }
            invocation.CancellationToken.ThrowIfCancellationRequested();
        }

        if (invocation.Operation == PowerShellStagedIntentOperation.Commit &&
            invocation.Intent.StageIdentifier == "intent-commit-expiry")
        {
            CommitStarted.Set();
            if (!ReleaseCommit.Wait(TimeSpan.FromSeconds(5)))
            {
                throw new TimeoutException("The commit race test did not release the handler.");
            }
        }

        return invocation.Operation switch
        {
            PowerShellStagedIntentOperation.Stage => Stage(invocation.Intent.StageIdentifier, nameText),
            PowerShellStagedIntentOperation.Validate => Validate(invocation.Intent.StageIdentifier),
            PowerShellStagedIntentOperation.Commit => Commit(invocation.Intent.StageIdentifier),
            PowerShellStagedIntentOperation.Abort => Abort(invocation.Intent.StageIdentifier),
            _ => throw new ArgumentOutOfRangeException(nameof(invocation)),
        };
    }

    public void Dispose()
    {
        CancellationStarted.Dispose();
        CommitStarted.Dispose();
        ReleaseCommit.Dispose();
    }

    private PowerShellStagedIntentHandlerResult Stage(string stageIdentifier, string name)
    {
        lock (gate)
        {
            stageCalls++;
            return staged.TryAdd(stageIdentifier, name)
                ? PowerShellStagedIntentHandlerResult.Accept()
                : PowerShellStagedIntentHandlerResult.Reject("The stage already exists.");
        }
    }

    private PowerShellStagedIntentHandlerResult Validate(string stageIdentifier)
    {
        lock (gate)
        {
            return staged.ContainsKey(stageIdentifier)
                ? PowerShellStagedIntentHandlerResult.Accept()
                : PowerShellStagedIntentHandlerResult.Reject("The stage is missing.");
        }
    }

    private PowerShellStagedIntentHandlerResult Commit(string stageIdentifier)
    {
        lock (gate)
        {
            if (!staged.Remove(stageIdentifier, out string? name))
            {
                return PowerShellStagedIntentHandlerResult.Reject("The stage is missing.");
            }

            committedName = name;
            return PowerShellStagedIntentHandlerResult.Accept();
        }
    }

    private PowerShellStagedIntentHandlerResult Abort(string stageIdentifier)
    {
        lock (gate)
        {
            abortCalls++;
            if (staged.Remove(stageIdentifier, out string? name))
            {
                abortedName = name;
            }

            return PowerShellStagedIntentHandlerResult.Accept();
        }
    }
}

sealed class HostInteractionCapability : IPowerShellCapabilityHandler
{
    public int CallCount { get; private set; }

    public string? Text { get; private set; }

    public PowerShellProgressUpdate? Progress { get; private set; }

    public int PromptCount { get; private set; }

    public PowerShellValue Invoke(
        PowerShellCapabilityInvocation invocation,
        IReadOnlyList<PowerShellValue> arguments)
    {
        CallCount++;
        switch (invocation.Definition.Name)
        {
            case "host.write-text":
                if (arguments.Count != 1 ||
                    !arguments[0].TryGetString(out string? text) ||
                    text != "host-text")
                {
                    throw new ArgumentException("The host text capability payload is invalid.");
                }

                Text = text;
                return PowerShellValue.Null;
            case "host.report-progress":
                if (arguments.Count != 1)
                {
                    throw new ArgumentException("The host progress capability payload is invalid.");
                }

                Progress = PowerShellHostInteraction.ParseProgressUpdate(arguments[0]);
                return PowerShellValue.Null;
            case "host.prompt-choice":
                if (arguments.Count != 1 || arguments[0].Kind != PowerShellValueKind.PropertyBag)
                {
                    throw new ArgumentException("The host choice capability payload is invalid.");
                }

                PromptCount++;
                return PowerShellValue.SignedInteger(1);
            default:
                throw new ArgumentException("The host capability is not registered.");
        }
    }
}
"@ | Set-Content -Path (Join-Path $consumerDirectory 'Program.cs') -Encoding utf8

    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('restore', $consumerProject, '--configfile', $nugetConfig)
        Assert-RestoredPackageMatchesInspectedNupkg -NugetCache $nugetCache -PackageId $packageId -PackageVersion $PackageVersion -InspectedPackagePath $package.FullName
        Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('publish', $consumerProject, '--no-restore', '-c', $Configuration)

    $publishDirectory = Join-Path $consumerDirectory "bin\$Configuration\net10.0\win-x64\publish"
    $consumerExe = Join-Path $publishDirectory 'FfiPackageConsumer.exe'
    $nativeAsset = Join-Path $publishDirectory 'multi-pwsh-sdk.dll'
    if (-not (Test-Path $consumerExe -PathType Leaf) -or -not (Test-Path $nativeAsset -PathType Leaf)) {
        throw 'The published package consumer is missing its executable or native FFI asset.'
    }
    $nativeVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($nativeAsset)
    if ($nativeVersion.FileVersion -ne $multiPwshVersion -or
        $nativeVersion.ProductVersion -ne $multiPwshVersion -or
        $nativeVersion.OriginalFilename -ne 'multi-pwsh-sdk.dll') {
        throw "The packaged SDK native asset version metadata does not match multi-pwsh $multiPwshVersion."
    }

    Write-Host ">> $consumerExe $payloadDirectory"
    $consumerOutput = & $consumerExe $payloadDirectory
    $consumerOutput | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $consumerExe $payloadDirectory"
    }

    $reportedVersionLine = $consumerOutput |
        Where-Object { $_ -like 'FFI package consumer PowerShell file version: *' } |
        Select-Object -First 1
    if ($null -eq $reportedVersionLine) {
        throw 'The package consumer did not report the PowerShell file version from runtime diagnostics.'
    }

    # The duplex broker must be exercised through the restored package's public
    # facade and staged native asset, not through a source project reference.
    if (-not ($consumerOutput | Where-Object { $_ -eq 'FFI package consumer duplex broker channel: Success' })) {
        throw 'The package consumer did not exercise the duplex broker channel through the packed SDK.'
    }

    $bridgeConsumerDirectory = Join-Path $workspace 'bridge-consumer'
    $bridgePackDirectory = Join-Path $workspace 'bridge-pack'
    New-Item -Path $bridgeConsumerDirectory -ItemType Directory -Force | Out-Null
    New-Item -Path $bridgePackDirectory -ItemType Directory -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'dotnet\interop\BridgeTestContract.cs') `
        -Destination (Join-Path $bridgeConsumerDirectory 'BridgeTestContract.cs')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'dotnet\bridge-contract-test-host\BridgeTestCountHost.cs') `
        -Destination (Join-Path $bridgeConsumerDirectory 'BridgeTestCountHost.cs')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'dotnet\interop\BridgeTestContract.cs') `
        -Destination (Join-Path $bridgePackDirectory 'BridgeTestContract.cs')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'dotnet\bridge-contract-test-pack\BridgeContractTestPack.cs') `
        -Destination (Join-Path $bridgePackDirectory 'BridgeContractTestPack.cs')

    $bridgePackProject = Join-Path $bridgePackDirectory 'FfiPackageBridgePack.csproj'
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <LiveContractMode>Payload</LiveContractMode>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="$packageId" Version="$PackageVersion" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $bridgePackProject -Encoding utf8

    $bridgeConsumerProject = Join-Path $bridgeConsumerDirectory 'FfiPackageBridgeConsumer.csproj'
    @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <SelfContained>true</SelfContained>
    <PublishAot>true</PublishAot>
    <InvariantGlobalization>true</InvariantGlobalization>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <Nullable>enable</Nullable>
    <LiveContractMode>Host</LiveContractMode>
    <DevolutionsMultiPwshSdkEnabled>true</DevolutionsMultiPwshSdkEnabled>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="$packageId" Version="$PackageVersion" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $bridgeConsumerProject -Encoding utf8

    @"
#nullable enable
using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Devolutions.MultiPwsh.BridgeTest;
using Devolutions.PowerShell.Ffi;
using Devolutions.PowerShell.Ffi.LiveObjects;

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

static PowerShellSecret CreateFixtureSecret()
{
    char[] characters =
    [
        (char)102, (char)105, (char)120, (char)116, (char)117, (char)114, (char)101,
        (char)45, (char)115, (char)101, (char)99, (char)114, (char)101, (char)116,
    ];
    try
    {
        return PowerShellSecret.Create(characters);
    }
    finally
    {
        Array.Clear(characters);
    }
}

static void VerifySecretBindings(PowerShellRuntime runtime)
{
    using PowerShellSession session = runtime.CreateSession(
        new PowerShellSessionOptions(
            historyMode: PowerShellSessionHistoryMode.Disabled,
            errorPreference: PowerShellSessionPreference.Stop,
            warningPreference: PowerShellSessionPreference.Continue));

    using (PowerShellSecret secret = CreateFixtureSecret())
    using (PowerShellCredential credential = new("fixture-user", secret))
    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellSecretResult result = pipeline
        .AddScript(
            "param([System.Management.Automation.PSCredential]`$Credential) " +
            "if (`$Credential.UserName -ne 'fixture-user' -or " +
            "`$Credential.GetNetworkCredential().Password.Length -eq 0) { throw 'credential binding failed' }")
        .AddParameter("Credential", credential)
        .InvokeWithSecretBindings())
    {
        Require(result.Kind == PowerShellSecretResultKind.None,
            "The contract-pack credential input binding failed.");
    }

    using (PowerShellSecret secret = CreateFixtureSecret())
    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellSecretResult result = pipeline
        .AddScript("param([System.Security.SecureString]`$Secret) `$Secret")
        .AddParameter("Secret", secret)
        .InvokeSecretResult(PowerShellSecretResultKind.SecureString))
    {
        Require(result.Kind == PowerShellSecretResultKind.SecureString && result.Secret is not null,
            "The contract-pack SecureString result binding failed.");
    }

    using (PowerShellSecret secret = CreateFixtureSecret())
    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellSecretResult result = pipeline
        .AddScript(
            "param([System.Security.SecureString]`$Secret) " +
            "[System.Management.Automation.PSCredential]::new('fixture-user', `$Secret)")
        .AddParameter("Secret", secret)
        .InvokeSecretResult(PowerShellSecretResultKind.Credential))
    {
        Require(
            result.Kind == PowerShellSecretResultKind.Credential &&
            result.Credential?.UserName == "fixture-user",
            "The contract-pack PSCredential result binding failed.");
    }

    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellCredentialResult result = pipeline
        .AddScript(
            "`$Result.Username = 'fixture-user'; " +
            "`$Result.Domain = 'fixture-domain'; " +
            "`$Result.Password = 'plain-fixture'; " +
            "`$Result.OutputMessages = @('output-one', 'output-two'); " +
            "`$Result.ErrorMessages = @('error-one'); " +
            "`$Result.LogMessage = 'fixture-log'")
        .InvokeCredentialResult())
    {
        Require(
            result.Username == "fixture-user" &&
            result.Domain == "fixture-domain" &&
            result.OutputMessages.SequenceEqual(["output-one", "output-two"]) &&
            result.ErrorMessages.SequenceEqual(["error-one"]) &&
            result.LogMessage == "fixture-log" &&
            !result.IsCancelled &&
            result.Password is not null,
            "The bounded plaintext credential result projection failed.");
        Require(
            !result.ToString().Contains("plain-fixture", StringComparison.Ordinal) &&
            !result.Password.ToString().Contains("plain-fixture", StringComparison.Ordinal),
            "The plaintext credential result leaked through diagnostics.");

        char[] password = new char[result.Password.Length];
        try
        {
            result.Password.CopyTo(password);
            Require(
                password.AsSpan().SequenceEqual("plain-fixture".AsSpan()),
                "PowerShellSecret.CopyTo did not transfer the credential result.");

            using var securePassword = result.Password.ToSecureString();
            Require(
                securePassword.IsReadOnly() &&
                securePassword.Length == password.Length,
                "PowerShellSecret.ToSecureString did not return an independent read-only copy.");
        }
        finally
        {
            Array.Clear(password);
        }
    }

    using (PowerShell pipeline = PowerShell.Create())
    using (PowerShellCredentialResult result = pipeline
        .AddScript(
            "`$Result.Username = 'standalone-user'; " +
            "`$Result.Password = 'standalone-fixture'")
        .InvokeCredentialResult())
    {
        Require(
            result.Username == "standalone-user" &&
            result.Password is not null &&
            result.Password.Length == "standalone-fixture".Length,
            "The standalone credential result invocation did not create an isolated runspace.");
    }

    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellCredentialResult result = pipeline
        .AddScript(
            "`$Result.Username = 'secure-user'; " +
            "`$Result.SecurePassword = ConvertTo-SecureString 'secure-fixture' -AsPlainText -Force")
        .InvokeCredentialResult())
    {
        Require(
            result.Username == "secure-user" &&
            result.Password is not null &&
            result.Password.Length == "secure-fixture".Length,
            "CreateDefault2 did not expose the controlled ConvertTo-SecureString credential result path.");

        char[] password = new char[result.Password.Length];
        try
        {
            result.Password.CopyTo(password);
            Require(
                password.AsSpan().SequenceEqual("secure-fixture".AsSpan()),
                "The SecureString credential result did not preserve its contents.");
        }
        finally
        {
            Array.Clear(password);
        }
    }

    using (PowerShell pipeline = session.CreatePowerShell())
    using (PowerShellCredentialResult result = pipeline
        .AddScript("`$Result.Cancel = `$true; `$Result.Password = 'discarded-fixture'")
        .InvokeCredentialResult())
    {
        Require(
            result.IsCancelled && result.Password is null,
            "A cancelled credential result retained password material.");
    }

    using (PowerShell pipeline = session.CreatePowerShell())
    {
        try
        {
            pipeline
                .AddScript("`$Result.Username = 'fixture-user'; 'ordinary-output'")
                .InvokeCredentialResult();
            throw new InvalidOperationException("Credential result invocation accepted ordinary pipeline output.");
        }
        catch (PowerShellFfiException exception)
        {
            Require(
                !exception.Message.Contains("ordinary-output", StringComparison.Ordinal),
                "Rejected credential-result output leaked through diagnostics.");
        }
    }

    using (PowerShell pipeline = session.CreatePowerShell())
    {
        try
        {
            pipeline
                .AddScript("`$Result.Password = 7")
                .InvokeCredentialResult();
            throw new InvalidOperationException("Credential result invocation accepted an unsafe password value.");
        }
        catch (PowerShellFfiException exception)
        {
            Require(
                !exception.Message.Contains("Password", StringComparison.Ordinal),
                "Rejected credential-result values leaked sink details through diagnostics.");
        }
    }
}

if (args.Length != 1)
{
    throw new ArgumentException("Expected exactly one PowerShell payload directory.");
}

string packPath = Path.Combine(AppContext.BaseDirectory, "FfiPackageBridgePack.dll");
Require(File.Exists(packPath), "The package-only bridge payload pack was not published beside the consumer.");

PowerShellRuntime runtime = PowerShellRuntime.Activate(
    args[0],
    [new PowerShellLiveObjectContractPack(
        packPath,
        "Devolutions.MultiPwsh.BridgeTest.BridgeContractTestPack, FfiPackageBridgePack")]);
VerifySecretBindings(runtime);
using PowerShellBridgeChannel channel = runtime.CreateBridgeChannel(
    new PowerShellBrokerChannelOptions(
        maximumInflightFrames: 8,
        maximumBodyBytes: 4096,
        defaultDeadline: TimeSpan.FromSeconds(20)));
using var host = new BridgeTestCountHost(41);
using PowerShellBridgeBinding binding = channel.CreateBinding(host.Dispatcher);
using var stopping = new CancellationTokenSource();
using var ready = new ManualResetEventSlim();
Exception? pumpFailure = null;
int requests = 0;
int events = 0;

var pump = new Thread(() =>
{
    try
    {
        if (!channel.TryReceive(TimeSpan.Zero, out _))
        {
            throw new InvalidOperationException("The package-only generated bridge channel closed before its pump attached.");
        }

        ready.Set();
        while (!stopping.IsCancellationRequested &&
            channel.TryReceive(TimeSpan.FromMilliseconds(100), out PowerShellBridgeDispatch? dispatch))
        {
            if (dispatch is null)
            {
                continue;
            }

            _ = Task.Run(() =>
            {
                bool replied = dispatch.Dispatch();
                if (replied)
                {
                    Interlocked.Increment(ref requests);
                }
            });
            Interlocked.Increment(ref events);
        }
    }
    catch (Exception exception)
    {
        pumpFailure = exception;
        ready.Set();
    }
})
{
    IsBackground = true,
    Name = "package-generated-bridge-pump",
};
pump.Start();

try
{
    Require(ready.Wait(TimeSpan.FromSeconds(5)) && pumpFailure is null,
        "The package-only generated bridge pump did not attach.");

    using (PowerShell synchronous = runtime.Create())
    {
        synchronous.AddScript("'unused'").WithBridge(binding, "RDM");
        try
        {
            _ = synchronous.Invoke();
            throw new InvalidOperationException("A synchronous invocation with a generated bridge attached was accepted.");
        }
        catch (PowerShellFfiException exception)
            when (exception.Status == PowerShellFfiStatus.UnsupportedCapability)
        {
        }
    }

    using PowerShell command = runtime.Create();
    PowerShellInvocationResult output = await command
        .AddScript("`$RDM.Report(77); \"`$(`$RDM.Count)|`$(`$RDM.Add(5))\"")
        .WithBridge(binding, "RDM")
        .InvokeAsync(CancellationToken.None);

    Require(
        pumpFailure is null &&
        output.Output.Records.Count == 1 &&
        output.Output.Records[0].DisplayText == "41|46" &&
        SpinWait.SpinUntil(() => host.LastReportedCount == 77, TimeSpan.FromSeconds(5)) &&
        Volatile.Read(ref requests) >= 2 &&
        Volatile.Read(ref events) >= 3,
        "The package-only generated bridge did not complete its bounded deferred request and event dispatch.");

    using (PowerShell finiteJob = runtime.Create())
    {
        PowerShellInvocationResult jobOutput = await finiteJob
            .AddScript("`$job = `$RDM.StartJob(); `$before = `$job.Status; `$page = `$job.ReadResults([Guid]::Empty, 17, 29); `$terminal = `$job.ReadResults(`$page.NextCursor, 17, 29); `$gap = `$job.ReadResults([Guid]::Empty, 18, 29); `$job.Cancel(); `$job.Cancel(); \"`$(`$before.State)|`$(`$page.Columns[2].Name)|`$(`$page.Rows[0].Label)|`$(@(`$page.Rows).Count)|`$(`$page.SnapshotRevision)|`$(`$page.PermissionRevision)|`$(`$terminal.Rows[0].Label)|`$(`$terminal.IsTerminal)|`$(`$gap.IsGap)|`$(`$job.Status.IsTerminal)\"")
            .WithBridge(binding, "RDM")
            .InvokeAsync(CancellationToken.None);
        Require(
            jobOutput.Output.Records.Count == 1 &&
            jobOutput.Output.Records[0].DisplayText == "Running|Label|result-10|2|17|29|result-30|True|True|True" &&
            host.JobCancelDispatchCount == 1,
            "The package-only generated bridge did not preserve its finite-operation page, revision gap, or first-wins cancellation.");
    }

    using PowerShell reliableEvent = runtime.Create();
    Task<PowerShellInvocationResult> reliableEventInvocation = reliableEvent
        .AddScript("`$RDM.ReportReliable(88); Start-Sleep -Milliseconds 500")
        .WithBridge(binding, "RDM")
        .InvokeAsync(CancellationToken.None);
    PowerShellBridgeReliableEventStream? stream = null;
    Require(
        SpinWait.SpinUntil(
            () => (stream = channel.GetReliableEventStreams(binding)
                .SingleOrDefault(candidate => candidate.Identity.MemberId == 6U)) is not null,
            TimeSpan.FromSeconds(5)) &&
        stream is not null,
        "The package-only generated bridge did not retain a reliable event stream.");
    PowerShellBridgeReliableEventStream reliableStream = stream
        ?? throw new InvalidOperationException("The package-only reliable event stream was lost after discovery.");
    PowerShellBridgeReliableEventBatch reliableBatch = reliableStream.Read(0, 2);
    Require(
        reliableBatch.Events.Count == 1 &&
        reliableBatch.Events[0].Sequence == 1 &&
        !reliableBatch.Info.IsTerminal,
        "The package-only generated bridge did not assign a reliable event cursor.");
    await Task.Run(reliableBatch.Events[0].Dispatch);
    reliableStream.Acknowledge(reliableBatch.Events[0].Sequence);
    Require(
        host.LastReliableReportedCount == 88 &&
        host.ReliableReportCount == 1,
        "The package-only generated bridge did not dispatch a retained reliable event.");
    _ = await reliableEventInvocation;
    Require(
        reliableStream.GetInfo().TerminalState == PowerShellBridgeReliableEventTerminalState.LeaseClosed,
        "The package-only generated bridge did not close the completed reliable-event lease.");

    using var reliableCancellation = new CancellationTokenSource();
    using PowerShell reliableCancelled = runtime.Create();
    Task<PowerShellInvocationResult> reliableCancelledInvocation = reliableCancelled
        .AddScript("`$RDM.ReportReliable(55); Start-Sleep -Seconds 20")
        .WithBridge(binding, "RDM")
        .InvokeAsync(reliableCancellation.Token);
    PowerShellBridgeReliableEventStream? cancelledStream = null;
    Require(
        SpinWait.SpinUntil(
            () =>
            {
                cancelledStream = channel.GetReliableEventStreams(binding)
                    .SingleOrDefault(candidate =>
                        candidate.Identity.MemberId == 6U &&
                        candidate.Identity.LeaseId != reliableStream.Identity.LeaseId);
                return cancelledStream is not null;
            },
            TimeSpan.FromSeconds(5)),
        "The package-only generated bridge did not retain a cancellable reliable event.");
    PowerShellBridgeReliableEventStream cancelledReliableStream = cancelledStream
        ?? throw new InvalidOperationException("The package-only cancellable reliable event stream was lost after discovery.");
    PowerShellBridgeReliableEventBatch cancelledBatch = cancelledReliableStream.Read(0, 1);
    Require(
        cancelledBatch.Events.Count == 1 &&
        cancelledBatch.Events[0].Sequence == 1,
        "The package-only generated bridge did not expose the cancellable reliable event before cleanup.");
    reliableCancellation.Cancel();
    try
    {
        _ = await reliableCancelledInvocation;
        throw new InvalidOperationException("The package-only reliable event invocation completed after cancellation.");
    }
    catch (OperationCanceledException)
    {
    }

    Require(
        SpinWait.SpinUntil(
            () => cancelledReliableStream.GetInfo().TerminalState == PowerShellBridgeReliableEventTerminalState.LeaseClosed,
            TimeSpan.FromSeconds(5)) &&
        cancelledReliableStream.GetInfo().DroppedEventCount == 1,
        "The package-only generated bridge did not terminalize retained reliable events on cancellation.");
    await Task.Run(cancelledBatch.Events[0].Dispatch);
    Require(
        host.ReliableReportCount == 1,
        "The package-only generated bridge dispatched a reliable event after cancellation released it.");
}
finally
{
    stopping.Cancel();
    channel.Dispose();
    pump.Join(TimeSpan.FromSeconds(5));
}

Console.WriteLine("FFI package consumer generated bridge attachment: Success");
"@ | Set-Content -Path (Join-Path $bridgeConsumerDirectory 'Program.cs') -Encoding utf8

    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('restore', $bridgePackProject, '--configfile', $nugetConfig)
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('restore', $bridgeConsumerProject, '--configfile', $nugetConfig)
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('build', $bridgePackProject, '--no-restore', '-c', $Configuration)
    Invoke-CheckedCommand -FilePath dotnet -ArgumentList @('publish', $bridgeConsumerProject, '--no-restore', '-c', $Configuration)

    $bridgePackAssembly = Join-Path $bridgePackDirectory "bin\$Configuration\net8.0\FfiPackageBridgePack.dll"
    $bridgePublishDirectory = Join-Path $bridgeConsumerDirectory "bin\$Configuration\net10.0\win-x64\publish"
    $bridgeConsumerExe = Join-Path $bridgePublishDirectory 'FfiPackageBridgeConsumer.exe'
    if (-not (Test-Path $bridgePackAssembly -PathType Leaf) -or
        -not (Test-Path $bridgeConsumerExe -PathType Leaf)) {
        throw 'The package-only generated bridge consumer did not publish its pack or executable.'
    }
    Copy-Item -LiteralPath $bridgePackAssembly -Destination (Join-Path $bridgePublishDirectory 'FfiPackageBridgePack.dll')

    Write-Host ">> $bridgeConsumerExe $payloadDirectory"
    $bridgeConsumerOutput = & $bridgeConsumerExe $payloadDirectory
    $bridgeConsumerOutput | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $bridgeConsumerExe $payloadDirectory"
    }
    if (-not ($bridgeConsumerOutput | Where-Object { $_ -eq 'FFI package consumer generated bridge attachment: Success' })) {
        throw 'The package-only consumer did not exercise the generated bridge attachment through the packed SDK.'
    }

    $reportedVersion = $reportedVersionLine.Substring('FFI package consumer PowerShell file version: '.Length).Trim()
    if ($reportedVersion -ne 'unreported' -and
        $reportedVersion -ne $script:QualifiedPowerShellVersion -and
        -not $reportedVersion.StartsWith("$($script:QualifiedPowerShellVersion).", [StringComparison]::Ordinal)) {
        throw "Runtime diagnostics reported PowerShell '$reportedVersion' but the qualified payload is $($script:QualifiedPowerShellVersion)."
    }

    Write-Host "Qualified PowerShell payload: $($script:QualifiedPowerShellVersion) (runtime diagnostics reported '$reportedVersion')"
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_STEP_SUMMARY)) {
        "Package harness qualified PowerShell $($script:QualifiedPowerShellVersion) (runtime diagnostics reported ``$reportedVersion``)." |
            Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Encoding utf8 -Append
    }
}
finally {
    if ($null -eq $oldNugetPackages) {
        Remove-Item Env:NUGET_PACKAGES -ErrorAction SilentlyContinue
    }
    else {
        $env:NUGET_PACKAGES = $oldNugetPackages
    }
    if ($null -eq $oldNugetFallbackPackages) {
        Remove-Item Env:NUGET_FALLBACK_PACKAGES -ErrorAction SilentlyContinue
    }
    else {
        $env:NUGET_FALLBACK_PACKAGES = $oldNugetFallbackPackages
    }

    if ($KeepWorkspace) {
        Write-Host "Kept smoke workspace: $workspace"
            Write-Host "Kept isolated NuGet cache: $nugetCache"
        }
        else {
            Remove-Item -Path $workspace -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Item -Path $nugetCache -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
