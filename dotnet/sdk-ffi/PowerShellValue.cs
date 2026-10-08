using System.Collections;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Text;

namespace Devolutions.PowerShell.Ffi;

public sealed class PowerShellValue
{
    public const int MaximumPayloadLength = 64 * 1024;
    public const int MaximumContainerEntries = 64;
    public const int MaximumDepth = 8;
    private readonly byte[] payload;

    private PowerShellValue(PowerShellValueKind kind, byte[] payload)
    {
        if (payload.Length > MaximumPayloadLength)
        {
            throw new ArgumentOutOfRangeException(nameof(payload), "Tagged value payload exceeds 64 KiB.");
        }

        ValidatePayload(kind, payload, 0);
        Kind = kind;
        this.payload = payload;
    }

    public PowerShellValueKind Kind { get; }

    public bool IsNull => Kind == PowerShellValueKind.Null;

    public static PowerShellValue Null { get; } = new(PowerShellValueKind.Null, System.Array.Empty<byte>());

    public static PowerShellValue String(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new PowerShellValue(PowerShellValueKind.String, EncodeUtf8(value));
    }

    public static PowerShellValue Switch(bool isPresent = true)
    {
        return new PowerShellValue(PowerShellValueKind.Switch, [isPresent ? (byte)1 : (byte)0]);
    }

    public static PowerShellValue Boolean(bool value)
    {
        return new PowerShellValue(PowerShellValueKind.Boolean, [value ? (byte)1 : (byte)0]);
    }

    public static PowerShellValue SignedInteger(long value)
    {
        return new PowerShellValue(PowerShellValueKind.SignedInteger, Int64Payload(value));
    }

    public static PowerShellValue UnsignedInteger(ulong value)
    {
        return new PowerShellValue(PowerShellValueKind.UnsignedInteger, UInt64Payload(value));
    }

    public static PowerShellValue Double(double value)
    {
        return new PowerShellValue(PowerShellValueKind.Double, Int64Payload(BitConverter.DoubleToInt64Bits(value)));
    }

    public static PowerShellValue Decimal(decimal value)
    {
        return new PowerShellValue(PowerShellValueKind.Decimal, EncodeUtf8(value.ToString(CultureInfo.InvariantCulture)));
    }

    public static PowerShellValue Bytes(byte[] value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new PowerShellValue(PowerShellValueKind.Bytes, (byte[])value.Clone());
    }

    public static PowerShellValue DateTime(DateTime value)
    {
        return new PowerShellValue(PowerShellValueKind.DateTime, Int64Payload(value.ToBinary()));
    }

    public static PowerShellValue DateTimeOffset(DateTimeOffset value)
    {
        int minutes = checked((int)value.Offset.TotalMinutes);
        var payload = new byte[sizeof(long) + sizeof(short)];
        WriteInt64(payload, 0, value.Ticks);
        payload[sizeof(long)] = (byte)minutes;
        payload[sizeof(long) + 1] = (byte)(minutes >> 8);
        return new PowerShellValue(PowerShellValueKind.DateTimeOffset, payload);
    }

    public static PowerShellValue TimeSpan(TimeSpan value)
    {
        return new PowerShellValue(PowerShellValueKind.TimeSpan, Int64Payload(value.Ticks));
    }

    public static PowerShellValue Guid(Guid value)
    {
        return new PowerShellValue(PowerShellValueKind.Guid, EncodeUtf8(value.ToString("D")));
    }

    public static PowerShellValue Uri(Uri value)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (!value.IsAbsoluteUri)
        {
            throw new ArgumentException("Only absolute URIs can cross the PowerShell FFI boundary.", nameof(value));
        }

        return new PowerShellValue(PowerShellValueKind.Uri, EncodeUtf8(value.AbsoluteUri));
    }

    public static PowerShellValue Array(IEnumerable<PowerShellValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        var encoded = new List<byte>();
        int countOffset = encoded.Count;
        WriteUInt32(encoded, 0);
        int count = 0;
        foreach (PowerShellValue value in values)
        {
            ArgumentNullException.ThrowIfNull(value);
            if (count == MaximumContainerEntries)
            {
                throw new ArgumentOutOfRangeException(nameof(values), "Tagged arrays contain at most 64 values.");
            }

            ValidateNestedValueDepth(value, 1, nameof(values));
            WriteNested(encoded, value);
            count++;
        }

        SetUInt32(encoded, countOffset, checked((uint)count));
        return new PowerShellValue(PowerShellValueKind.Array, encoded.ToArray());
    }

    public static PowerShellValue PropertyBag(IEnumerable<KeyValuePair<string, PowerShellValue>> properties)
    {
        ArgumentNullException.ThrowIfNull(properties);
        var encoded = new List<byte>();
        int countOffset = encoded.Count;
        WriteUInt32(encoded, 0);
        int count = 0;
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (KeyValuePair<string, PowerShellValue> property in properties)
        {
            ArgumentException.ThrowIfNullOrEmpty(property.Key);
            ArgumentNullException.ThrowIfNull(property.Value);
            if (!names.Add(property.Key))
            {
                throw new ArgumentException("Property bag keys must be unique.", nameof(properties));
            }
            if (count == MaximumContainerEntries)
            {
                throw new ArgumentOutOfRangeException(nameof(properties), "Property bags contain at most 64 values.");
            }

            byte[] name = EncodeUtf8(property.Key);
            ValidateNestedValueDepth(property.Value, 1, nameof(properties));
            WriteUInt32(encoded, checked((uint)name.Length));
            encoded.AddRange(name);
            WriteNested(encoded, property.Value);
            count++;
        }

        SetUInt32(encoded, countOffset, checked((uint)count));
        return new PowerShellValue(PowerShellValueKind.PropertyBag, encoded.ToArray());
    }

    public static PowerShellValue From(object? value)
    {
        return From(value, new HashSet<object>(ReferenceEqualityComparer.Instance), 0);
    }

    /// <summary>
    /// Gets a copied string when this is a tagged string value.
    /// </summary>
    public bool TryGetString(out string? value)
    {
        if (Kind != PowerShellValueKind.String)
        {
            value = null;
            return false;
        }

        value = DecodeUtf8(payload);
        return true;
    }

    /// <summary>
    /// Gets a copied switch presence value when this is a tagged switch.
    /// </summary>
    public bool TryGetSwitch(out bool value)
    {
        return TryGetBoolean(PowerShellValueKind.Switch, out value);
    }

    /// <summary>
    /// Gets a copied Boolean value when this is a tagged Boolean.
    /// </summary>
    public bool TryGetBoolean(out bool value)
    {
        return TryGetBoolean(PowerShellValueKind.Boolean, out value);
    }

    public bool TryGetSignedInteger(out long value)
    {
        if (Kind != PowerShellValueKind.SignedInteger)
        {
            value = default;
            return false;
        }

        value = ReadInt64(payload, 0);
        return true;
    }

    public bool TryGetUnsignedInteger(out ulong value)
    {
        if (Kind != PowerShellValueKind.UnsignedInteger)
        {
            value = default;
            return false;
        }

        value = ReadUInt64(payload, 0);
        return true;
    }

    public bool TryGetDouble(out double value)
    {
        if (Kind != PowerShellValueKind.Double)
        {
            value = default;
            return false;
        }

        value = BitConverter.Int64BitsToDouble(ReadInt64(payload, 0));
        return true;
    }

    public bool TryGetDecimal(out decimal value)
    {
        if (Kind != PowerShellValueKind.Decimal)
        {
            value = default;
            return false;
        }

        return decimal.TryParse(
            DecodeUtf8(payload),
            NumberStyles.Number,
            CultureInfo.InvariantCulture,
            out value);
    }

    /// <summary>
    /// Gets a copied byte array when this is a tagged byte value.
    /// </summary>
    public bool TryGetBytes(out byte[]? value)
    {
        if (Kind != PowerShellValueKind.Bytes)
        {
            value = null;
            return false;
        }

        value = (byte[])payload.Clone();
        return true;
    }

    public bool TryGetDateTime(out DateTime value)
    {
        if (Kind != PowerShellValueKind.DateTime)
        {
            value = default;
            return false;
        }

        value = System.DateTime.FromBinary(ReadInt64(payload, 0));
        return true;
    }

    public bool TryGetDateTimeOffset(out DateTimeOffset value)
    {
        if (Kind != PowerShellValueKind.DateTimeOffset)
        {
            value = default;
            return false;
        }

        value = DecodeDateTimeOffset(payload);
        return true;
    }

    public bool TryGetTimeSpan(out TimeSpan value)
    {
        if (Kind != PowerShellValueKind.TimeSpan)
        {
            value = default;
            return false;
        }

        value = System.TimeSpan.FromTicks(ReadInt64(payload, 0));
        return true;
    }

    public bool TryGetGuid(out Guid value)
    {
        if (Kind != PowerShellValueKind.Guid)
        {
            value = default;
            return false;
        }

        return System.Guid.TryParseExact(DecodeUtf8(payload), "D", out value);
    }

    public bool TryGetUri(out Uri? value)
    {
        if (Kind != PowerShellValueKind.Uri)
        {
            value = null;
            return false;
        }

        return System.Uri.TryCreate(DecodeUtf8(payload), System.UriKind.Absolute, out value);
    }

    /// <summary>
    /// Returns a copied, immutable sequence of the documented tagged values.
    /// </summary>
    public IReadOnlyList<PowerShellValue> GetArray()
    {
        if (Kind != PowerShellValueKind.Array)
        {
            throw new InvalidOperationException("The tagged value is not an array.");
        }

        var values = new List<PowerShellValue>();
        int offset = 0;
        uint count = ReadUInt32(payload, ref offset);
        for (uint index = 0; index < count; index++)
        {
            PowerShellValueKind kind = (PowerShellValueKind)ReadUInt32(payload, ref offset);
            byte[] nestedPayload = ReadBytes(payload, ref offset, ReadUInt32(payload, ref offset)).ToArray();
            values.Add(FromNative((uint)kind, nestedPayload));
        }

        if (offset != payload.Length)
        {
            throw InvalidNativeValue();
        }

        return System.Array.AsReadOnly(values.ToArray());
    }

    /// <summary>
    /// Returns a copied, immutable property bag of documented tagged values.
    /// </summary>
    public IReadOnlyDictionary<string, PowerShellValue> GetPropertyBag()
    {
        if (Kind != PowerShellValueKind.PropertyBag)
        {
            throw new InvalidOperationException("The tagged value is not a property bag.");
        }

        var properties = new Dictionary<string, PowerShellValue>(StringComparer.OrdinalIgnoreCase);
        int offset = 0;
        uint count = ReadUInt32(payload, ref offset);
        for (uint index = 0; index < count; index++)
        {
            string name = DecodeUtf8(ReadBytes(payload, ref offset, ReadUInt32(payload, ref offset)));
            PowerShellValueKind kind = (PowerShellValueKind)ReadUInt32(payload, ref offset);
            byte[] nestedPayload = ReadBytes(payload, ref offset, ReadUInt32(payload, ref offset)).ToArray();
            if (!properties.TryAdd(name, FromNative((uint)kind, nestedPayload)))
            {
                throw InvalidNativeValue();
            }
        }

        if (offset != payload.Length)
        {
            throw InvalidNativeValue();
        }

        return new ReadOnlyDictionary<string, PowerShellValue>(properties);
    }

    /// <summary>
    /// Looks up a copied property by its case-insensitive documented property name.
    /// </summary>
    public bool TryGetProperty(string name, out PowerShellValue? value)
    {
        ArgumentException.ThrowIfNullOrEmpty(name);
        if (Kind != PowerShellValueKind.PropertyBag)
        {
            value = null;
            return false;
        }

        return GetPropertyBag().TryGetValue(name, out value);
    }

    private static PowerShellValue From(object? value, HashSet<object> ancestors, int depth)
    {
        return value switch
        {
            null => Null,
            PowerShellValue tagged => ValidateNestedValueDepth(tagged, depth, nameof(value)),
            string text => String(text),
            bool boolean => Boolean(boolean),
            sbyte signed => SignedInteger(signed),
            short signed => SignedInteger(signed),
            int signed => SignedInteger(signed),
            long signed => SignedInteger(signed),
            byte unsigned => UnsignedInteger(unsigned),
            ushort unsigned => UnsignedInteger(unsigned),
            uint unsigned => UnsignedInteger(unsigned),
            ulong unsigned => UnsignedInteger(unsigned),
            float floating => Double(floating),
            double floating => Double(floating),
            decimal decimalValue => Decimal(decimalValue),
            byte[] bytes => Bytes(bytes),
            DateTime dateTime => DateTime(dateTime),
            DateTimeOffset dateTimeOffset => DateTimeOffset(dateTimeOffset),
            TimeSpan timeSpan => TimeSpan(timeSpan),
            Guid guid => Guid(guid),
            Uri uri => Uri(uri),
            Delegate callback => throw Unsupported(callback.GetType(), "Delegates cannot cross the PowerShell FFI boundary."),
            IDictionary<string, object?> properties => FromPropertyBag(properties, ancestors, depth),
            IEnumerable enumerable => FromArray(enumerable, ancestors, depth),
            _ => throw Unsupported(value.GetType(), "Only documented tagged primitives, arrays, and property bags can cross the PowerShell FFI boundary."),
        };
    }

    internal byte[] Payload => payload;

    internal bool IsSnapshotScalar =>
        Kind <= PowerShellValueKind.Uri || Kind == PowerShellValueKind.TimeSpan;

    internal bool IsSnapshotPropertyBag
    {
        get
        {
            if (Kind != PowerShellValueKind.PropertyBag)
            {
                return false;
            }

            try
            {
                int offset = 0;
                uint count = ReadUInt32(payload, ref offset);
                if (count > 16)
                {
                    return false;
                }

                for (uint index = 0; index < count; index++)
                {
                    string name = DecodeUtf8(ReadBytes(payload, ref offset, ReadUInt32(payload, ref offset)));
                    if (name.Length == 0 || name.Length > 128)
                    {
                        return false;
                    }

                    PowerShellValueKind nestedKind = (PowerShellValueKind)ReadUInt32(payload, ref offset);
                    ReadOnlySpan<byte> nestedPayload = ReadBytes(payload, ref offset, ReadUInt32(payload, ref offset));
                    if (nestedKind > PowerShellValueKind.Uri &&
                        nestedKind != PowerShellValueKind.TimeSpan)
                    {
                        return false;
                    }

                    ValidatePayload(nestedKind, nestedPayload, 0);
                }

                return offset == payload.Length;
            }
            catch (PowerShellFfiException)
            {
                return false;
            }
        }
    }

    internal static PowerShellValue FromNative(uint kind, byte[] payload)
    {
        ArgumentNullException.ThrowIfNull(payload);
        if (!Enum.IsDefined((PowerShellValueKind)kind))
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unknown tagged value kind.");
        }

        byte[] copy = (byte[])payload.Clone();
        return new PowerShellValue((PowerShellValueKind)kind, copy);
    }

    internal IReadOnlyList<PowerShellValue> GetArrayElements()
    {
        return GetArray();
    }

    private static PowerShellValue FromPropertyBag(
        IDictionary<string, object?> properties,
        HashSet<object> ancestors,
        int depth)
    {
        EnterContainer(properties, ancestors, depth);
        try
        {
            var converted = new List<KeyValuePair<string, PowerShellValue>>();
            foreach (KeyValuePair<string, object?> property in properties)
            {
                if (converted.Count == MaximumContainerEntries)
                {
                    throw new ArgumentOutOfRangeException(
                        nameof(properties),
                        "Property bags contain at most 64 values.");
                }

                converted.Add(new KeyValuePair<string, PowerShellValue>(
                    property.Key,
                    From(property.Value, ancestors, depth + 1)));
            }

            return PropertyBag(converted);
        }
        finally
        {
            ancestors.Remove(properties);
        }
    }

    private static PowerShellValue FromArray(
        IEnumerable values,
        HashSet<object> ancestors,
        int depth)
    {
        EnterContainer(values, ancestors, depth);
        try
        {
            var converted = new List<PowerShellValue>();
            foreach (object? value in values)
            {
                if (converted.Count == MaximumContainerEntries)
                {
                    throw new ArgumentOutOfRangeException(
                        nameof(values),
                        "Tagged arrays contain at most 64 values.");
                }

                converted.Add(From(value, ancestors, depth + 1));
            }

            return Array(converted);
        }
        finally
        {
            ancestors.Remove(values);
        }
    }

    private static void EnterContainer(object value, HashSet<object> ancestors, int depth)
    {
        if (depth > MaximumDepth)
        {
            throw new ArgumentOutOfRangeException(nameof(value), "Tagged value nesting exceeds eight levels.");
        }

        if (!ancestors.Add(value))
        {
            throw new PowerShellValueConversionException(
                value.GetType(),
                "Cyclic object graphs cannot cross the PowerShell FFI boundary.");
        }
    }

    private static PowerShellValueConversionException Unsupported(Type type, string message)
    {
        return new PowerShellValueConversionException(type, message);
    }

    private static PowerShellValue ValidateNestedValueDepth(PowerShellValue value, int depth, string parameterName)
    {
        try
        {
            ValidatePayload(value.Kind, value.payload, depth);
        }
        catch (PowerShellFfiException exception) when (exception.Status == PowerShellFfiStatus.ManagedFailure)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                "Nested tagged values exceed the maximum depth of eight levels.");
        }

        return value;
    }

    private static byte[] EncodeUtf8(string value)
    {
        if (value.IndexOf('\0') >= 0)
        {
            throw new ArgumentException("Tagged UTF-8 values cannot contain NUL characters.", nameof(value));
        }

        return new UTF8Encoding(false, true).GetBytes(value);
    }

    private static byte[] Int64Payload(long value)
    {
        var payload = new byte[sizeof(long)];
        WriteInt64(payload, 0, value);
        return payload;
    }

    private static byte[] UInt64Payload(ulong value)
    {
        var payload = new byte[sizeof(ulong)];
        for (int index = 0; index < payload.Length; index++)
        {
            payload[index] = (byte)(value >> (index * 8));
        }

        return payload;
    }

    private static void WriteNested(List<byte> target, PowerShellValue value)
    {
        WriteUInt32(target, (uint)value.Kind);
        WriteUInt32(target, checked((uint)value.payload.Length));
        target.AddRange(value.payload);
    }

    private static void WriteUInt32(List<byte> target, uint value)
    {
        target.Add((byte)value);
        target.Add((byte)(value >> 8));
        target.Add((byte)(value >> 16));
        target.Add((byte)(value >> 24));
    }

    private static void SetUInt32(List<byte> target, int offset, uint value)
    {
        target[offset] = (byte)value;
        target[offset + 1] = (byte)(value >> 8);
        target[offset + 2] = (byte)(value >> 16);
        target[offset + 3] = (byte)(value >> 24);
    }

    private static void WriteInt64(byte[] target, int offset, long value)
    {
        for (int index = 0; index < sizeof(long); index++)
        {
            target[offset + index] = (byte)(value >> (index * 8));
        }
    }

    private bool TryGetBoolean(PowerShellValueKind expectedKind, out bool value)
    {
        if (Kind != expectedKind)
        {
            value = default;
            return false;
        }

        value = payload[0] != 0;
        return true;
    }

    private static DateTimeOffset DecodeDateTimeOffset(ReadOnlySpan<byte> value)
    {
        long ticks = ReadInt64(value, 0);
        short offsetMinutes = unchecked((short)(value[sizeof(long)] | (value[sizeof(long) + 1] << 8)));
        return new System.DateTimeOffset(ticks, System.TimeSpan.FromMinutes(offsetMinutes));
    }

    private static long ReadInt64(ReadOnlySpan<byte> value, int offset)
    {
        return unchecked((long)ReadUInt64(value, offset));
    }

    private static ulong ReadUInt64(ReadOnlySpan<byte> value, int offset)
    {
        if (offset < 0 || value.Length - offset < sizeof(ulong))
        {
            throw InvalidNativeValue();
        }

        ulong result = 0;
        for (int index = 0; index < sizeof(ulong); index++)
        {
            result |= (ulong)value[offset + index] << (index * 8);
        }

        return result;
    }

    private static void ValidatePayload(PowerShellValueKind kind, ReadOnlySpan<byte> value, int depth)
        {
            if (depth > MaximumDepth)
            {
                throw InvalidNativeValue();
            }

            switch (kind)
            {
                case PowerShellValueKind.Null:
                    if (!value.IsEmpty) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.Switch:
                case PowerShellValueKind.Boolean:
                    if (value.Length != 1 || value[0] > 1) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.SignedInteger:
                case PowerShellValueKind.UnsignedInteger:
                case PowerShellValueKind.Double:
                    if (value.Length != sizeof(long)) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.DateTime:
                    if (value.Length != sizeof(long)) throw InvalidNativeValue();
                    try
                    {
                        _ = System.DateTime.FromBinary(ReadInt64(value, 0));
                    }
                    catch (ArgumentException)
                    {
                        throw InvalidNativeValue();
                    }
                    return;
                case PowerShellValueKind.TimeSpan:
                    if (value.Length != sizeof(long)) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.DateTimeOffset:
                    if (value.Length != sizeof(long) + sizeof(short)) throw InvalidNativeValue();
                    try
                    {
                        _ = DecodeDateTimeOffset(value);
                    }
                    catch (ArgumentOutOfRangeException)
                    {
                        throw InvalidNativeValue();
                    }
                    return;
                case PowerShellValueKind.Bytes:
                    return;
                case PowerShellValueKind.String:
                    ValidateUtf8(value);
                    return;
                case PowerShellValueKind.Decimal:
                    if (!decimal.TryParse(
                        DecodeUtf8(value),
                        NumberStyles.Number,
                        CultureInfo.InvariantCulture,
                        out _))
                    {
                        throw InvalidNativeValue();
                    }
                    return;
                case PowerShellValueKind.Guid:
                    if (!System.Guid.TryParseExact(DecodeUtf8(value), "D", out _)) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.Uri:
                    if (!System.Uri.TryCreate(DecodeUtf8(value), System.UriKind.Absolute, out _)) throw InvalidNativeValue();
                    return;
                case PowerShellValueKind.Array:
                    ValidateArray(value, depth);
                    return;
                case PowerShellValueKind.PropertyBag:
                    ValidatePropertyBag(value, depth);
                    return;
                default:
                    throw InvalidNativeValue();
            }
        }

        private static void ValidateArray(ReadOnlySpan<byte> value, int depth)
        {
            int offset = 0;
            uint count = ReadUInt32(value, ref offset);
            if (count > MaximumContainerEntries) throw InvalidNativeValue();
            for (uint index = 0; index < count; index++)
            {
                PowerShellValueKind kind = (PowerShellValueKind)ReadUInt32(value, ref offset);
                uint length = ReadUInt32(value, ref offset);
                ReadOnlySpan<byte> nested = ReadBytes(value, ref offset, length);
                ValidatePayload(kind, nested, depth + 1);
            }

            if (offset != value.Length) throw InvalidNativeValue();
        }

        private static void ValidatePropertyBag(ReadOnlySpan<byte> value, int depth)
        {
            int offset = 0;
            uint count = ReadUInt32(value, ref offset);
            if (count > MaximumContainerEntries) throw InvalidNativeValue();
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            for (uint index = 0; index < count; index++)
            {
                uint nameLength = ReadUInt32(value, ref offset);
                string name = DecodeUtf8(ReadBytes(value, ref offset, nameLength));
                if (name.Length == 0 || !names.Add(name)) throw InvalidNativeValue();
                PowerShellValueKind kind = (PowerShellValueKind)ReadUInt32(value, ref offset);
                uint length = ReadUInt32(value, ref offset);
                ReadOnlySpan<byte> nested = ReadBytes(value, ref offset, length);
                ValidatePayload(kind, nested, depth + 1);
            }

            if (offset != value.Length) throw InvalidNativeValue();
        }

        private static uint ReadUInt32(ReadOnlySpan<byte> value, ref int offset)
        {
            ReadOnlySpan<byte> bytes = ReadBytes(value, ref offset, sizeof(uint));
            return (uint)(bytes[0] | (bytes[1] << 8) | (bytes[2] << 16) | (bytes[3] << 24));
        }

        private static ReadOnlySpan<byte> ReadBytes(ReadOnlySpan<byte> value, ref int offset, uint length)
        {
            if (length > value.Length - offset) throw InvalidNativeValue();
            ReadOnlySpan<byte> result = value.Slice(offset, checked((int)length));
            offset += checked((int)length);
            return result;
        }

        private static void ValidateUtf8(ReadOnlySpan<byte> value)
        {
            _ = DecodeUtf8(value);
        }

        private static string DecodeUtf8(ReadOnlySpan<byte> value)
        {
            try
            {
                string text = new UTF8Encoding(false, true).GetString(value);
                if (text.IndexOf('\0') >= 0) throw InvalidNativeValue();
                return text;
            }
            catch (DecoderFallbackException)
            {
                throw InvalidNativeValue();
            }
        }

        private static PowerShellFfiException InvalidNativeValue()
        {
            return new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an invalid tagged value payload.");
    }
}
