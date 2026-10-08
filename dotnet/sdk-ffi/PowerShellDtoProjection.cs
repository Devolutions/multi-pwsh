using System.Collections.ObjectModel;

namespace Devolutions.PowerShell.Ffi;

public enum PowerShellDtoProjectionFailure
{
    InvalidRoot,
    UnknownMember,
    MissingMember,
    InvalidVersion,
    InvalidValue,
    ValueTooLarge,
}

public sealed class PowerShellDtoProjectionException : InvalidOperationException
{
    internal PowerShellDtoProjectionException(PowerShellDtoProjectionError error)
        : base(error.Message)
    {
        Error = error;
    }

    public PowerShellDtoProjectionError Error { get; }
}

public sealed class PowerShellDtoProjectionError
{
    internal PowerShellDtoProjectionError(
        PowerShellDtoProjectionFailure failure,
        string path,
        string message)
    {
        Failure = failure;
        Path = path;
        Message = message;
    }

    public PowerShellDtoProjectionFailure Failure { get; }

    public string Path { get; }

    public string Message { get; }
}

/// <summary>
/// Shared bounded primitives used by source-generated DTO projections.
/// </summary>
public static class PowerShellDtoProjection
{
    public const string VersionMemberName = "$version";

    public static PowerShellDtoProjectionException CreateException(PowerShellDtoProjectionError error)
    {
        ArgumentNullException.ThrowIfNull(error);
        return new PowerShellDtoProjectionException(error);
    }

    public static bool TryGetPropertyBag(
        PowerShellValue value,
        int version,
        bool rejectUnknownMembers,
        IReadOnlySet<string> declaredMembers,
        string path,
        out IReadOnlyDictionary<string, PowerShellValue>? properties,
        out PowerShellDtoProjectionError? error)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(declaredMembers);
        if (version < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(version));
        }

        if (value.Kind != PowerShellValueKind.PropertyBag)
        {
            properties = null;
            error = Failure(PowerShellDtoProjectionFailure.InvalidRoot, path, "Expected a copied property bag.");
            return false;
        }

        properties = value.GetPropertyBag();
        if (!properties.TryGetValue(VersionMemberName, out PowerShellValue? versionValue) ||
            !versionValue.TryGetUnsignedInteger(out ulong encodedVersion) ||
            encodedVersion != (ulong)version)
        {
            properties = null;
            error = Failure(PowerShellDtoProjectionFailure.InvalidVersion, path, "The DTO version is missing or incompatible.");
            return false;
        }

        if (rejectUnknownMembers &&
            properties.Keys.Any(name => !string.Equals(name, VersionMemberName, StringComparison.Ordinal) &&
                                        !declaredMembers.Contains(name)))
        {
            properties = null;
            error = Failure(PowerShellDtoProjectionFailure.UnknownMember, path, "The DTO contains an undeclared member.");
            return false;
        }

        error = null;
        return true;
    }

    public static bool TryGetMember(
        IReadOnlyDictionary<string, PowerShellValue> properties,
        string name,
        bool required,
        string path,
        out PowerShellValue? value,
        out PowerShellDtoProjectionError? error)
    {
        ArgumentNullException.ThrowIfNull(properties);
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        if (properties.TryGetValue(name, out value))
        {
            error = null;
            return true;
        }

        error = required
            ? Failure(PowerShellDtoProjectionFailure.MissingMember, JoinPath(path, name), "The required DTO member is missing.")
            : null;
        return !required;
    }

    public static bool TryReadString(
        PowerShellValue value,
        int maximumLength,
        string path,
        out string? result,
        out PowerShellDtoProjectionError? error)
    {
        if (maximumLength < 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumLength));
        }

        if (!value.TryGetString(out result) || result is null || result.Length > maximumLength)
        {
            result = null;
            error = Failure(
                value.Kind == PowerShellValueKind.String
                    ? PowerShellDtoProjectionFailure.ValueTooLarge
                    : PowerShellDtoProjectionFailure.InvalidValue,
                path,
                "The DTO string member has an invalid kind or exceeds its bound.");
            return false;
        }

        error = null;
        return true;
    }

    public static bool TryReadArray(
        PowerShellValue value,
        int maximumCount,
        string path,
        out IReadOnlyList<PowerShellValue>? values,
        out PowerShellDtoProjectionError? error)
    {
        if (maximumCount < 0 || maximumCount > PowerShellValue.MaximumContainerEntries)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumCount));
        }

        if (value.Kind != PowerShellValueKind.Array)
        {
            values = null;
            error = Failure(PowerShellDtoProjectionFailure.InvalidValue, path, "Expected a copied tagged array.");
            return false;
        }

        values = value.GetArray();
        if (values.Count > maximumCount)
        {
            values = null;
            error = Failure(PowerShellDtoProjectionFailure.ValueTooLarge, path, "The DTO array exceeds its declared bound.");
            return false;
        }

        error = null;
        return true;
    }

    public static PowerShellValue CreatePropertyBag(
        int version,
        IEnumerable<KeyValuePair<string, PowerShellValue>> members)
    {
        ArgumentNullException.ThrowIfNull(members);
        if (version < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(version));
        }

        return PowerShellValue.PropertyBag(
            new[] { new KeyValuePair<string, PowerShellValue>(VersionMemberName, PowerShellValue.UnsignedInteger((ulong)version)) }
                .Concat(members));
    }

    public static string JoinPath(string path, string member)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(member);
        return string.IsNullOrEmpty(path) ? member : string.Concat(path, ".", member);
    }

    public static PowerShellDtoProjectionError PrefixPath(PowerShellDtoProjectionError error, string path)
    {
        ArgumentNullException.ThrowIfNull(error);
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        return Failure(
            error.Failure,
            string.IsNullOrEmpty(error.Path) ? path : JoinPath(path, error.Path),
            error.Message);
    }

    public static PowerShellDtoProjectionError InvalidValue(string path, string message) =>
        Failure(PowerShellDtoProjectionFailure.InvalidValue, path, message);

    public static PowerShellDtoProjectionError ValueTooLarge(string path, string message) =>
        Failure(PowerShellDtoProjectionFailure.ValueTooLarge, path, message);

    private static PowerShellDtoProjectionError Failure(
        PowerShellDtoProjectionFailure failure,
        string path,
        string message) =>
        new(failure, path, message);
}
