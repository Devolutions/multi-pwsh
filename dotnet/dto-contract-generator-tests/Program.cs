using Devolutions.PowerShell.Ffi;
using Devolutions.MultiPwsh.DtoContract.Generator;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

PowerShellValue valid = SampleDtoPowerShellDtoProjection.Write(new SampleDto
{
    Name = "alpha",
    Enabled = true,
    Identifiers = [1, 2, 3],
});

if (!SampleDtoPowerShellDtoProjection.TryRead(valid, out SampleDto? roundTrip, out PowerShellDtoProjectionError? error) ||
    error is not null ||
    roundTrip is not { Name: "alpha", Enabled: true, Identifiers: not null } ||
    !roundTrip.Identifiers.SequenceEqual([1L, 2L, 3L]))
{
    throw new InvalidOperationException("Generated PowerShell DTO projection did not round-trip a bounded contract.");
}

PowerShellValue unknown = PowerShellValue.PropertyBag(
[
    new("$version", PowerShellValue.UnsignedInteger(2)),
    new("Name", PowerShellValue.String("alpha")),
    new("Enabled", PowerShellValue.Boolean(true)),
    new("Identifiers", PowerShellValue.Array([PowerShellValue.SignedInteger(1)])),
    new("Unexpected", PowerShellValue.String("rejected")),
]);
if (SampleDtoPowerShellDtoProjection.TryRead(unknown, out _, out error) ||
    error?.Failure != PowerShellDtoProjectionFailure.UnknownMember)
{
    throw new InvalidOperationException("Generated PowerShell DTO projection accepted an unknown member.");
}

PowerShellValue wrongVersion = PowerShellValue.PropertyBag(
[
    new("$version", PowerShellValue.UnsignedInteger(1)),
    new("Name", PowerShellValue.String("alpha")),
    new("Enabled", PowerShellValue.Boolean(true)),
    new("Identifiers", PowerShellValue.Array([PowerShellValue.SignedInteger(1)])),
]);
if (SampleDtoPowerShellDtoProjection.TryRead(wrongVersion, out _, out error) ||
    error?.Failure != PowerShellDtoProjectionFailure.InvalidVersion)
{
    throw new InvalidOperationException("Generated PowerShell DTO projection accepted an incompatible version.");
}

PowerShellValue keywordProperty = KeywordAndStringsDtoPowerShellDtoProjection.Write(new KeywordAndStringsDto
{
    @event = ["one", "two"],
});
if (!KeywordAndStringsDtoPowerShellDtoProjection.TryRead(keywordProperty, out KeywordAndStringsDto? keywordRoundTrip, out error) ||
    error is not null ||
    keywordRoundTrip is null ||
    !keywordRoundTrip.@event.SequenceEqual(["one", "two"]))
{
    throw new InvalidOperationException("Generated PowerShell DTO projection did not support a keyword property identifier.");
}

ExpectValueTooLarge(
    () => KeywordAndStringsDtoPowerShellDtoProjection.Write(new KeywordAndStringsDto { @event = ["exceeds"] }),
    "Generated PowerShell DTO projection did not enforce nested string bounds when writing an array.");

PowerShellValue firstShared = First.Contracts.SharedDtoPowerShellDtoProjection.Write(new First.Contracts.SharedDto { Name = "first" });
PowerShellValue secondShared = Second.Contracts.SharedDtoPowerShellDtoProjection.Write(new Second.Contracts.SharedDto { Name = "second" });
if (!firstShared.TryGetProperty("Name", out PowerShellValue? firstName) ||
    !firstName!.TryGetString(out string? firstText) ||
    firstText != "first" ||
    !secondShared.TryGetProperty("Name", out PowerShellValue? secondName) ||
    !secondName!.TryGetString(out string? secondText) ||
    secondText != "second")
{
    throw new InvalidOperationException("Generated PowerShell DTO projections with duplicate simple type names conflicted.");
}

if (SixtyThreeMemberDtoPowerShellDtoProjection.Write(new SixtyThreeMemberDto()).GetPropertyBag().Count != 64)
{
    throw new InvalidOperationException("Generated PowerShell DTO projection did not reserve one property bag entry for $version.");
}

VerifyCheckedNarrowIntegerBoundaries();
VerifyNullableSupportedScalars();
VerifyNullableArrayElements();
VerifyNonFlagsEnumsWithExplicitUnderlyingConversions();
VerifyTimeSpanProjection();
VerifyBoundedNestedDtoProjection();
VerifyNestedDtoGeneratorDiagnostics();
VerifyNestedFailureCategories();
VerifyTaggedValueDepthBoundaries();

VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public abstract class AbstractDto
    {
        [PowerShellDtoMember] public bool Enabled { get; set; }
    }
    """,
    "MPWDTO001",
    "an abstract DTO");
VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public class RequiredDto
    {
        [PowerShellDtoMember] public required string Name { get; set; }
    }
    """,
    "MPWDTO002",
    "a required DTO property");
VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public class IndexedDto
    {
        [PowerShellDtoMember] public string this[int index] { get => string.Empty; set { } }
    }
    """,
    "MPWDTO002",
    "a DTO indexer");
VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public class InitOnlyDto
    {
        [PowerShellDtoMember] public string Name { get; init; } = string.Empty;
    }
    """,
    "MPWDTO002",
    "an init-only DTO property");
VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public class ReservedNameDto
    {
        [PowerShellDtoMember("$VERSION")] public bool Value { get; set; }
    }
    """,
    "MPWDTO002",
    "a case-insensitive $version member name");
VerifyGeneratorDiagnostic(
    """
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public struct ValueDto
    {
        [PowerShellDtoMember] public bool Value { get; set; }
    }
    """,
    "MPWDTO001",
    "a struct DTO");
VerifyGeneratorDiagnostic(
    "using Devolutions.PowerShell.Ffi; [PowerShellDtoContract(1)] public class TooManyMembersDto { " +
    string.Concat(Enumerable.Range(1, 64).Select(static index =>
        $"[PowerShellDtoMember] public bool Value{index} {{ get; set; }} ")) +
    "}",
    "MPWDTO001",
    "a DTO with more than 63 members");
VerifyGeneratorDiagnostic(
    """
    using System;
    using Devolutions.PowerShell.Ffi;
    [PowerShellDtoContract(1)]
    public sealed class FlagsDto
    {
        [PowerShellDtoMember] public UnsupportedFlags Value { get; set; }
    }
    [Flags]
    public enum UnsupportedFlags : ushort
    {
        None = 0,
        First = 1,
        Second = 2,
    }
    """,
    "MPWDTO002",
    "a [Flags] enum DTO member",
    "Flags");

static void ExpectValueTooLarge(Action action, string description)
{
    try
    {
        action();
    }
    catch (PowerShellDtoProjectionException exception) when (exception.Error.Failure == PowerShellDtoProjectionFailure.ValueTooLarge)
    {
        return;
    }

    throw new InvalidOperationException(description);
}

static void VerifyCheckedNarrowIntegerBoundaries()
{
    var minimum = new NarrowIntegerDto
    {
        SignedByte = sbyte.MinValue,
        SignedShort = short.MinValue,
        SignedInt = int.MinValue,
        Byte = byte.MinValue,
        UnsignedShort = ushort.MinValue,
        UnsignedInt = uint.MinValue,
    };
    var maximum = new NarrowIntegerDto
    {
        SignedByte = sbyte.MaxValue,
        SignedShort = short.MaxValue,
        SignedInt = int.MaxValue,
        Byte = byte.MaxValue,
        UnsignedShort = ushort.MaxValue,
        UnsignedInt = uint.MaxValue,
    };

    NarrowIntegerDto minimumResult = NarrowIntegerDtoPowerShellDtoProjection.Read(
        NarrowIntegerDtoPowerShellDtoProjection.Write(minimum));
    NarrowIntegerDto maximumResult = NarrowIntegerDtoPowerShellDtoProjection.Read(
        NarrowIntegerDtoPowerShellDtoProjection.Write(maximum));
    if (minimumResult.SignedByte != sbyte.MinValue ||
        minimumResult.SignedShort != short.MinValue ||
        minimumResult.SignedInt != int.MinValue ||
        minimumResult.Byte != byte.MinValue ||
        minimumResult.UnsignedShort != ushort.MinValue ||
        minimumResult.UnsignedInt != uint.MinValue ||
        maximumResult.SignedByte != sbyte.MaxValue ||
        maximumResult.SignedShort != short.MaxValue ||
        maximumResult.SignedInt != int.MaxValue ||
        maximumResult.Byte != byte.MaxValue ||
        maximumResult.UnsignedShort != ushort.MaxValue ||
        maximumResult.UnsignedInt != uint.MaxValue)
    {
        throw new InvalidOperationException("Generated DTO projection did not preserve checked narrow integer boundaries.");
    }

    PowerShellValue maximumWire = NarrowIntegerDtoPowerShellDtoProjection.Write(maximum);
    RequireSignedMember(maximumWire, nameof(NarrowIntegerDto.SignedByte), sbyte.MaxValue);
    RequireSignedMember(maximumWire, nameof(NarrowIntegerDto.SignedShort), short.MaxValue);
    RequireSignedMember(maximumWire, nameof(NarrowIntegerDto.SignedInt), int.MaxValue);
    RequireUnsignedMember(maximumWire, nameof(NarrowIntegerDto.Byte), byte.MaxValue);
    RequireUnsignedMember(maximumWire, nameof(NarrowIntegerDto.UnsignedShort), ushort.MaxValue);
    RequireUnsignedMember(maximumWire, nameof(NarrowIntegerDto.UnsignedInt), uint.MaxValue);

    (string Member, PowerShellValue Invalid)[] invalidValues =
    [
        (nameof(NarrowIntegerDto.SignedByte), PowerShellValue.SignedInteger((long)sbyte.MinValue - 1)),
        (nameof(NarrowIntegerDto.SignedByte), PowerShellValue.SignedInteger((long)sbyte.MaxValue + 1)),
        (nameof(NarrowIntegerDto.SignedShort), PowerShellValue.SignedInteger((long)short.MinValue - 1)),
        (nameof(NarrowIntegerDto.SignedShort), PowerShellValue.SignedInteger((long)short.MaxValue + 1)),
        (nameof(NarrowIntegerDto.SignedInt), PowerShellValue.SignedInteger((long)int.MinValue - 1)),
        (nameof(NarrowIntegerDto.SignedInt), PowerShellValue.SignedInteger((long)int.MaxValue + 1)),
        (nameof(NarrowIntegerDto.Byte), PowerShellValue.SignedInteger(-1)),
        (nameof(NarrowIntegerDto.Byte), PowerShellValue.UnsignedInteger((ulong)byte.MaxValue + 1)),
        (nameof(NarrowIntegerDto.UnsignedShort), PowerShellValue.SignedInteger(-1)),
        (nameof(NarrowIntegerDto.UnsignedShort), PowerShellValue.UnsignedInteger((ulong)ushort.MaxValue + 1)),
        (nameof(NarrowIntegerDto.UnsignedInt), PowerShellValue.SignedInteger(-1)),
        (nameof(NarrowIntegerDto.UnsignedInt), PowerShellValue.UnsignedInteger((ulong)uint.MaxValue + 1)),
    ];
    foreach ((string member, PowerShellValue invalid) in invalidValues)
    {
        PowerShellValue wire = ReplaceProperty(maximumWire, member, invalid);
        if (NarrowIntegerDtoPowerShellDtoProjection.TryRead(wire, out _, out PowerShellDtoProjectionError? error) ||
            error is not { Failure: PowerShellDtoProjectionFailure.InvalidValue } ||
            error?.Path != member)
        {
            throw new InvalidOperationException(
                $"Generated DTO projection did not reject the out-of-range {member} boundary with its concrete member path.");
        }
    }
}

static void VerifyNullableSupportedScalars()
{
    DateTime dateTime = new(2026, 10, 7, 13, 27, 45, DateTimeKind.Utc);
    DateTimeOffset dateTimeOffset = new(2026, 10, 7, 9, 27, 45, TimeSpan.FromHours(-4));
    Guid guid = Guid.Parse("e4d7c9a4-d175-463e-91dd-987687db7258");
    Uri uri = new("https://example.test/nullable");
    TimeSpan duration = TimeSpan.FromTicks(-987654321);
    var populated = new NullableScalarDto
    {
        Text = "value",
        Boolean = true,
        SignedByte = sbyte.MinValue,
        SignedShort = short.MaxValue,
        SignedInt = int.MinValue,
        SignedLong = long.MaxValue,
        Byte = byte.MaxValue,
        UnsignedShort = ushort.MaxValue,
        UnsignedInt = uint.MaxValue,
        UnsignedLong = ulong.MaxValue,
        Double = -1.25,
        Decimal = 79228162514264337593543950335m,
        DateTime = dateTime,
        DateTimeOffset = dateTimeOffset,
        Guid = guid,
        Uri = uri,
        Duration = duration,
        State = SignedState.Negative,
    };

    NullableScalarDto populatedResult = NullableScalarDtoPowerShellDtoProjection.Read(
        NullableScalarDtoPowerShellDtoProjection.Write(populated));
    if (populatedResult.Text != "value" ||
        populatedResult.Boolean != true ||
        populatedResult.SignedByte != sbyte.MinValue ||
        populatedResult.SignedShort != short.MaxValue ||
        populatedResult.SignedInt != int.MinValue ||
        populatedResult.SignedLong != long.MaxValue ||
        populatedResult.Byte != byte.MaxValue ||
        populatedResult.UnsignedShort != ushort.MaxValue ||
        populatedResult.UnsignedInt != uint.MaxValue ||
        populatedResult.UnsignedLong != ulong.MaxValue ||
        populatedResult.Double != -1.25 ||
        populatedResult.Decimal != decimal.MaxValue ||
        populatedResult.DateTime != dateTime ||
        populatedResult.DateTimeOffset != dateTimeOffset ||
        populatedResult.Guid != guid ||
        populatedResult.Uri != uri ||
        populatedResult.Duration != duration ||
        populatedResult.State != SignedState.Negative)
    {
        throw new InvalidOperationException("Generated DTO projection did not preserve populated nullable supported scalars.");
    }

    PowerShellValue nullWire = NullableScalarDtoPowerShellDtoProjection.Write(new NullableScalarDto());
    if (nullWire.GetPropertyBag().Count != 19 ||
        nullWire.GetPropertyBag()
        .Where(static property => property.Key != PowerShellDtoProjection.VersionMemberName)
        .Any(static property => property.Value.Kind != PowerShellValueKind.Null))
    {
        throw new InvalidOperationException("Generated DTO projection did not encode nullable scalar nulls with the Null tagged kind.");
    }

    NullableScalarDto nullResult = NullableScalarDtoPowerShellDtoProjection.Read(nullWire);
    if (nullResult.Text is not null ||
        nullResult.Boolean is not null ||
        nullResult.SignedByte is not null ||
        nullResult.SignedShort is not null ||
        nullResult.SignedInt is not null ||
        nullResult.SignedLong is not null ||
        nullResult.Byte is not null ||
        nullResult.UnsignedShort is not null ||
        nullResult.UnsignedInt is not null ||
        nullResult.UnsignedLong is not null ||
        nullResult.Double is not null ||
        nullResult.Decimal is not null ||
        nullResult.DateTime is not null ||
        nullResult.DateTimeOffset is not null ||
        nullResult.Guid is not null ||
        nullResult.Uri is not null ||
        nullResult.Duration is not null ||
        nullResult.State is not null)
    {
        throw new InvalidOperationException("Generated DTO projection did not restore nullable scalar nulls.");
    }
}

static void VerifyNonFlagsEnumsWithExplicitUnderlyingConversions()
{
    var expected = new EnumDto
    {
        Signed = SignedState.Negative,
        Unsigned = UnsignedState.Maximum,
    };
    PowerShellValue wire = EnumDtoPowerShellDtoProjection.Write(expected);
    RequireSignedMember(wire, nameof(EnumDto.Signed), -2);
    RequireUnsignedMember(wire, nameof(EnumDto.Unsigned), uint.MaxValue);

    EnumDto result = EnumDtoPowerShellDtoProjection.Read(wire);
    if (result.Signed != SignedState.Negative || result.Unsigned != UnsignedState.Maximum)
    {
        throw new InvalidOperationException("Generated DTO projection did not use explicit enum underlying integer conversions.");
    }

    VerifyInvalidEnumRead(
        ReplaceProperty(wire, nameof(EnumDto.Signed), PowerShellValue.SignedInteger(-1)),
        nameof(EnumDto.Signed));
    VerifyInvalidEnumRead(
        ReplaceProperty(wire, nameof(EnumDto.Unsigned), PowerShellValue.UnsignedInteger(1)),
        nameof(EnumDto.Unsigned));
}

static void VerifyNullableArrayElements()
{
    var expected = new NullableArrayDto
    {
        Numbers = [null, int.MinValue, null, int.MaxValue],
        Durations = [TimeSpan.MinValue, null, TimeSpan.MaxValue],
        Labels = [null, "four", string.Empty],
        States = [SignedState.Negative, null, SignedState.Positive],
    };
    PowerShellValue wire = NullableArrayDtoPowerShellDtoProjection.Write(expected);
    NullableArrayDto actual = NullableArrayDtoPowerShellDtoProjection.Read(wire);
    if (!actual.Numbers.SequenceEqual(expected.Numbers) ||
        !actual.Durations.SequenceEqual(expected.Durations) ||
        !actual.Labels.SequenceEqual(expected.Labels) ||
        !actual.States.SequenceEqual(expected.States) ||
        !wire.GetPropertyBag()[nameof(NullableArrayDto.Numbers)].GetArray()[0].IsNull ||
        !wire.GetPropertyBag()[nameof(NullableArrayDto.Durations)].GetArray()[1].IsNull)
    {
        throw new InvalidOperationException("Nullable scalar array elements did not preserve nulls and populated values.");
    }

    NullableArrayDto empty = NullableArrayDtoPowerShellDtoProjection.Read(
        NullableArrayDtoPowerShellDtoProjection.Write(new NullableArrayDto()));
    if (empty.Numbers.Length != 0 || empty.Durations.Length != 0 ||
        empty.Labels.Length != 0 || empty.States.Length != 0)
    {
        throw new InvalidOperationException("Nullable scalar arrays did not preserve empty collections.");
    }

    (string Member, PowerShellValue Invalid)[] invalidElements =
    [
        (nameof(NullableArrayDto.Numbers), PowerShellValue.String("wrong")),
        (nameof(NullableArrayDto.Numbers), PowerShellValue.SignedInteger((long)int.MaxValue + 1)),
        (nameof(NullableArrayDto.Durations), PowerShellValue.SignedInteger(1)),
        (nameof(NullableArrayDto.States), PowerShellValue.SignedInteger(-1)),
    ];
    foreach ((string member, PowerShellValue invalid) in invalidElements)
    {
        PowerShellValue invalidWire = ReplaceProperty(wire, member, PowerShellValue.Array([PowerShellValue.Null, invalid]));
        if (NullableArrayDtoPowerShellDtoProjection.TryRead(invalidWire, out _, out var error) ||
            error?.Failure != PowerShellDtoProjectionFailure.InvalidValue ||
            error.Path != member)
        {
            throw new InvalidOperationException($"Nullable array {member} accepted an invalid populated element.");
        }
    }

    ExpectValueTooLarge(
        () => NullableArrayDtoPowerShellDtoProjection.Write(new NullableArrayDto { Labels = [null, "fives"] }),
        "Nullable string arrays did not enforce populated element bounds.");
    if (NullableArrayDtoPowerShellDtoProjection.TryRead(
        ReplaceProperty(wire, nameof(NullableArrayDto.Labels), PowerShellValue.Array([PowerShellValue.Null, PowerShellValue.String("fives")])),
        out _, out var oversizedError) ||
        oversizedError?.Failure != PowerShellDtoProjectionFailure.ValueTooLarge)
    {
        throw new InvalidOperationException("Nullable string array reads did not enforce populated element bounds.");
    }
    if (SampleDtoPowerShellDtoProjection.TryRead(
        ReplaceProperty(SampleDtoPowerShellDtoProjection.Write(new SampleDto()), nameof(SampleDto.Identifiers), PowerShellValue.Array([PowerShellValue.Null])),
        out _, out var nonNullableError) ||
        nonNullableError?.Failure != PowerShellDtoProjectionFailure.InvalidValue)
    {
        throw new InvalidOperationException("Non-nullable scalar arrays accepted a null element.");
    }
}

static void VerifyTimeSpanProjection()
{
    var expected = new TimeSpanDto
    {
        Negative = TimeSpan.FromTicks(-1),
        Positive = TimeSpan.MaxValue,
    };
    PowerShellValue wire = TimeSpanDtoPowerShellDtoProjection.Write(expected);
    IReadOnlyDictionary<string, PowerShellValue> properties = wire.GetPropertyBag();
    TimeSpan negative = default;
    TimeSpan positive = default;
    if (!properties[nameof(TimeSpanDto.Negative)].TryGetTimeSpan(out negative) ||
        properties[nameof(TimeSpanDto.Negative)].Kind != PowerShellValueKind.TimeSpan ||
        negative.Ticks != -1 ||
        !properties[nameof(TimeSpanDto.Positive)].TryGetTimeSpan(out positive) ||
        properties[nameof(TimeSpanDto.Positive)].Kind != PowerShellValueKind.TimeSpan ||
        positive.Ticks != TimeSpan.MaxValue.Ticks)
    {
        throw new InvalidOperationException("Generated DTO projection did not encode TimeSpan values with exact ticks.");
    }

    TimeSpanDto result = TimeSpanDtoPowerShellDtoProjection.Read(wire);
    if (result.Negative != expected.Negative || result.Positive != expected.Positive)
    {
        throw new InvalidOperationException("Generated DTO projection did not round-trip negative and positive TimeSpan boundaries.");
    }
}

static void VerifyBoundedNestedDtoProjection()
{
    var expected = new ParentDto
    {
        Child = new ChildDto { Label = "root", Count = int.MinValue },
        Children =
        [
            new ChildDto { Label = "one", Count = 1 },
            new ChildDto { Label = "two", Count = int.MaxValue },
        ],
    };
    PowerShellValue wire = ParentDtoPowerShellDtoProjection.Write(expected);
    IReadOnlyDictionary<string, PowerShellValue> properties = wire.GetPropertyBag();
    if (properties[nameof(ParentDto.Child)].Kind != PowerShellValueKind.PropertyBag ||
        properties[nameof(ParentDto.Children)].Kind != PowerShellValueKind.Array ||
        properties[nameof(ParentDto.Children)].GetArray().Count != 2)
    {
        throw new InvalidOperationException("Generated DTO projection did not encode nested DTOs as bounded property bags.");
    }

    ParentDto result = ParentDtoPowerShellDtoProjection.Read(wire);
    if (result.Child is not { Label: "root", Count: int.MinValue } ||
        result.Children.Length != 2 ||
        result.Children[0] is not { Label: "one", Count: 1 } ||
        result.Children[1] is not { Label: "two", Count: int.MaxValue })
    {
        throw new InvalidOperationException("Generated DTO projection did not round-trip a nested DTO and one-dimensional nested DTO array.");
    }

    VerifyParentWriteFailure(
        new ParentDto
        {
            Child = new ChildDto { Label = "fives", Count = 0 },
            Children = [],
        },
        PowerShellDtoProjectionFailure.ValueTooLarge,
        "Child.Label",
        "Generated DTO projection did not enforce the nested child string bound.");
    VerifyParentWriteFailure(
        new ParentDto
        {
            Child = new ChildDto { Label = "root", Count = 0 },
            Children =
            [
                new ChildDto { Label = "one", Count = 1 },
                new ChildDto { Label = "two", Count = 2 },
                new ChildDto { Label = "tri", Count = 3 },
            ],
        },
        PowerShellDtoProjectionFailure.ValueTooLarge,
        nameof(ParentDto.Children),
        "Generated DTO projection did not enforce the nested DTO array count bound.");

    PowerShellValue oversizedChild = PowerShellDtoProjection.CreatePropertyBag(
        1,
        [
            new(nameof(ChildDto.Label), PowerShellValue.String("fives")),
            new(nameof(ChildDto.Count), PowerShellValue.SignedInteger(0)),
        ]);
    VerifyParentReadFailure(
        PowerShellDtoProjection.CreatePropertyBag(
            1,
            [
                new(nameof(ParentDto.Child), oversizedChild),
                new(nameof(ParentDto.Children), PowerShellValue.Array([])),
            ]),
        PowerShellDtoProjectionFailure.ValueTooLarge,
        "Child.Label",
        "Generated DTO projection accepted an oversized nested child string while reading.");

    PowerShellValue validChild = ChildDtoPowerShellDtoProjection.Write(
        new ChildDto { Label = "root", Count = 0 });
    VerifyParentReadFailure(
        PowerShellDtoProjection.CreatePropertyBag(
            1,
            [
                new(nameof(ParentDto.Child), validChild),
                new(
                    nameof(ParentDto.Children),
                    PowerShellValue.Array([validChild, validChild, validChild])),
            ]),
        PowerShellDtoProjectionFailure.ValueTooLarge,
        nameof(ParentDto.Children),
        "Generated DTO projection accepted too many nested DTO array elements while reading.");
    VerifyParentReadFailure(
        PowerShellDtoProjection.CreatePropertyBag(
            1,
            [
                new(nameof(ParentDto.Child), PowerShellValue.SignedInteger(1)),
                new(nameof(ParentDto.Children), PowerShellValue.Array([])),
            ]),
        PowerShellDtoProjectionFailure.InvalidRoot,
        nameof(ParentDto.Child),
        "Generated DTO projection accepted a non-property-bag nested DTO.");
}

static void VerifyNestedFailureCategories()
{
    PowerShellValue child = ChildDtoPowerShellDtoProjection.Write(new ChildDto { Label = "root", Count = 1 });
    (PowerShellValue Value, PowerShellDtoProjectionFailure Failure, string Path, string Message)[] cases =
    [
        (PowerShellValue.Boolean(true), PowerShellDtoProjectionFailure.InvalidRoot, "", "Expected a copied property bag."),
        (PowerShellValue.PropertyBag(child.GetPropertyBag().Where(property => property.Key != nameof(ChildDto.Count))),
            PowerShellDtoProjectionFailure.MissingMember, "Count", "The required DTO member is missing."),
        (PowerShellValue.PropertyBag(child.GetPropertyBag().Append(new("extra", PowerShellValue.Boolean(true)))),
            PowerShellDtoProjectionFailure.UnknownMember, "", "The DTO contains an undeclared member."),
        (ReplaceProperty(child, "$version", PowerShellValue.UnsignedInteger(2)),
            PowerShellDtoProjectionFailure.InvalidVersion, "", "The DTO version is missing or incompatible."),
        (ReplaceProperty(child, nameof(ChildDto.Count), PowerShellValue.String("wrong")),
            PowerShellDtoProjectionFailure.InvalidValue, "Count", "The DTO member has an invalid tagged value kind."),
        (ReplaceProperty(child, nameof(ChildDto.Label), PowerShellValue.String("fives")),
            PowerShellDtoProjectionFailure.ValueTooLarge, "Label", "The DTO string member has an invalid kind or exceeds its bound."),
    ];
    foreach (var item in cases)
    {
        foreach (bool inArray in new[] { false, true })
        {
            string prefix = inArray ? nameof(ParentDto.Children) : nameof(ParentDto.Child);
            PowerShellValue parent = PowerShellDtoProjection.CreatePropertyBag(
                1,
                [
                    new(nameof(ParentDto.Child), inArray ? child : item.Value),
                    new(nameof(ParentDto.Children), PowerShellValue.Array(inArray ? [item.Value] : [])),
                ]);
            string expectedPath = item.Path.Length == 0 ? prefix : $"{prefix}.{item.Path}";
            if (ParentDtoPowerShellDtoProjection.TryRead(parent, out _, out var error) ||
                error?.Failure != item.Failure || error.Path != expectedPath || error.Message != item.Message)
            {
                throw new InvalidOperationException($"Nested {prefix} read lost {item.Failure}, its path, or its message.");
            }
        }
    }
}

static void VerifyTaggedValueDepthBoundaries()
{
    var expected = new DepthBoundaryRoot
    {
        Children = [new DepthBoundaryMiddle
        {
            Children = [new DepthBoundaryBranch
            {
                Children = [new DepthBoundaryLeaf { Values = [int.MinValue, int.MaxValue] }],
            }],
        }],
    };
    DepthBoundaryRoot actual = DepthBoundaryRootPowerShellDtoProjection.Read(
        DepthBoundaryRootPowerShellDtoProjection.Write(expected));
    if (!actual.Children.Single().Children.Single().Children.Single().Values.SequenceEqual([int.MinValue, int.MaxValue]))
    {
        throw new InvalidOperationException("The eight-level tagged-value boundary did not round-trip.");
    }

    foreach ((int edges, bool dtoArrays, bool scalarArray) in new[] { (4, true, false), (7, false, true), (8, false, false) })
    {
        var source = new System.Text.StringBuilder("using Devolutions.PowerShell.Ffi;");
        for (int index = 0; index <= edges; index++)
        {
            string type = index == edges ? (scalarArray ? "int[]" : "int") : $"BoundaryDto{index + 1}" + (dtoArrays ? "[]" : "");
            string initializer = type == "int" ? "" : " = null!;";
            source.AppendLine($$"""
                [PowerShellDtoContract(1)]
                public sealed class BoundaryDto{{index}}
                {
                    [PowerShellDtoMember] public {{type}} Value { get; set; }{{initializer}}
                }
                """);
        }
        VerifyGeneratorDiagnostic(source.ToString(), "MPWDTO001", "an over-depth tagged DTO graph", "depth");
    }
}

static void VerifyNestedDtoGeneratorDiagnostics()
{
    VerifyGeneratorDiagnostic(
        """
        using Devolutions.PowerShell.Ffi;
        [PowerShellDtoContract(1)]
        public sealed class SelfCycleDto
        {
            [PowerShellDtoMember] public SelfCycleDto Value { get; set; } = null!;
        }
        """,
        "MPWDTO001",
        "a self-referential DTO cycle",
        "cycle");
    VerifyGeneratorDiagnostic(
        """
        using Devolutions.PowerShell.Ffi;
        [PowerShellDtoContract(1)]
        public sealed class FirstCycleDto
        {
            [PowerShellDtoMember] public SecondCycleDto Value { get; set; } = null!;
        }
        [PowerShellDtoContract(1)]
        public sealed class SecondCycleDto
        {
            [PowerShellDtoMember] public FirstCycleDto Value { get; set; } = null!;
        }
        """,
        "MPWDTO001",
        "a mutually-referential DTO cycle",
        "cycle");

    var depthSource = new System.Text.StringBuilder(
        "using Devolutions.PowerShell.Ffi;" + Environment.NewLine);
    for (int index = 0; index <= PowerShellValue.MaximumDepth + 1; index++)
    {
        string propertyType = index == PowerShellValue.MaximumDepth + 1
            ? "bool"
            : $"DepthDto{index + 1}";
        string initializer = propertyType == "bool" ? string.Empty : " = null!;";
        depthSource.AppendLine($$"""
            [PowerShellDtoContract(1)]
            public sealed class DepthDto{{index}}
            {
                [PowerShellDtoMember] public {{propertyType}} Value { get; set; }{{initializer}}
            }
            """);
    }
    VerifyGeneratorDiagnostic(
        depthSource.ToString(),
        "MPWDTO001",
        "a DTO graph deeper than the tagged value bound",
        "depth");
}

static PowerShellValue ReplaceProperty(PowerShellValue source, string member, PowerShellValue replacement)
{
    return PowerShellValue.PropertyBag(
        source.GetPropertyBag().Select(property =>
            new KeyValuePair<string, PowerShellValue>(
                property.Key,
                string.Equals(property.Key, member, StringComparison.Ordinal)
                    ? replacement
                    : property.Value)));
}

static void RequireSignedMember(PowerShellValue source, string member, long expected)
{
    if (!source.TryGetProperty(member, out PowerShellValue? value) ||
        value!.Kind != PowerShellValueKind.SignedInteger ||
        !value.TryGetSignedInteger(out long actual) ||
        actual != expected)
    {
        throw new InvalidOperationException($"Generated DTO projection did not encode {member} as signed integer {expected}.");
    }
}

static void RequireUnsignedMember(PowerShellValue source, string member, ulong expected)
{
    if (!source.TryGetProperty(member, out PowerShellValue? value) ||
        value!.Kind != PowerShellValueKind.UnsignedInteger ||
        !value.TryGetUnsignedInteger(out ulong actual) ||
        actual != expected)
    {
        throw new InvalidOperationException($"Generated DTO projection did not encode {member} as unsigned integer {expected}.");
    }
}

static void VerifyInvalidEnumRead(PowerShellValue wire, string member)
{
    if (EnumDtoPowerShellDtoProjection.TryRead(wire, out _, out PowerShellDtoProjectionError? error) ||
        error is not { Failure: PowerShellDtoProjectionFailure.InvalidValue } ||
        error?.Path != member)
    {
        throw new InvalidOperationException($"Generated DTO projection accepted undefined enum value for {member}.");
    }
}

static void VerifyParentReadFailure(
    PowerShellValue wire,
    PowerShellDtoProjectionFailure expectedFailure,
    string expectedPath,
    string description)
{
    if (ParentDtoPowerShellDtoProjection.TryRead(wire, out _, out PowerShellDtoProjectionError? error) ||
        error?.Failure != expectedFailure ||
        error?.Path != expectedPath)
    {
        throw new InvalidOperationException(description);
    }
}

static void VerifyParentWriteFailure(
    ParentDto value,
    PowerShellDtoProjectionFailure expectedFailure,
    string expectedPath,
    string description)
{
    try
    {
        _ = ParentDtoPowerShellDtoProjection.Write(value);
    }
    catch (PowerShellDtoProjectionException exception)
        when (exception.Error.Failure == expectedFailure && exception.Error.Path == expectedPath)
    {
        return;
    }

    throw new InvalidOperationException(description);
}

static void VerifyGeneratorDiagnostic(
    string source,
    string expectedDiagnostic,
    string description,
    string? expectedMessageFragment = null)
{
    CSharpCompilation compilation = CSharpCompilation.Create(
        "DtoContractGeneratorRegression",
        [CSharpSyntaxTree.ParseText(source, new CSharpParseOptions(LanguageVersion.Preview))],
        [
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(System.Reflection.Assembly.Load("System.Runtime").Location),
            MetadataReference.CreateFromFile(typeof(PowerShellDtoContractAttribute).Assembly.Location),
        ],
        new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
    GeneratorDriver driver = CSharpGeneratorDriver.Create(
        [new DtoContractGenerator().AsSourceGenerator()],
        parseOptions: new CSharpParseOptions(LanguageVersion.Preview));
    driver.RunGeneratorsAndUpdateCompilation(compilation, out Compilation output, out var generatorDiagnostics);
    Diagnostic[] diagnostics = generatorDiagnostics.Concat(output.GetDiagnostics()).ToArray();
    if (!diagnostics.Any(diagnostic =>
            diagnostic.Id == expectedDiagnostic &&
            (expectedMessageFragment is null ||
             diagnostic.GetMessage().Contains(expectedMessageFragment, StringComparison.OrdinalIgnoreCase))))
    {
        throw new InvalidOperationException(
            $"The DTO generator did not reject {description} with {expectedDiagnostic}" +
            (expectedMessageFragment is null ? string.Empty : $" containing '{expectedMessageFragment}'") +
            $": {string.Join("; ", diagnostics.Select(static diagnostic => $"{diagnostic.Id}: {diagnostic.GetMessage()}"))}");
    }
}

[PowerShellDtoContract(2)]
public sealed class SampleDto
{
    [PowerShellDtoMember(MaximumStringLength = 16)]
    public string Name { get; set; } = string.Empty;

    [PowerShellDtoMember]
    public bool Enabled { get; set; }

    [PowerShellDtoMember(MaximumCollectionCount = 8)]
    public long[] Identifiers { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class KeywordAndStringsDto
{
    [PowerShellDtoMember(MaximumStringLength = 4, MaximumCollectionCount = 4)]
    public string[] @event { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class NarrowIntegerDto
{
    [PowerShellDtoMember] public sbyte SignedByte { get; set; }
    [PowerShellDtoMember] public short SignedShort { get; set; }
    [PowerShellDtoMember] public int SignedInt { get; set; }
    [PowerShellDtoMember] public byte Byte { get; set; }
    [PowerShellDtoMember] public ushort UnsignedShort { get; set; }
    [PowerShellDtoMember] public uint UnsignedInt { get; set; }
}

public enum SignedState : short
{
    Negative = -2,
    Positive = short.MaxValue,
}

public enum UnsignedState : uint
{
    Zero = 0,
    Maximum = uint.MaxValue,
}

[PowerShellDtoContract(1)]
public sealed class NullableScalarDto
{
    [PowerShellDtoMember] public string? Text { get; set; }
    [PowerShellDtoMember] public bool? Boolean { get; set; }
    [PowerShellDtoMember] public sbyte? SignedByte { get; set; }
    [PowerShellDtoMember] public short? SignedShort { get; set; }
    [PowerShellDtoMember] public int? SignedInt { get; set; }
    [PowerShellDtoMember] public long? SignedLong { get; set; }
    [PowerShellDtoMember] public byte? Byte { get; set; }
    [PowerShellDtoMember] public ushort? UnsignedShort { get; set; }
    [PowerShellDtoMember] public uint? UnsignedInt { get; set; }
    [PowerShellDtoMember] public ulong? UnsignedLong { get; set; }
    [PowerShellDtoMember] public double? Double { get; set; }
    [PowerShellDtoMember] public decimal? Decimal { get; set; }
    [PowerShellDtoMember] public DateTime? DateTime { get; set; }
    [PowerShellDtoMember] public DateTimeOffset? DateTimeOffset { get; set; }
    [PowerShellDtoMember] public Guid? Guid { get; set; }
    [PowerShellDtoMember] public Uri? Uri { get; set; }
    [PowerShellDtoMember] public TimeSpan? Duration { get; set; }
    [PowerShellDtoMember] public SignedState? State { get; set; }
}

[PowerShellDtoContract(1)]
public sealed class NullableArrayDto
{
    [PowerShellDtoMember] public int?[] Numbers { get; set; } = [];
    [PowerShellDtoMember] public TimeSpan?[] Durations { get; set; } = [];
    [PowerShellDtoMember(MaximumStringLength = 4)] public string?[] Labels { get; set; } = [];
    [PowerShellDtoMember] public SignedState?[] States { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class DepthBoundaryRoot
{
    [PowerShellDtoMember] public DepthBoundaryMiddle[] Children { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class DepthBoundaryMiddle
{
    [PowerShellDtoMember] public DepthBoundaryBranch[] Children { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class DepthBoundaryBranch
{
    [PowerShellDtoMember] public DepthBoundaryLeaf[] Children { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class DepthBoundaryLeaf
{
    [PowerShellDtoMember] public int[] Values { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class EnumDto
{
    [PowerShellDtoMember] public SignedState Signed { get; set; }
    [PowerShellDtoMember] public UnsignedState Unsigned { get; set; }
}

[PowerShellDtoContract(1)]
public sealed class TimeSpanDto
{
    [PowerShellDtoMember] public TimeSpan Negative { get; set; }
    [PowerShellDtoMember] public TimeSpan Positive { get; set; }
}

[PowerShellDtoContract(1)]
public sealed class ChildDto
{
    [PowerShellDtoMember(MaximumStringLength = 4)]
    public string Label { get; set; } = string.Empty;

    [PowerShellDtoMember]
    public int Count { get; set; }
}

[PowerShellDtoContract(1)]
public sealed class ParentDto
{
    [PowerShellDtoMember]
    public ChildDto Child { get; set; } = new();

    [PowerShellDtoMember(MaximumCollectionCount = 2)]
    public ChildDto[] Children { get; set; } = [];
}

[PowerShellDtoContract(1)]
public sealed class SixtyThreeMemberDto
{
    [PowerShellDtoMember] public bool Value01 { get; set; }
    [PowerShellDtoMember] public bool Value02 { get; set; }
    [PowerShellDtoMember] public bool Value03 { get; set; }
    [PowerShellDtoMember] public bool Value04 { get; set; }
    [PowerShellDtoMember] public bool Value05 { get; set; }
    [PowerShellDtoMember] public bool Value06 { get; set; }
    [PowerShellDtoMember] public bool Value07 { get; set; }
    [PowerShellDtoMember] public bool Value08 { get; set; }
    [PowerShellDtoMember] public bool Value09 { get; set; }
    [PowerShellDtoMember] public bool Value10 { get; set; }
    [PowerShellDtoMember] public bool Value11 { get; set; }
    [PowerShellDtoMember] public bool Value12 { get; set; }
    [PowerShellDtoMember] public bool Value13 { get; set; }
    [PowerShellDtoMember] public bool Value14 { get; set; }
    [PowerShellDtoMember] public bool Value15 { get; set; }
    [PowerShellDtoMember] public bool Value16 { get; set; }
    [PowerShellDtoMember] public bool Value17 { get; set; }
    [PowerShellDtoMember] public bool Value18 { get; set; }
    [PowerShellDtoMember] public bool Value19 { get; set; }
    [PowerShellDtoMember] public bool Value20 { get; set; }
    [PowerShellDtoMember] public bool Value21 { get; set; }
    [PowerShellDtoMember] public bool Value22 { get; set; }
    [PowerShellDtoMember] public bool Value23 { get; set; }
    [PowerShellDtoMember] public bool Value24 { get; set; }
    [PowerShellDtoMember] public bool Value25 { get; set; }
    [PowerShellDtoMember] public bool Value26 { get; set; }
    [PowerShellDtoMember] public bool Value27 { get; set; }
    [PowerShellDtoMember] public bool Value28 { get; set; }
    [PowerShellDtoMember] public bool Value29 { get; set; }
    [PowerShellDtoMember] public bool Value30 { get; set; }
    [PowerShellDtoMember] public bool Value31 { get; set; }
    [PowerShellDtoMember] public bool Value32 { get; set; }
    [PowerShellDtoMember] public bool Value33 { get; set; }
    [PowerShellDtoMember] public bool Value34 { get; set; }
    [PowerShellDtoMember] public bool Value35 { get; set; }
    [PowerShellDtoMember] public bool Value36 { get; set; }
    [PowerShellDtoMember] public bool Value37 { get; set; }
    [PowerShellDtoMember] public bool Value38 { get; set; }
    [PowerShellDtoMember] public bool Value39 { get; set; }
    [PowerShellDtoMember] public bool Value40 { get; set; }
    [PowerShellDtoMember] public bool Value41 { get; set; }
    [PowerShellDtoMember] public bool Value42 { get; set; }
    [PowerShellDtoMember] public bool Value43 { get; set; }
    [PowerShellDtoMember] public bool Value44 { get; set; }
    [PowerShellDtoMember] public bool Value45 { get; set; }
    [PowerShellDtoMember] public bool Value46 { get; set; }
    [PowerShellDtoMember] public bool Value47 { get; set; }
    [PowerShellDtoMember] public bool Value48 { get; set; }
    [PowerShellDtoMember] public bool Value49 { get; set; }
    [PowerShellDtoMember] public bool Value50 { get; set; }
    [PowerShellDtoMember] public bool Value51 { get; set; }
    [PowerShellDtoMember] public bool Value52 { get; set; }
    [PowerShellDtoMember] public bool Value53 { get; set; }
    [PowerShellDtoMember] public bool Value54 { get; set; }
    [PowerShellDtoMember] public bool Value55 { get; set; }
    [PowerShellDtoMember] public bool Value56 { get; set; }
    [PowerShellDtoMember] public bool Value57 { get; set; }
    [PowerShellDtoMember] public bool Value58 { get; set; }
    [PowerShellDtoMember] public bool Value59 { get; set; }
    [PowerShellDtoMember] public bool Value60 { get; set; }
    [PowerShellDtoMember] public bool Value61 { get; set; }
    [PowerShellDtoMember] public bool Value62 { get; set; }
    [PowerShellDtoMember] public bool Value63 { get; set; }
}

namespace First.Contracts
{
    [PowerShellDtoContract(1)]
    public sealed class SharedDto
    {
        [PowerShellDtoMember]
        public string Name { get; set; } = string.Empty;
    }
}

namespace Second.Contracts
{
    [PowerShellDtoContract(1)]
    public sealed class SharedDto
    {
        [PowerShellDtoMember]
        public string Name { get; set; } = string.Empty;
    }
}
