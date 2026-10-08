using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using Devolutions.PowerShell.Ffi;

Assembly facadeAssembly = typeof(PowerShell).Assembly;
Type nativeMethodsType = facadeAssembly.GetType("Devolutions.PowerShell.Ffi.NativeMethods", throwOnError: true)!;
const BindingFlags PublicDeclared = BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly;
const BindingFlags StaticNonPublic = BindingFlags.Static | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;
const BindingFlags InstanceDeclared = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

var publicBaseline = new List<string>();
foreach (Type type in facadeAssembly.GetExportedTypes().OrderBy(type => type.FullName, StringComparer.Ordinal))
{
    publicBaseline.Add($"facade:type:{type.FullName}");

    foreach (ConstructorInfo constructor in type.GetConstructors(PublicDeclared).OrderBy(constructor => constructor.ToString(), StringComparer.Ordinal))
    {
        publicBaseline.Add($"facade:ctor:{type.FullName}::{constructor}");
    }

    foreach (PropertyInfo property in type.GetProperties(PublicDeclared).OrderBy(property => property.Name, StringComparer.Ordinal))
    {
        publicBaseline.Add($"facade:property:{type.FullName}::{property}");
    }

    foreach (MethodInfo method in type.GetMethods(PublicDeclared).Where(method => !method.IsSpecialName).OrderBy(method => method.ToString(), StringComparer.Ordinal))
    {
        publicBaseline.Add($"facade:method:{type.FullName}::{method}");
    }

    foreach (FieldInfo field in type.GetFields(PublicDeclared).Where(field => !field.IsSpecialName).OrderBy(field => field.Name, StringComparer.Ordinal))
    {
        publicBaseline.Add($"facade:field:{type.FullName}::{field}");
    }
}

NativeStructInspection[] nativeStructs = facadeAssembly.GetTypes()
    .Where(type =>
        type.Namespace is "Devolutions.PowerShell.Ffi" or "Devolutions.PowerShell.Ffi.LiveObjects" &&
        type.IsValueType &&
        type.Name.StartsWith("Native", StringComparison.Ordinal))
    .OrderBy(type => type.Name, StringComparer.Ordinal)
    .Select(type => new NativeStructInspection(
        type.Name,
        Marshal.SizeOf(type),
        type.GetFields(InstanceDeclared)
            .OrderBy(field => field.MetadataToken)
            .Select(field => new NativeFieldInspection(
                field.Name,
                Marshal.OffsetOf(type, field.Name).ToInt64(),
                GetManagedTypeName(field.FieldType)))
            .ToArray()))
    .ToArray();

Type statusType = facadeAssembly.GetType("Devolutions.PowerShell.Ffi.PowerShellFfiStatus", throwOnError: true)!;
var statuses = Enum.GetNames(statusType)
    .ToDictionary(name => name, name => Convert.ToInt32(Enum.Parse(statusType, name)), StringComparer.Ordinal);

NativeImportInspection[] nativeImports = nativeMethodsType.GetMethods(StaticNonPublic)
    .Select(method =>
    {
        CustomAttributeData? libraryImport = method.CustomAttributes.SingleOrDefault(attribute =>
            attribute.AttributeType.FullName == "System.Runtime.InteropServices.LibraryImportAttribute");
        if (libraryImport is null)
        {
            return null;
        }

        CustomAttributeNamedArgument entryPoint = libraryImport.NamedArguments.Single(argument => argument.MemberName == "EntryPoint");
        return new NativeImportInspection((string)entryPoint.TypedValue.Value!, GetManagedTypeName(method.ReturnType));
    })
    .Where(import => import is not null)
    .Select(import => import!)
    .OrderBy(import => import.EntryPoint, StringComparer.Ordinal)
    .ToArray();

ValidateAbiCompatibility(facadeAssembly, StaticNonPublic);
ValidatePowerShellValueTimeSpanPayload();

var inspection = new FacadeInspection(
    facadeAssembly.GetReferencedAssemblies().Select(reference => reference.Name!).OrderBy(name => name, StringComparer.Ordinal).ToArray(),
    publicBaseline,
    nativeStructs,
    statuses,
    nativeImports);

Console.WriteLine(JsonSerializer.Serialize(inspection));

static string GetManagedTypeName(Type type)
{
    if (type.IsPointer)
    {
        return $"{GetManagedTypeName(type.GetElementType()!)}*";
    }

    return type == typeof(UIntPtr) ? "System.UIntPtr" : type.FullName!;
}

static void ValidateAbiCompatibility(Assembly facadeAssembly, BindingFlags staticNonPublic)
{
    Type powerShellType = facadeAssembly.GetType("Devolutions.PowerShell.Ffi.PowerShell", throwOnError: true)!;
    Type abiInfoType = facadeAssembly.GetType("Devolutions.PowerShell.Ffi.NativeAbiInfo", throwOnError: true)!;
    MethodInfo? ensureSupportedAbi = powerShellType.GetMethod(
        "EnsureSupportedAbi",
        staticNonPublic,
        binder: null,
        types: [abiInfoType],
        modifiers: null);
    if (ensureSupportedAbi is null)
    {
        throw new InvalidOperationException("The facade must retain an ABI validation overload that accepts NativeAbiInfo.");
    }

    const ulong allRequiredFeatures = 0x1C1FFFDFF;
    ensureSupportedAbi.Invoke(null, [CreateAbiInfo(abiInfoType, allRequiredFeatures, abiVersion: 2, minimumCompatibleAbiVersion: 2)]);
    foreach (int bit in Enumerable.Range(0, 25).Concat([30, 31, 32]))
    {
        if ((allRequiredFeatures & (1UL << bit)) == 0)
        {
            continue;
        }

        if (!IsRejected(
                ensureSupportedAbi,
                CreateAbiInfo(abiInfoType, allRequiredFeatures ^ (1UL << bit), abiVersion: 2, minimumCompatibleAbiVersion: 2)))
        {
            throw new InvalidOperationException($"Facade ABI validation accepted an absent required feature bit {bit}.");
        }
    }

    if (!IsRejected(ensureSupportedAbi, CreateAbiInfo(abiInfoType, allRequiredFeatures, abiVersion: 1, minimumCompatibleAbiVersion: 1)))
    {
        throw new InvalidOperationException("Facade ABI validation accepted an incompatible ABI version.");
    }

    if (!IsRejected(ensureSupportedAbi, CreateAbiInfo(abiInfoType, allRequiredFeatures, abiVersion: 3, minimumCompatibleAbiVersion: 2)))
    {
        throw new InvalidOperationException("Facade ABI validation accepted an incompatible ABI version.");
    }

    if (!IsRejected(ensureSupportedAbi, CreateAbiInfo(abiInfoType, allRequiredFeatures, abiVersion: 2, minimumCompatibleAbiVersion: 3)))
    {
        throw new InvalidOperationException("Facade ABI validation accepted an incompatible minimum ABI version.");
    }

}

static object CreateAbiInfo(Type abiInfoType, ulong featureFlags, uint abiVersion, uint minimumCompatibleAbiVersion)
{
    object instance = Activator.CreateInstance(abiInfoType)!;
    const BindingFlags instanceNonPublic = BindingFlags.Instance | BindingFlags.NonPublic;
    abiInfoType.GetField("Size", instanceNonPublic)!.SetValue(instance, 24U);
    abiInfoType.GetField("AbiVersion", instanceNonPublic)!.SetValue(instance, abiVersion);
    abiInfoType.GetField("FeatureFlags", instanceNonPublic)!.SetValue(instance, featureFlags);
    abiInfoType.GetField("MinimumCompatibleAbiVersion", instanceNonPublic)!.SetValue(instance, minimumCompatibleAbiVersion);
    abiInfoType.GetField("Reserved", instanceNonPublic)!.SetValue(instance, 0U);
    return instance;
}

static bool IsRejected(MethodInfo ensureSupportedAbi, object abiInfo)
{
    try
    {
        ensureSupportedAbi.Invoke(null, [abiInfo]);
        return false;
    }
    catch (TargetInvocationException exception) when (exception.InnerException is NotSupportedException)
    {
        return true;
    }
}

static void ValidatePowerShellValueTimeSpanPayload()
{
    if ((uint)PowerShellValueKind.TimeSpan != 17)
    {
        throw new InvalidOperationException("PowerShellValueKind.TimeSpan must append at wire kind 17 without renumbering V1 values.");
    }

    TimeSpan[] expectedValues =
    [
        TimeSpan.MinValue,
        TimeSpan.FromTicks(-1),
        TimeSpan.Zero,
        TimeSpan.MaxValue,
    ];
    PropertyInfo payloadProperty = typeof(PowerShellValue).GetProperty(
        "Payload",
        BindingFlags.Instance | BindingFlags.NonPublic)!;
    foreach (TimeSpan expected in expectedValues)
    {
        PowerShellValue value = PowerShellValue.TimeSpan(expected);
        TimeSpan actual = default;
        if (value.Kind != PowerShellValueKind.TimeSpan ||
            !value.TryGetTimeSpan(out actual) ||
            actual.Ticks != expected.Ticks)
        {
            throw new InvalidOperationException($"PowerShellValue TimeSpan did not round-trip exact ticks {expected.Ticks}.");
        }

        byte[] payload = (byte[])payloadProperty.GetValue(value)!;
        byte[] expectedPayload = Enumerable.Range(0, sizeof(long))
            .Select(index => (byte)(expected.Ticks >> (index * 8)))
            .ToArray();
        if (!payload.SequenceEqual(expectedPayload))
        {
            throw new InvalidOperationException($"PowerShellValue TimeSpan payload was not an eight-byte little-endian tick count for {expected.Ticks}.");
        }
    }

    PowerShellValue copied = PowerShellValue.From(TimeSpan.FromTicks(-123456789));
    TimeSpan copiedResult = default;
    if (copied.Kind != PowerShellValueKind.TimeSpan ||
        !copied.TryGetTimeSpan(out copiedResult) ||
        copiedResult.Ticks != -123456789)
    {
        throw new InvalidOperationException("PowerShellValue.From did not preserve a copied TimeSpan.");
    }

    if (PowerShellValue.SignedInteger(-123456789).TryGetTimeSpan(out TimeSpan wrongKindResult) ||
        wrongKindResult != default)
    {
        throw new InvalidOperationException("TryGetTimeSpan accepted a non-TimeSpan tagged value.");
    }

    MethodInfo fromNative = typeof(PowerShellValue).GetMethod(
        "FromNative",
        BindingFlags.Static | BindingFlags.NonPublic)!;
    long nativeTicks = -72623859790382856;
    byte[] nativePayload = Enumerable.Range(0, sizeof(long))
        .Select(index => (byte)(nativeTicks >> (index * 8)))
        .ToArray();
    PowerShellValue nativeCopy = (PowerShellValue)fromNative.Invoke(
        null,
        [(uint)PowerShellValueKind.TimeSpan, nativePayload])!;
    Array.Fill(nativePayload, (byte)0);
    if (!nativeCopy.TryGetTimeSpan(out TimeSpan nativeResult) ||
        nativeResult.Ticks != nativeTicks)
    {
        throw new InvalidOperationException("PowerShellValue did not decode and detach a copied native TimeSpan payload.");
    }

    foreach (int invalidLength in new[] { sizeof(long) - 1, sizeof(long) + 1 })
    {
        try
        {
            _ = fromNative.Invoke(null, [(uint)PowerShellValueKind.TimeSpan, new byte[invalidLength]]);
            throw new InvalidOperationException($"PowerShellValue accepted a malformed TimeSpan payload length of {invalidLength}.");
        }
        catch (TargetInvocationException exception) when (exception.InnerException is PowerShellFfiException)
        {
        }
    }
}

internal sealed record FacadeInspection(
    string[] References,
    List<string> PublicBaseline,
    NativeStructInspection[] NativeStructs,
    Dictionary<string, int> Statuses,
    NativeImportInspection[] NativeImports);

internal sealed record NativeStructInspection(string Name, int Size, NativeFieldInspection[] Fields);

internal sealed record NativeFieldInspection(string Name, long Offset, string TypeName);

internal sealed record NativeImportInspection(string EntryPoint, string ReturnType);
