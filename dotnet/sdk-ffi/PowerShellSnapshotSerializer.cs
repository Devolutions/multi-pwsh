using System.Text.Json;
using System.Text.Json.Serialization;

namespace Devolutions.PowerShell.Ffi;

/// <summary>
/// Serializes immutable invocation snapshots to a bounded, versioned JSON document for
/// storage or display. This format never reconstructs PowerShell, SMA, or arbitrary CLR objects.
/// </summary>
public static class PowerShellSnapshotSerializer
{
    public const int FormatVersion = 2;
    public const int MaxDocumentBytes = 1024 * 1024;

    /// <summary>Serializes a snapshot to deterministic UTF-8 JSON for storage or display only.</summary>
    public static byte[] Serialize(PowerShellInvocationResult result)
    {
        ArgumentNullException.ThrowIfNull(result);
        byte[] document = JsonSerializer.SerializeToUtf8Bytes(ToDto(result), SnapshotJsonContext.Default.SnapshotDocumentDto);
        if (document.Length > MaxDocumentBytes)
        {
            throw new ArgumentOutOfRangeException(nameof(result), "The serialized snapshot exceeds the 1 MiB format limit.");
        }

        return document;
    }

    /// <summary>Restores a copied snapshot from this serializer's storage/display-only JSON format.</summary>
    public static PowerShellInvocationResult Deserialize(ReadOnlySpan<byte> document)
    {
        if (document.Length == 0 || document.Length > MaxDocumentBytes)
        {
            throw new ArgumentOutOfRangeException(nameof(document), "The snapshot document must be between 1 byte and 1 MiB.");
        }

        SnapshotDocumentDto? dto;
        try
        {
            using JsonDocument parsed = JsonDocument.Parse(
                document.ToArray(),
                new JsonDocumentOptions
                {
                    AllowTrailingCommas = false,
                    CommentHandling = JsonCommentHandling.Disallow,
                    MaxDepth = 16,
                });
            dto = JsonSerializer.Deserialize(document, SnapshotJsonContext.Default.SnapshotDocumentDto);
        }
        catch (JsonException exception)
        {
            throw new ArgumentException("The snapshot document is not valid versioned snapshot JSON.", nameof(document), exception);
        }

        if (dto is null || dto.Version is not (1 or FormatVersion) || dto.Result is null)
        {
            throw new ArgumentException("The snapshot document has an unsupported version or missing result.", nameof(document));
        }

        return FromDto(dto.Result, dto.Version);
    }

    private static SnapshotDocumentDto ToDto(PowerShellInvocationResult result)
    {
        return new SnapshotDocumentDto
        {
            Version = FormatVersion,
            Result = new InvocationResultDto
            {
                State = checked((uint)result.State),
                InvocationId = result.InvocationId,
                HadErrors = result.HadErrors,
                IsTerminatingFailure = result.IsTerminatingFailure,
                IsSequenceTruncated = result.IsSequenceTruncated,
                Output = ToDto(result.Output, ToDto),
                Errors = ToDto(result.Errors, ToDto),
                Warnings = ToDto(result.Warnings),
                Verbose = ToDto(result.Verbose),
                Debug = ToDto(result.Debug),
                Information = ToDto(result.Information),
                Progress = ToDto(result.Progress),
                Sequence = result.Sequence.Select(item => new SequenceDto
                {
                    Stream = checked((uint)item.Stream),
                    Index = item.Index,
                    Sequence = item.Sequence,
                }).ToList(),
            },
        };
    }

    private static ObjectStreamDto ToDto(
        PowerShellStreamSnapshot<PowerShellObjectSnapshot> stream,
        Func<PowerShellObjectSnapshot, ObjectDto> convert)
    {
        return new ObjectStreamDto
        {
            IsTruncated = stream.IsTruncated,
            TotalRecordCount = stream.TotalRecordCount,
            DroppedRecordCount = stream.DroppedRecordCount,
            Records = stream.Records.Select(convert).ToList(),
        };
    }

    private static ErrorStreamDto ToDto(
        PowerShellStreamSnapshot<PowerShellInvocationError> stream,
        Func<PowerShellInvocationError, ErrorDto> convert)
    {
        return new ErrorStreamDto
        {
            IsTruncated = stream.IsTruncated,
            TotalRecordCount = stream.TotalRecordCount,
            DroppedRecordCount = stream.DroppedRecordCount,
            Records = stream.Records.Select(convert).ToList(),
        };
    }

    private static TextStreamDto ToDto(PowerShellStreamSnapshot<PowerShellStreamRecord> stream)
    {
        return new TextStreamDto
        {
            IsTruncated = stream.IsTruncated,
            TotalRecordCount = stream.TotalRecordCount,
            DroppedRecordCount = stream.DroppedRecordCount,
            Records = stream.Records.Select(item => new TextDto
            {
                DisplayText = item.DisplayText,
                Sequence = item.Sequence,
                IsTruncated = item.IsTruncated,
            }).ToList(),
        };
    }

    private static ObjectDto ToDto(PowerShellObjectSnapshot item)
    {
        return new ObjectDto
        {
            DisplayText = item.DisplayText,
            TypeNames = item.TypeNames.ToList(),
            Sequence = item.Sequence,
            IsTruncated = item.IsTruncated,
            ScalarValue = ToDto(item.ScalarValue),
            PropertyBag = ToDto(item.PropertyBag),
            PropertyEntryCount = item.PropertyEntryCount,
            DroppedPropertyEntryCount = item.DroppedPropertyEntryCount,
            TypeNameCount = item.TypeNameCount,
            DroppedTypeNameCount = item.DroppedTypeNameCount,
            IsPropertyBagTruncated = item.IsPropertyBagTruncated,
        };
    }

    private static ErrorDto ToDto(PowerShellInvocationError item)
    {
        return new ErrorDto
        {
            Message = item.Message,
            FullyQualifiedErrorId = item.FullyQualifiedErrorId,
            Category = item.Category,
            ExceptionType = item.ExceptionType,
            ExceptionHResult = item.ExceptionHResult,
            InvocationName = item.InvocationName,
            PositionMessage = item.PositionMessage,
            ScriptStackTrace = item.ScriptStackTrace,
            CategoryReason = item.CategoryReason,
            CategoryActivity = item.CategoryActivity,
            CategoryTargetName = item.CategoryTargetName,
            CategoryTargetType = item.CategoryTargetType,
            CommandName = item.CommandName,
            InvocationLine = item.InvocationLine,
            OffsetInLine = item.OffsetInLine,
            PipelineLength = item.PipelineLength,
            PipelinePosition = item.PipelinePosition,
            ErrorDetailsMessage = item.ErrorDetailsMessage,
            RecommendedAction = item.RecommendedAction,
            TargetDisplayText = item.TargetDisplayText,
            TargetValue = ToDto(item.TargetValue),
            Sequence = item.Sequence,
            IsTruncated = item.IsTruncated,
        };
    }

    private static ValueDto? ToDto(PowerShellValue? value)
    {
        return value is null ? null : new ValueDto
        {
            Kind = checked((uint)value.Kind),
            Payload = Convert.ToBase64String(value.Payload),
        };
    }

    private static PowerShellInvocationResult FromDto(InvocationResultDto result, int formatVersion)
    {
        if (!Enum.IsDefined((PowerShellInvocationState)result.State)
            || result.Output is null
            || result.Errors is null
            || result.Warnings is null
            || result.Verbose is null
            || result.Debug is null
            || result.Information is null
            || result.Progress is null
            || result.Sequence is null)
        {
            throw InvalidDocument();
        }

        IReadOnlyList<PowerShellStreamSequenceRecord> sequence = result.Sequence.Select(item =>
        {
            if (!Enum.IsDefined((PowerShellStreamKind)item.Stream)) throw InvalidDocument();
            return new PowerShellStreamSequenceRecord((PowerShellStreamKind)item.Stream, item.Index, item.Sequence);
        }).ToList().AsReadOnly();
        if (sequence.Count > 224) throw InvalidDocument();

        return new PowerShellInvocationResult(
            FromObjectStream(result.Output, formatVersion),
            FromErrorStream(result.Errors, formatVersion),
            FromTextStream(result.Warnings, PowerShellStreamKind.Warning),
            FromTextStream(result.Verbose, PowerShellStreamKind.Verbose),
            FromTextStream(result.Debug, PowerShellStreamKind.Debug),
            FromTextStream(result.Information, PowerShellStreamKind.Information),
            FromTextStream(result.Progress, PowerShellStreamKind.Progress),
            sequence,
            (PowerShellInvocationState)result.State,
            result.InvocationId,
            result.HadErrors,
            result.IsTerminatingFailure,
            result.IsSequenceTruncated);
    }

    private static PowerShellStreamSnapshot<PowerShellObjectSnapshot> FromObjectStream(ObjectStreamDto stream, int formatVersion)
    {
        List<ObjectDto> records = RequireRecords(stream.Records, stream.TotalRecordCount, stream.DroppedRecordCount);
        return new PowerShellStreamSnapshot<PowerShellObjectSnapshot>(
            PowerShellStreamKind.Output,
            records.Select(item =>
            {
                string displayText = Require(item.DisplayText);
                List<string> typeNames = item.TypeNames ?? throw InvalidDocument();
                if (typeNames.Count > 8 || item.TypeNameCount < typeNames.Count) throw InvalidDocument();
                PowerShellValue? scalarValue = FromDto(item.ScalarValue, formatVersion);
                PowerShellValue? propertyBag = FromDto(item.PropertyBag, formatVersion);
                if ((scalarValue is not null && !scalarValue.IsSnapshotScalar)
                    || (propertyBag is not null && !propertyBag.IsSnapshotPropertyBag)
                    || item.PropertyEntryCount > 16
                    || (propertyBag is null && item.PropertyEntryCount != 0))
                {
                    throw InvalidDocument();
                }
                return new PowerShellObjectSnapshot(
                    displayText,
                    typeNames.Select(Require).ToArray(),
                    item.Sequence,
                    item.IsTruncated,
                    scalarValue,
                    propertyBag,
                    item.PropertyEntryCount,
                    item.DroppedPropertyEntryCount,
                    item.TypeNameCount,
                    item.DroppedTypeNameCount,
                    item.IsPropertyBagTruncated);
            }).ToArray(),
            stream.IsTruncated,
            stream.TotalRecordCount,
            stream.DroppedRecordCount);
    }

    private static PowerShellStreamSnapshot<PowerShellInvocationError> FromErrorStream(ErrorStreamDto stream, int formatVersion)
    {
        List<ErrorDto> records = RequireRecords(stream.Records, stream.TotalRecordCount, stream.DroppedRecordCount);
        return new PowerShellStreamSnapshot<PowerShellInvocationError>(
            PowerShellStreamKind.Error,
            records.Select(item =>
            {
                if ((formatVersion == FormatVersion && item.ExceptionHResult is null) ||
                    (formatVersion == 1 && item.HasExceptionHResult))
                {
                    throw InvalidDocument();
                }
                PowerShellValue? targetValue = FromDto(item.TargetValue, formatVersion);
                if (targetValue is not null && !targetValue.IsSnapshotScalar) throw InvalidDocument();
                return new PowerShellInvocationError(
                    Require(item.Message),
                    Require(item.FullyQualifiedErrorId),
                    Require(item.Category),
                    Require(item.ExceptionType),
                    item.ExceptionHResult ?? 0,
                    Require(item.InvocationName),
                    Require(item.PositionMessage),
                    Require(item.ScriptStackTrace),
                    Require(item.CategoryReason),
                    Require(item.CategoryActivity),
                    Require(item.CategoryTargetName),
                    Require(item.CategoryTargetType),
                    Require(item.CommandName),
                    Require(item.InvocationLine),
                    Require(item.OffsetInLine),
                    Require(item.PipelineLength),
                    Require(item.PipelinePosition),
                    Require(item.ErrorDetailsMessage),
                    Require(item.RecommendedAction),
                    Require(item.TargetDisplayText),
                    targetValue,
                    item.Sequence,
                    item.IsTruncated);
            }).ToArray(),
            stream.IsTruncated,
            stream.TotalRecordCount,
            stream.DroppedRecordCount);
    }

    private static PowerShellStreamSnapshot<PowerShellStreamRecord> FromTextStream(TextStreamDto stream, PowerShellStreamKind kind)
    {
        List<TextDto> records = RequireRecords(stream.Records, stream.TotalRecordCount, stream.DroppedRecordCount);
        return new PowerShellStreamSnapshot<PowerShellStreamRecord>(
            kind,
            records.Select(item => new PowerShellStreamRecord(Require(item.DisplayText), item.Sequence, item.IsTruncated)).ToArray(),
            stream.IsTruncated,
            stream.TotalRecordCount,
            stream.DroppedRecordCount);
    }

    private static PowerShellValue? FromDto(ValueDto? value, int formatVersion)
    {
        if (value is null) return null;
        try
        {
            byte[] payload = Convert.FromBase64String(Require(value.Payload));
            if (payload.Length > 16 * 1024) throw InvalidDocument();
            PowerShellValue copied = PowerShellValue.FromNative(value.Kind, payload);
            if (formatVersion == 1 &&
                (copied.Kind == PowerShellValueKind.TimeSpan ||
                 (copied.Kind == PowerShellValueKind.PropertyBag &&
                  copied.GetPropertyBag().Values.Any(item => item.Kind == PowerShellValueKind.TimeSpan))))
            {
                throw InvalidDocument();
            }
            return copied;
        }
        catch (FormatException)
        {
            throw InvalidDocument();
        }
        catch (PowerShellFfiException)
        {
            throw InvalidDocument();
        }
    }

    private static List<T> RequireRecords<T>(List<T>? records, ulong totalRecordCount, ulong droppedRecordCount)
    {
        if (records is null
            || records.Count > 32
            || totalRecordCount < (ulong)records.Count
            || droppedRecordCount > totalRecordCount)
        {
            throw InvalidDocument();
        }

        return records;
    }

    private static string Require(string? value)
    {
        if (value is null || value.Length > 4 * 1024 || value.IndexOf('\0') >= 0) throw InvalidDocument();
        return value;
    }

    private static ArgumentException InvalidDocument()
    {
        return new ArgumentException("The snapshot document contains invalid bounded snapshot data.");
    }
}

[JsonSourceGenerationOptions(
    PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
    UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
    WriteIndented = false)]
[JsonSerializable(typeof(SnapshotDocumentDto))]
internal sealed partial class SnapshotJsonContext : JsonSerializerContext
{
}

internal sealed class SnapshotDocumentDto
{
    [JsonRequired, JsonPropertyOrder(0)]
    public int Version { get; set; }

    [JsonRequired, JsonPropertyOrder(1)]
    public InvocationResultDto? Result { get; set; }
}

internal sealed class InvocationResultDto
{
    [JsonRequired, JsonPropertyOrder(0)] public uint State { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public ulong InvocationId { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public bool HadErrors { get; set; }
    [JsonRequired, JsonPropertyOrder(3)] public bool IsTerminatingFailure { get; set; }
    [JsonRequired, JsonPropertyOrder(4)] public bool IsSequenceTruncated { get; set; }
    [JsonRequired, JsonPropertyOrder(5)] public ObjectStreamDto? Output { get; set; }
    [JsonRequired, JsonPropertyOrder(6)] public ErrorStreamDto? Errors { get; set; }
    [JsonRequired, JsonPropertyOrder(7)] public TextStreamDto? Warnings { get; set; }
    [JsonRequired, JsonPropertyOrder(8)] public TextStreamDto? Verbose { get; set; }
    [JsonRequired, JsonPropertyOrder(9)] public TextStreamDto? Debug { get; set; }
    [JsonRequired, JsonPropertyOrder(10)] public TextStreamDto? Information { get; set; }
    [JsonRequired, JsonPropertyOrder(11)] public TextStreamDto? Progress { get; set; }
    [JsonRequired, JsonPropertyOrder(12)] public List<SequenceDto>? Sequence { get; set; }
}

internal abstract class StreamDto
{
    [JsonRequired, JsonPropertyOrder(0)] public bool IsTruncated { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public ulong TotalRecordCount { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public ulong DroppedRecordCount { get; set; }
}

internal sealed class ObjectStreamDto : StreamDto
{
    [JsonRequired, JsonPropertyOrder(3)] public List<ObjectDto>? Records { get; set; }
}

internal sealed class ErrorStreamDto : StreamDto
{
    [JsonRequired, JsonPropertyOrder(3)] public List<ErrorDto>? Records { get; set; }
}

internal sealed class TextStreamDto : StreamDto
{
    [JsonRequired, JsonPropertyOrder(3)] public List<TextDto>? Records { get; set; }
}

internal sealed class ObjectDto
{
    [JsonRequired, JsonPropertyOrder(0)] public string? DisplayText { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public List<string>? TypeNames { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public ulong Sequence { get; set; }
    [JsonRequired, JsonPropertyOrder(3)] public bool IsTruncated { get; set; }
    [JsonPropertyOrder(4)] public ValueDto? ScalarValue { get; set; }
    [JsonPropertyOrder(5)] public ValueDto? PropertyBag { get; set; }
    [JsonRequired, JsonPropertyOrder(6)] public uint PropertyEntryCount { get; set; }
    [JsonRequired, JsonPropertyOrder(7)] public uint DroppedPropertyEntryCount { get; set; }
    [JsonRequired, JsonPropertyOrder(8)] public uint TypeNameCount { get; set; }
    [JsonRequired, JsonPropertyOrder(9)] public uint DroppedTypeNameCount { get; set; }
    [JsonRequired, JsonPropertyOrder(10)] public bool IsPropertyBagTruncated { get; set; }
}

internal sealed class ErrorDto
{
    private int? exceptionHResult;

    [JsonRequired, JsonPropertyOrder(0)] public string? Message { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public string? FullyQualifiedErrorId { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public string? Category { get; set; }
    [JsonRequired, JsonPropertyOrder(3)] public string? ExceptionType { get; set; }
    [JsonPropertyOrder(4)]
    public int? ExceptionHResult
    {
        get => exceptionHResult;
        set
        {
            exceptionHResult = value;
            HasExceptionHResult = true;
        }
    }

    [JsonIgnore] public bool HasExceptionHResult { get; private set; }
    [JsonRequired, JsonPropertyOrder(5)] public string? InvocationName { get; set; }
    [JsonRequired, JsonPropertyOrder(6)] public string? PositionMessage { get; set; }
    [JsonRequired, JsonPropertyOrder(7)] public string? ScriptStackTrace { get; set; }
    [JsonRequired, JsonPropertyOrder(8)] public string? CategoryReason { get; set; }
    [JsonRequired, JsonPropertyOrder(9)] public string? CategoryActivity { get; set; }
    [JsonRequired, JsonPropertyOrder(10)] public string? CategoryTargetName { get; set; }
    [JsonRequired, JsonPropertyOrder(11)] public string? CategoryTargetType { get; set; }
    [JsonRequired, JsonPropertyOrder(12)] public string? CommandName { get; set; }
    [JsonRequired, JsonPropertyOrder(13)] public string? InvocationLine { get; set; }
    [JsonRequired, JsonPropertyOrder(14)] public string? OffsetInLine { get; set; }
    [JsonRequired, JsonPropertyOrder(15)] public string? PipelineLength { get; set; }
    [JsonRequired, JsonPropertyOrder(16)] public string? PipelinePosition { get; set; }
    [JsonRequired, JsonPropertyOrder(17)] public string? ErrorDetailsMessage { get; set; }
    [JsonRequired, JsonPropertyOrder(18)] public string? RecommendedAction { get; set; }
    [JsonRequired, JsonPropertyOrder(19)] public string? TargetDisplayText { get; set; }
    [JsonPropertyOrder(20)] public ValueDto? TargetValue { get; set; }
    [JsonRequired, JsonPropertyOrder(21)] public ulong Sequence { get; set; }
    [JsonRequired, JsonPropertyOrder(22)] public bool IsTruncated { get; set; }
}

internal sealed class TextDto
{
    [JsonRequired, JsonPropertyOrder(0)] public string? DisplayText { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public ulong Sequence { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public bool IsTruncated { get; set; }
}

internal sealed class ValueDto
{
    [JsonRequired, JsonPropertyOrder(0)] public uint Kind { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public string? Payload { get; set; }
}

internal sealed class SequenceDto
{
    [JsonRequired, JsonPropertyOrder(0)] public uint Stream { get; set; }
    [JsonRequired, JsonPropertyOrder(1)] public uint Index { get; set; }
    [JsonRequired, JsonPropertyOrder(2)] public ulong Sequence { get; set; }
}
