namespace Devolutions.PowerShell.Ffi;

public enum PowerShellRemoteProviderStatus : uint
{
    Unavailable = 0,
    Available = 1,
}

public enum PowerShellRemoteProviderUnavailableReason : uint
{
    None = 0,
    PayloadHooksMissing = 1,
    TransportImplementationMissing = 2,
    RegistrationFailed = 3,
    UnsupportedPlatform = 4,
}

[Flags]
public enum PowerShellRemoteProviderCapability : ulong
{
    None = 0,
    CustomClientTransport = 1UL << 0,
    RemoteRunspacePool = 1UL << 1,
    HostStreamRelay = 1UL << 2,
    NegotiateAuthentication = 1UL << 3,
    KerberosAuthentication = 1UL << 4,
    NtlmAuthentication = 1UL << 5,
    KdcProxy = 1UL << 6,
}

/// <summary>
/// Copied, non-secret capability information for a MultiPwsh-managed remoting provider.
/// </summary>
public sealed class PowerShellRemoteProviderDiagnostic
{
    public const string DevolutionsManagedWsManProviderId = "devolutions.wsman";

    internal PowerShellRemoteProviderDiagnostic(
        string providerId,
        PowerShellRemoteProviderStatus status,
        PowerShellRemoteProviderUnavailableReason unavailableReason,
        PowerShellRemoteProviderCapability capabilities)
    {
        ProviderId = providerId;
        Status = status;
        UnavailableReason = unavailableReason;
        Capabilities = capabilities;
    }

    public string ProviderId { get; }

    public PowerShellRemoteProviderStatus Status { get; }

    public PowerShellRemoteProviderUnavailableReason UnavailableReason { get; }

    public PowerShellRemoteProviderCapability Capabilities { get; }

    public bool IsAvailable => Status == PowerShellRemoteProviderStatus.Available;
}
