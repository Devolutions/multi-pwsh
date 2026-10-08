using System.Text;

namespace Devolutions.PowerShell.Ffi;

public enum PowerShellPayloadTableShape : uint
{
    V1 = 1,
}

/// <summary>
/// A safe logical identity for a live-object contract pack registered at activation.
/// </summary>
public sealed class PowerShellLiveObjectContractPackIdentity
{
    internal PowerShellLiveObjectContractPackIdentity(string payloadAdapterTypeName)
    {
        PayloadAdapterTypeName = payloadAdapterTypeName;
    }

    /// <summary>
    /// The registered payload adapter type name. Assembly paths are intentionally not reported.
    /// </summary>
    public string PayloadAdapterTypeName { get; }
}

/// <summary>
/// Immutable descriptive facts about the active PowerShell FFI deployment.
/// </summary>
/// <remarks>
/// This report does not attest payload integrity, evaluate deployment policy, inspect
/// environment variables, or expose secrets, assembly paths, or payload objects.
/// A null <see cref="PowerShellFileVersion"/> means the payload did not make that
/// descriptive file-version value available.
/// <para>
/// <see cref="PayloadDirectory"/> is the runtime's canonicalized active payload
/// directory. It is not a verbatim echo of an activation argument: on Windows it is an
/// extended-length <c>\\?\</c>-prefixed path, and for <c>PATH</c>-based activation there
/// is no caller-supplied path at all because the runtime discovers it. Do not compare it
/// against an input string. It is a local filesystem path and may embed a user profile
/// directory, so redact it before writing this report to shared logs or external telemetry.
/// </para>
/// </remarks>
public sealed class PowerShellRuntimeDiagnosticReport
{
    internal PowerShellRuntimeDiagnosticReport(
        string payloadDirectory,
        string? powerShellFileVersion,
        uint bindingsAbiVersion,
        nuint payloadTableSize,
        uint payloadTableSlotCount,
        PowerShellPayloadTableShape payloadTableShape,
        ulong featureFlags,
        IReadOnlyList<PowerShellLiveObjectContractPackIdentity> registeredLiveObjectContractPacks,
        PowerShellRemoteProviderDiagnostic managedWsManProvider)
    {
        PayloadDirectory = payloadDirectory;
        PowerShellFileVersion = powerShellFileVersion;
        BindingsAbiVersion = bindingsAbiVersion;
        PayloadTableSize = payloadTableSize;
        PayloadTableSlotCount = payloadTableSlotCount;
        PayloadTableShape = payloadTableShape;
        FeatureFlags = featureFlags;
        RegisteredLiveObjectContractPacks = registeredLiveObjectContractPacks;
        ManagedWsManProvider = managedWsManProvider;
    }

    /// <summary>
    /// The runtime's canonicalized active payload directory, extended-length
    /// (<c>\\?\</c>) prefixed on Windows and runtime-resolved for <c>PATH</c>-based
    /// activation. This is a local filesystem path; redact it before external telemetry.
    /// </summary>
    public string PayloadDirectory { get; }

    public string? PowerShellFileVersion { get; }

    public uint BindingsAbiVersion { get; }

    public nuint PayloadTableSize { get; }

    public uint PayloadTableSlotCount { get; }

    public PowerShellPayloadTableShape PayloadTableShape { get; }

    public ulong FeatureFlags { get; }

    public IReadOnlyList<PowerShellLiveObjectContractPackIdentity> RegisteredLiveObjectContractPacks { get; }

    public PowerShellRemoteProviderDiagnostic ManagedWsManProvider { get; }

    internal static unsafe PowerShellRuntimeDiagnosticReport Create(
        string payloadDirectory,
        ulong featureFlags)
    {
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeRuntimeDiagnosticsInfo info = new()
        {
            Size = checked((uint)sizeof(NativeRuntimeDiagnosticsInfo)),
        };
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        int status = NativeMethods.GetRuntimeDiagnosticsInfo(&info, &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);

        if (info.BindingsAbiVersion != 1 ||
            info.PayloadTableSize == 0 ||
            info.PayloadTableSlotCount == 0 ||
            info.PayloadTableShape != (uint)PowerShellPayloadTableShape.V1 ||
            info.PowerShellFileVersionAvailable > 1 ||
            info.ContractPackCount > 16 ||
            info.Reserved != 0 ||
            !Enum.IsDefined((PowerShellRemoteProviderStatus)info.RemoteProviderStatus) ||
            !Enum.IsDefined((PowerShellRemoteProviderUnavailableReason)info.RemoteProviderUnavailableReason) ||
            info.Reserved2 != 0 ||
            (info.RemoteProviderCapabilities &
             ~(ulong)(PowerShellRemoteProviderCapability.CustomClientTransport |
                       PowerShellRemoteProviderCapability.RemoteRunspacePool |
                       PowerShellRemoteProviderCapability.HostStreamRelay |
                       PowerShellRemoteProviderCapability.NegotiateAuthentication |
                       PowerShellRemoteProviderCapability.KerberosAuthentication |
                       PowerShellRemoteProviderCapability.NtlmAuthentication |
                       PowerShellRemoteProviderCapability.KdcProxy)) != 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid runtime diagnostic metadata.");
        }

        string? fileVersion = info.PowerShellFileVersionAvailable == 0
            ? null
            : CopyUtf8(
                NativeMethods.CopyRuntimeDiagnosticsPowerShellFileVersion,
                128,
                diagnostic,
                "PowerShell file version");
        var contractPacks = new PowerShellLiveObjectContractPackIdentity[checked((int)info.ContractPackCount)];
        for (uint index = 0; index < info.ContractPackCount; index++)
        {
            uint contractPackIndex = index;
            string identity = CopyUtf8(
                (byte* buffer, nuint bufferLength, nuint* requiredLength, NativeCallResult* callResult) =>
                    NativeMethods.CopyRuntimeDiagnosticsContractPackIdentity(
                        contractPackIndex,
                        buffer,
                        bufferLength,
                        requiredLength,
                        callResult),
                512,
                diagnostic,
                "live-object contract-pack identity");
            if (string.IsNullOrWhiteSpace(identity) || identity.IndexOf('\0') >= 0)
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned an invalid live-object contract-pack identity.");
            }

            contractPacks[checked((int)index)] = new PowerShellLiveObjectContractPackIdentity(identity);
        }

        var providerStatus = (PowerShellRemoteProviderStatus)info.RemoteProviderStatus;
        var unavailableReason =
            (PowerShellRemoteProviderUnavailableReason)info.RemoteProviderUnavailableReason;
        var providerCapabilities =
            (PowerShellRemoteProviderCapability)info.RemoteProviderCapabilities;
        bool providerIsConsistent = providerStatus switch
        {
            PowerShellRemoteProviderStatus.Available =>
                unavailableReason == PowerShellRemoteProviderUnavailableReason.None &&
                providerCapabilities.HasFlag(PowerShellRemoteProviderCapability.CustomClientTransport) &&
                providerCapabilities.HasFlag(PowerShellRemoteProviderCapability.RemoteRunspacePool),
            PowerShellRemoteProviderStatus.Unavailable =>
                unavailableReason != PowerShellRemoteProviderUnavailableReason.None &&
                providerCapabilities == PowerShellRemoteProviderCapability.None,
            _ => false,
        };
        if (!providerIsConsistent)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned inconsistent remote provider diagnostic metadata.");
        }

        var managedWsManProvider = new PowerShellRemoteProviderDiagnostic(
            PowerShellRemoteProviderDiagnostic.DevolutionsManagedWsManProviderId,
            providerStatus,
            unavailableReason,
            providerCapabilities);

        return new PowerShellRuntimeDiagnosticReport(
            payloadDirectory,
            fileVersion,
            info.BindingsAbiVersion,
            info.PayloadTableSize,
            info.PayloadTableSlotCount,
            (PowerShellPayloadTableShape)info.PayloadTableShape,
            featureFlags,
            Array.AsReadOnly(contractPacks),
            managedWsManProvider);
    }

    private unsafe delegate int CopyUtf8Delegate(
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    private static unsafe string CopyUtf8(
        CopyUtf8Delegate copy,
        nuint maximumLength,
        byte* diagnostic,
        string description)
    {
        nuint requiredLength = 0;
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        int status = copy(null, 0, &requiredLength, &result);
        if (status != (int)PowerShellFfiStatus.Success &&
            status != (int)PowerShellFfiStatus.BufferTooSmall)
        {
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        if (requiredLength == 0 || requiredLength > maximumLength)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                $"Native PowerShell FFI returned an invalid {description} length.");
        }

        byte[] bytes = new byte[checked((int)requiredLength)];
        fixed (byte* buffer = bytes)
        {
            result = NativeCall.CreateResult(diagnostic);
            status = copy(buffer, (nuint)bytes.Length, &requiredLength, &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        if (requiredLength != (nuint)bytes.Length)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                $"Native PowerShell FFI changed the {description} during copy.");
        }

        try
        {
            return new UTF8Encoding(false, true).GetString(bytes);
        }
        catch (DecoderFallbackException exception)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                $"Native PowerShell FFI returned invalid UTF-8 {description}: {exception.Message}");
        }
    }
}
