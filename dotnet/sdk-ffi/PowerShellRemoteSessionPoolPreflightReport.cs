namespace Devolutions.PowerShell.Ffi;

public enum PowerShellRemoteSessionPoolPreflightFailure
{
    None = 0,
    ProviderUnavailable = 1,
    MissingCapability = 2,
}

/// <summary>
/// A non-executing validation result for a remote runspace-pool configuration.
/// </summary>
public sealed class PowerShellRemoteSessionPoolPreflightReport
{
    internal PowerShellRemoteSessionPoolPreflightReport(
        PowerShellRemoteProviderDiagnostic provider,
        PowerShellRemoteSessionPoolPreflightFailure failure,
        string message)
    {
        Provider = provider;
        Failure = failure;
        Message = message;
    }

    public PowerShellRemoteProviderDiagnostic Provider { get; }

    public PowerShellRemoteSessionPoolPreflightFailure Failure { get; }

    public string Message { get; }

    public bool IsValid => Failure == PowerShellRemoteSessionPoolPreflightFailure.None;
}
