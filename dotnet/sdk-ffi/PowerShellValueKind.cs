namespace Devolutions.PowerShell.Ffi;

public enum PowerShellValueKind : uint
{
    Null = 0,
    String = 1,
    Switch = 2,
    Boolean = 3,
    SignedInteger = 4,
    UnsignedInteger = 5,
    Double = 6,
    Decimal = 7,
    Bytes = 8,
    DateTime = 9,
    DateTimeOffset = 10,
    Guid = 11,
    Uri = 12,
    Array = 13,
    PropertyBag = 14,
    TimeSpan = 17,
}
