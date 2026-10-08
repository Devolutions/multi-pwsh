namespace Devolutions.PowerShell.Ffi;

public enum PowerShellRemoteSspiProvider
{
    Default = 0,
    Platform = 1,
    Rust = 2,
}

public enum PowerShellRemoteAuthentication
{
    Negotiate = 0,
    Kerberos = 1,
    Ntlm = 2,
}

public sealed class PowerShellRemoteStreamConnectContext
{
    internal PowerShellRemoteStreamConnectContext(string host, int port, bool useSsl)
    {
        Host = host;
        Port = port;
        UseSsl = useSsl;
    }

    public string Host { get; }

    public int Port { get; }

    public bool UseSsl { get; }
}

/// <summary>
/// Creates host-owned streams for a MultiPwsh-managed remote transport relay.
/// Returned streams never cross the FFI boundary.
/// </summary>
public interface IPowerShellRemoteStreamConnector
{
    ValueTask<Stream> ConnectAsync(
        PowerShellRemoteStreamConnectContext context,
        CancellationToken cancellationToken);
}

public sealed class DevolutionsManagedWsManConnectionOptions
{
    public DevolutionsManagedWsManConnectionOptions(
        Uri endpointUri,
        PowerShellCredential? credential = null,
        PowerShellRemoteAuthentication authentication = PowerShellRemoteAuthentication.Negotiate,
        string shellUri = "http://schemas.microsoft.com/powershell/Microsoft.PowerShell",
        int maximumEnvelopeSizeKilobytes = 512,
        int maximumConnections = 20,
        bool skipServerCertificateCheck = false,
        PowerShellRemoteSspiProvider sspiProvider = PowerShellRemoteSspiProvider.Default,
        string? domain = null,
        string? kdcProxyName = null,
        string? spn = null,
        TimeSpan? openTimeout = null,
        TimeSpan? operationTimeout = null,
        TimeSpan? cancelTimeout = null,
        IPowerShellRemoteStreamConnector? connector = null)
    {
        ArgumentNullException.ThrowIfNull(endpointUri);
        ArgumentException.ThrowIfNullOrWhiteSpace(shellUri);
        if (!endpointUri.IsAbsoluteUri ||
            endpointUri.Scheme is not ("http" or "https") ||
            string.IsNullOrWhiteSpace(endpointUri.Host) ||
            endpointUri.UserInfo.Length != 0 ||
            endpointUri.Fragment.Length != 0 ||
            endpointUri.AbsoluteUri.Length > 2_048 ||
            ContainsControlCharacter(endpointUri.AbsoluteUri))
        {
            throw new ArgumentException(
                "The WSMan endpoint must be an absolute HTTP or HTTPS URI without user information or a fragment.",
                nameof(endpointUri));
        }
        if (shellUri.Length > 1_024 ||
            ContainsControlCharacter(shellUri) ||
            !Uri.TryCreate(shellUri, UriKind.Absolute, out _))
        {
            throw new ArgumentException(
                "The WSMan shell URI must be an absolute non-NUL URI of at most 1,024 characters.",
                nameof(shellUri));
        }
        if (maximumEnvelopeSizeKilobytes is < 1 or > 16 * 1_024)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumEnvelopeSizeKilobytes));
        }
        if (maximumConnections is < 1 or > 64)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumConnections));
        }
        if (!Enum.IsDefined(sspiProvider))
        {
            throw new ArgumentOutOfRangeException(nameof(sspiProvider));
        }
        if (!Enum.IsDefined(authentication))
        {
            throw new ArgumentOutOfRangeException(nameof(authentication));
        }

        ValidateOptionalText(domain, 256, nameof(domain));
        ValidateOptionalText(kdcProxyName, 2_048, nameof(kdcProxyName));
        ValidateOptionalText(spn, 1_024, nameof(spn));
        TimeSpan resolvedOpenTimeout = ValidateTimeout(
            openTimeout ?? TimeSpan.FromMinutes(3),
            allowZero: false,
            nameof(openTimeout));
        TimeSpan resolvedOperationTimeout = ValidateTimeout(
            operationTimeout ?? TimeSpan.FromMinutes(3),
            allowZero: false,
            nameof(operationTimeout));
        TimeSpan resolvedCancelTimeout = ValidateTimeout(
            cancelTimeout ?? TimeSpan.FromMinutes(1),
            allowZero: true,
            nameof(cancelTimeout));

        EndpointUri = endpointUri;
        Credential = credential;
        Authentication = authentication;
        ShellUri = shellUri;
        MaximumEnvelopeSizeKilobytes = maximumEnvelopeSizeKilobytes;
        MaximumConnections = maximumConnections;
        SkipServerCertificateCheck = skipServerCertificateCheck;
        SspiProvider = sspiProvider;
        Domain = domain;
        KdcProxyName = kdcProxyName;
        Spn = spn;
        OpenTimeout = resolvedOpenTimeout;
        OperationTimeout = resolvedOperationTimeout;
        CancelTimeout = resolvedCancelTimeout;
        Connector = connector;
    }

    public Uri EndpointUri { get; }

    public string ComputerName => EndpointUri.Host;

    public PowerShellCredential? Credential { get; }

    public PowerShellRemoteAuthentication Authentication { get; }

    public bool UseSsl => EndpointUri.Scheme == Uri.UriSchemeHttps;

    public int Port => EndpointUri.Port;

    public string AppName => EndpointUri.AbsolutePath;

    public string ShellUri { get; }

    public int MaximumEnvelopeSizeKilobytes { get; }

    public int MaximumConnections { get; }

    public bool SkipServerCertificateCheck { get; }

    public PowerShellRemoteSspiProvider SspiProvider { get; }

    public string? Domain { get; }

    public string? KdcProxyName { get; }

    public string? Spn { get; }

    public TimeSpan OpenTimeout { get; }

    public TimeSpan OperationTimeout { get; }

    public TimeSpan CancelTimeout { get; }

    public IPowerShellRemoteStreamConnector? Connector { get; }

    private static void ValidateOptionalText(string? value, int maximumLength, string parameterName)
    {
        if (value is not null &&
            (string.IsNullOrWhiteSpace(value) || value.Length > maximumLength || ContainsControlCharacter(value)))
        {
            throw new ArgumentException(
                $"The value must be non-empty text without control characters and at most {maximumLength} characters.",
                parameterName);
        }
    }

    private static bool ContainsControlCharacter(string value)
    {
        foreach (char character in value)
        {
            if (char.IsControl(character))
            {
                return true;
            }
        }

        return false;
    }

    private static TimeSpan ValidateTimeout(TimeSpan value, bool allowZero, string parameterName)
    {
        double milliseconds = value.TotalMilliseconds;
        if ((!allowZero && milliseconds < 1) ||
            (allowZero && milliseconds < 0) ||
            milliseconds > int.MaxValue ||
            milliseconds != Math.Truncate(milliseconds))
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                "Timeouts must use whole milliseconds within the supported 32-bit range.");
        }

        return value;
    }
}

public sealed class PowerShellRemoteSessionPoolOptions
{
    public PowerShellRemoteSessionPoolOptions(
        DevolutionsManagedWsManConnectionOptions connection,
        uint minimumRunspaces = 1,
        uint maximumRunspaces = 1)
    {
        ArgumentNullException.ThrowIfNull(connection);
        if (minimumRunspaces < 1 || maximumRunspaces < minimumRunspaces || maximumRunspaces > 64)
        {
            throw new ArgumentOutOfRangeException(
                nameof(maximumRunspaces),
                "Remote runspace pools require 1 to 64 runspaces and a maximum not less than the minimum.");
        }

        Connection = connection;
        MinimumRunspaces = minimumRunspaces;
        MaximumRunspaces = maximumRunspaces;
    }

    public DevolutionsManagedWsManConnectionOptions Connection { get; }

    public uint MinimumRunspaces { get; }

    public uint MaximumRunspaces { get; }
}
