#nullable enable

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.Text;
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;

namespace Devolutions.MultiPwsh.DtoContract.Generator;

[Generator]
public sealed class DtoContractGenerator : IIncrementalGenerator
{
    private const string ContractAttribute = "Devolutions.PowerShell.Ffi.PowerShellDtoContractAttribute";
    private const string MemberAttribute = "Devolutions.PowerShell.Ffi.PowerShellDtoMemberAttribute";
    private const int MaximumDtoDepth = 8;

    private static readonly DiagnosticDescriptor InvalidContract = new(
        "MPWDTO001",
        "Invalid PowerShell DTO contract",
        "PowerShell DTO contract '{0}' {1}",
        "MultiPwsh.DTO",
        DiagnosticSeverity.Error,
        true);

    private static readonly DiagnosticDescriptor UnsupportedMember = new(
        "MPWDTO002",
        "Unsupported PowerShell DTO member",
        "PowerShell DTO member '{0}' {1}",
        "MultiPwsh.DTO",
        DiagnosticSeverity.Error,
        true);

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        IncrementalValuesProvider<INamedTypeSymbol> contracts = context.SyntaxProvider
            .CreateSyntaxProvider(
                static (node, _) => node is Microsoft.CodeAnalysis.CSharp.Syntax.TypeDeclarationSyntax { AttributeLists.Count: > 0 },
                static (syntax, _) => syntax.SemanticModel.GetDeclaredSymbol(syntax.Node) as INamedTypeSymbol)
            .Where(static type => type is not null && HasAttribute(type, ContractAttribute))
            .Select(static (type, _) => type!);

        context.RegisterSourceOutput(contracts.Collect(), static (production, types) =>
        {
            INamedTypeSymbol[] distinctTypes = types
                .Distinct(SymbolEqualityComparer.Default)
                .OfType<INamedTypeSymbol>()
                .ToArray();
            var invalidGraphs = new HashSet<INamedTypeSymbol>(SymbolEqualityComparer.Default);
            foreach (INamedTypeSymbol type in distinctTypes)
            {
                if (!TryValidateDtoGraph(
                    type,
                    new HashSet<INamedTypeSymbol>(SymbolEqualityComparer.Default),
                    0,
                    out string reason))
                {
                    production.ReportDiagnostic(Diagnostic.Create(
                        InvalidContract,
                        Location(type),
                        type.Name,
                        reason));
                    invalidGraphs.Add(type);
                }
            }

            foreach (INamedTypeSymbol type in distinctTypes)
            {
                if (invalidGraphs.Contains(type))
                {
                    continue;
                }

                ContractInfo? contract = Analyze(type, production);
                if (contract is not null)
                {
                    production.AddSource(
                        GetHintName(type),
                        SourceText.From(Emit(contract), Encoding.UTF8));
                }
            }
        });
    }

    private static bool TryValidateDtoGraph(
        INamedTypeSymbol type,
        HashSet<INamedTypeSymbol> ancestors,
        int depth,
        out string reason)
    {
        if (depth > MaximumDtoDepth)
        {
            reason = "contains a nested DTO graph whose depth exceeds eight levels";
            return false;
        }

        if (!ancestors.Add(type))
        {
            reason = "contains a nested DTO cycle";
            return false;
        }

        try
        {
            foreach (IPropertySymbol property in type.GetMembers().OfType<IPropertySymbol>())
            {
                if (!HasAttribute(property, MemberAttribute))
                {
                    continue;
                }

                INamedTypeSymbol? nested = GetNestedContractType(property.Type);
                if (nested is not null &&
                    !TryValidateDtoGraph(nested, ancestors, depth + 1, out reason))
                {
                    return false;
                }
            }
        }
        finally
        {
            ancestors.Remove(type);
        }

        reason = string.Empty;
        return true;
    }

    private static INamedTypeSymbol? GetNestedContractType(ITypeSymbol type)
    {
        if (type is IArrayTypeSymbol { Rank: 1 } array)
        {
            type = array.ElementType;
        }

        if (type is INamedTypeSymbol nullable &&
            nullable.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T)
        {
            type = nullable.TypeArguments[0];
        }

        return type is INamedTypeSymbol named && HasAttribute(named, ContractAttribute)
            ? named
            : null;
    }

    private static ContractInfo? Analyze(INamedTypeSymbol type, SourceProductionContext production)
    {
        AttributeData? attribute = GetAttribute(type, ContractAttribute);
        if (type.DeclaredAccessibility != Accessibility.Public ||
            type.ContainingType is not null ||
            type.TypeParameters.Length != 0 ||
            type.TypeKind != TypeKind.Class ||
            type.IsAbstract ||
            attribute is null ||
            attribute.ConstructorArguments.Length != 1 ||
            attribute.ConstructorArguments[0].Value is not int version ||
            version < 1)
        {
            production.ReportDiagnostic(Diagnostic.Create(InvalidContract, Location(type), type.Name,
                "must be a public, non-abstract, non-generic top-level class with a positive version"));
            return null;
        }

        bool rejectUnknown = GetNamedBoolean(attribute, "RejectUnknownMembers", true);
        if (type.TypeKind == TypeKind.Class &&
            !type.InstanceConstructors.Any(constructor => constructor.DeclaredAccessibility == Accessibility.Public &&
                                                         constructor.Parameters.Length == 0))
        {
            production.ReportDiagnostic(Diagnostic.Create(InvalidContract, Location(type), type.Name,
                "must have a public parameterless constructor"));
            return null;
        }

        var members = new List<MemberInfo>();
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (IPropertySymbol property in type.GetMembers().OfType<IPropertySymbol>())
        {
            AttributeData? memberAttribute = GetAttribute(property, MemberAttribute);
            if (memberAttribute is null)
            {
                continue;
            }

            if (property.IsStatic || property.IsIndexer || property.IsRequired || property.SetMethod is null ||
                property.SetMethod.IsInitOnly ||
                property.SetMethod.DeclaredAccessibility != Accessibility.Public ||
                property.GetMethod is null || property.GetMethod.DeclaredAccessibility != Accessibility.Public)
            {
                production.ReportDiagnostic(Diagnostic.Create(UnsupportedMember, Location(property), property.Name,
                    "must have public instance getter and non-init setter and cannot be required or an indexer"));
                continue;
            }

            string wireName = memberAttribute.ConstructorArguments.Length == 1 &&
                              memberAttribute.ConstructorArguments[0].Value is string explicitName &&
                              !string.IsNullOrWhiteSpace(explicitName)
                ? explicitName
                : property.Name;
            int maxString = GetNamedInt(memberAttribute, "MaximumStringLength", 4096);
            int maxCollection = GetNamedInt(memberAttribute, "MaximumCollectionCount", 64);
            if (string.Equals(wireName, "$version", StringComparison.OrdinalIgnoreCase) ||
                wireName.Length > 128 || wireName.IndexOf('\0') >= 0 ||
                !names.Add(wireName) || maxString < 0 || maxString > 64 * 1024 ||
                maxCollection < 0 || maxCollection > 64)
            {
                production.ReportDiagnostic(Diagnostic.Create(UnsupportedMember, Location(property), property.Name,
                    "has an invalid or duplicate wire name or bound"));
                continue;
            }

            ProjectionKind kind = GetProjectionKind(property.Type);
            if (kind == ProjectionKind.Unsupported)
            {
                string requirement = IsFlagsEnum(property.Type)
                    ? "cannot use a [Flags] enum"
                    : "must use a supported scalar, annotated nested DTO, or one-dimensional array of either";
                production.ReportDiagnostic(Diagnostic.Create(UnsupportedMember, Location(property), property.Name,
                    requirement));
                continue;
            }

            members.Add(new MemberInfo(
                property.Name,
                wireName,
                property.Type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
                kind,
                GetNamedBoolean(memberAttribute, "Required", true),
                maxString,
                maxCollection));
        }

        if (members.Count == 0)
        {
            production.ReportDiagnostic(Diagnostic.Create(InvalidContract, Location(type), type.Name,
                "must declare at least one [PowerShellDtoMember] property"));
            return null;
        }

        if (members.Count > 63)
        {
            production.ReportDiagnostic(Diagnostic.Create(InvalidContract, Location(type), type.Name,
                "must declare no more than 63 [PowerShellDtoMember] properties because $version uses one of the 64 property bag entries"));
            return null;
        }

        return new ContractInfo(
            type.ContainingNamespace.IsGlobalNamespace ? string.Empty : type.ContainingNamespace.ToDisplayString(),
            type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
            type.Name,
            version,
            rejectUnknown,
            members);
    }

    private static string Emit(ContractInfo contract)
    {
        var source = new StringBuilder();
        source.AppendLine("// <auto-generated/>");
        source.AppendLine("#nullable enable");
        if (!string.IsNullOrEmpty(contract.Namespace))
        {
            source.Append("namespace ").Append(contract.Namespace).AppendLine(";");
            source.AppendLine();
        }

        source.Append("public static class ").Append(contract.Name).AppendLine("PowerShellDtoProjection");
        source.AppendLine("{");
        source.Append("    private static readonly global::System.Collections.Generic.IReadOnlySet<string> DeclaredMembers = new global::System.Collections.Generic.HashSet<string>(global::System.StringComparer.OrdinalIgnoreCase) { ");
        foreach (MemberInfo member in contract.Members)
        {
            source.Append(Literal(member.WireName)).Append(", ");
        }
        source.AppendLine("};");
        source.AppendLine();
        source.Append("    public static bool TryRead(global::Devolutions.PowerShell.Ffi.PowerShellValue value, out ")
            .Append(contract.TypeName)
            .Append("? result, out global::Devolutions.PowerShell.Ffi.PowerShellDtoProjectionError? error)");
        source.AppendLine();
        source.AppendLine("    {");
        source.Append("        if (!global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.TryGetPropertyBag(value, ")
            .Append(contract.Version.ToString(CultureInfo.InvariantCulture)).Append(", ")
            .Append(contract.RejectUnknown ? "true" : "false")
            .AppendLine(", DeclaredMembers, string.Empty, out var properties, out error))");
        source.AppendLine("        { result = default; return false; }");
        source.Append("        var dto = new ").Append(contract.TypeName).AppendLine("();");
        foreach (MemberInfo member in contract.Members)
        {
            EmitReadMember(source, member);
        }
        source.AppendLine("        result = dto;");
        source.AppendLine("        error = null;");
        source.AppendLine("        return true;");
        source.AppendLine("    }");
        source.AppendLine();
        source.Append("    public static ").Append(contract.TypeName).AppendLine(" Read(global::Devolutions.PowerShell.Ffi.PowerShellValue value)");
        source.AppendLine("    {");
        source.AppendLine("        if (!TryRead(value, out var result, out var error)) throw global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreateException(error!);");
        source.AppendLine("        return result!;");
        source.AppendLine("    }");
        source.AppendLine();
        source.Append("    public static global::Devolutions.PowerShell.Ffi.PowerShellValue Write(").Append(contract.TypeName).AppendLine(" value)");
        source.AppendLine("    {");
        source.AppendLine("        global::System.ArgumentNullException.ThrowIfNull(value);");
        foreach (MemberInfo member in contract.Members)
        {
            if (member.Kind.IsString)
            {
                source.Append("        if (value.@").Append(member.PropertyName);
                if (member.Kind.IsNullable)
                {
                    source.Append(" is not null && value.@").Append(member.PropertyName);
                }
                source.Append(".Length > ")
                    .Append(member.MaximumStringLength.ToString(CultureInfo.InvariantCulture))
                    .AppendLine(") throw global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreateException(global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.ValueTooLarge(" + Literal(member.WireName) + ", \"The DTO string member exceeds its declared bound.\"));");
            }
            else if (member.Kind.IsArray)
            {
                source.Append("        if (value.@").Append(member.PropertyName).Append(".Length > ")
                    .Append(member.MaximumCollectionCount.ToString(CultureInfo.InvariantCulture))
                    .AppendLine(") throw global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreateException(global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.ValueTooLarge(" + Literal(member.WireName) + ", \"The DTO array exceeds its declared bound.\"));");
                if (member.Kind.Element!.IsString)
                {
                    source.Append("        if (global::System.Linq.Enumerable.Any(value.@").Append(member.PropertyName)
                        .Append(", static item => ");
                    if (member.Kind.Element.IsNullable)
                    {
                        source.Append("item is not null && ");
                    }
                    source.Append("item.Length > ")
                        .Append(member.MaximumStringLength.ToString(CultureInfo.InvariantCulture))
                        .AppendLine(")) throw global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreateException(global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.ValueTooLarge(" + Literal(member.WireName) + ", \"A DTO string array member contains an item that exceeds its declared bound.\"));");
                }
            }
        }
        source.AppendLine("        return global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreatePropertyBag(" + contract.Version.ToString(CultureInfo.InvariantCulture) + ", new global::System.Collections.Generic.KeyValuePair<string, global::Devolutions.PowerShell.Ffi.PowerShellValue>[]");
        source.AppendLine("        {");
        foreach (MemberInfo member in contract.Members)
        {
            source.Append("            new(").Append(Literal(member.WireName)).Append(", ")
                .Append(WriteExpression("value.@" + member.PropertyName, member)).AppendLine("),");
        }
        source.AppendLine("        });");
        source.AppendLine("    }");
        foreach (MemberInfo member in contract.Members)
        {
            ProjectionKind nested = member.Kind.IsArray ? member.Kind.Element! : member.Kind;
            if (nested.IsDto)
            {
                EmitNestedWriter(source, member, nested);
            }
        }
        source.AppendLine("}");
        return source.ToString();
    }

    private static void EmitReadMember(StringBuilder source, MemberInfo member)
    {
        string path = "global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.JoinPath(string.Empty, " + Literal(member.WireName) + ")";
        source.Append("        if (!global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.TryGetMember(properties!, ")
            .Append(Literal(member.WireName)).Append(", ").Append(member.Required ? "true" : "false")
            .Append(", string.Empty, out var ").Append(member.PropertyName).Append("Value, out error))")
            .AppendLine(" { result = default; return false; }");
        source.Append("        if (").Append(member.PropertyName).Append("Value is not null)").AppendLine();
        source.AppendLine("        {");
        if (member.Kind.IsNullable && !member.Kind.IsArray)
        {
            source.Append("            if (").Append(member.PropertyName).AppendLine("Value.IsNull)");
            source.AppendLine("            {");
            source.Append("                dto.@").Append(member.PropertyName).AppendLine(" = null;");
            source.AppendLine("            }");
            source.AppendLine("            else");
            source.AppendLine("            {");
        }
        if (member.Kind.IsArray)
        {
            string element = member.Kind.Element!.DeclaredTypeName;
            source.Append("            if (!global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.TryReadArray(")
                .Append(member.PropertyName).Append("Value, ").Append(member.MaximumCollectionCount.ToString(CultureInfo.InvariantCulture))
                .Append(", ").Append(path).Append(", out var values, out error)) { result = default; return false; }").AppendLine();
            source.Append("            var converted = new ").Append(element).Append("[values!.Count];").AppendLine();
            source.AppendLine("            for (var index = 0; index < values.Count; index++)");
            source.AppendLine("            {");
            EmitScalarRead(source, member.Kind.Element!, "values[index]", "converted[index]", path, member.MaximumStringLength, "                ");
            source.AppendLine("            }");
            source.Append("            dto.@").Append(member.PropertyName).AppendLine(" = converted;");
        }
        else
        {
            EmitScalarRead(source, member.Kind, member.PropertyName + "Value", "dto.@" + member.PropertyName, path, member.MaximumStringLength, "            ");
        }
        if (member.Kind.IsNullable && !member.Kind.IsArray)
        {
            source.AppendLine("            }");
        }
        source.AppendLine("        }");
    }

    private static void EmitScalarRead(StringBuilder source, ProjectionKind kind, string input, string output, string path, int maximumStringLength, string indent)
    {
        if (kind.IsString)
        {
            source.Append(indent).Append("if (!global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.TryReadString(")
                .Append(input).Append(", ").Append(maximumStringLength.ToString(CultureInfo.InvariantCulture))
                .Append(", ").Append(path).Append(", out var scalar, out error)) { result = default; return false; }").AppendLine();
            source.Append(indent).Append(output).AppendLine(" = scalar!;");
            return;
        }

        if (kind.IsDto)
        {
            source.Append(indent).Append("if (!").Append(kind.DtoProjectionTypeName).Append(".TryRead(")
                .Append(input).AppendLine(", out var scalar, out var nestedError))");
            source.Append(indent).AppendLine("{");
            source.Append(indent).Append("    string nestedPath = nestedError is null || global::System.String.IsNullOrEmpty(nestedError.Path) ? ")
                .Append(path)
                .Append(" : global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.JoinPath(")
                .Append(path)
                .AppendLine(", nestedError.Path);");
            source.Append(indent).AppendLine("    error = nestedError?.Failure == global::Devolutions.PowerShell.Ffi.PowerShellDtoProjectionFailure.ValueTooLarge");
            source.Append(indent).AppendLine("        ? global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.ValueTooLarge(nestedPath, nestedError.Message)");
            source.Append(indent).AppendLine("        : global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.InvalidValue(nestedPath, nestedError?.Message ?? \"The nested DTO value is invalid.\");");
            source.Append(indent).AppendLine("    result = default;");
            source.Append(indent).AppendLine("    return false;");
            source.Append(indent).AppendLine("}");
            source.Append(indent).Append(output).AppendLine(" = scalar!;");
            return;
        }

        string method = kind.Name == "Boolean" ? "TryGetBoolean"
            : kind.Name == "SignedInteger" ? "TryGetSignedInteger"
            : kind.Name == "UnsignedInteger" ? "TryGetUnsignedInteger"
            : kind.Name == "Double" ? "TryGetDouble"
            : kind.Name == "Decimal" ? "TryGetDecimal"
            : kind.Name == "DateTime" ? "TryGetDateTime"
            : kind.Name == "DateTimeOffset" ? "TryGetDateTimeOffset"
            : kind.Name == "TimeSpan" ? "TryGetTimeSpan"
            : kind.Name == "Guid" ? "TryGetGuid"
            : kind.Name == "Uri" ? "TryGetUri"
            : throw new InvalidOperationException();
        source.Append(indent).Append("if (!").Append(input).Append(".").Append(method)
            .Append("(out var scalar)) { error = global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.InvalidValue(")
            .Append(path).Append(", \"The DTO member has an invalid tagged value kind.\"); result = default; return false; }").AppendLine();

        if (kind.IsEnum)
        {
            source.Append(indent).AppendLine("try");
            source.Append(indent).AppendLine("{");
            source.Append(indent).Append("    var converted = (").Append(kind.ValueTypeName).Append(")checked((")
                .Append(kind.ConversionTypeName).AppendLine(")scalar);");
            source.Append(indent).Append("    if (!global::System.Enum.IsDefined(typeof(").Append(kind.ValueTypeName)
                .AppendLine("), converted))");
            source.Append(indent).AppendLine("    {");
            source.Append(indent).Append("        error = global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.InvalidValue(")
                .Append(path).AppendLine(", \"The DTO enum member has an undefined value.\");");
            source.Append(indent).AppendLine("        result = default;");
            source.Append(indent).AppendLine("        return false;");
            source.Append(indent).AppendLine("    }");
            source.Append(indent).Append("    ").Append(output).AppendLine(" = converted;");
            source.Append(indent).AppendLine("}");
            source.Append(indent).AppendLine("catch (global::System.OverflowException)");
            source.Append(indent).AppendLine("{");
            EmitInvalidRange(source, path, indent + "    ");
            source.Append(indent).AppendLine("}");
            return;
        }

        if (kind.RequiresCheckedConversion)
        {
            source.Append(indent).AppendLine("try");
            source.Append(indent).AppendLine("{");
            source.Append(indent).Append("    ").Append(output).Append(" = checked((")
                .Append(kind.ValueTypeName).AppendLine(")scalar);");
            source.Append(indent).AppendLine("}");
            source.Append(indent).AppendLine("catch (global::System.OverflowException)");
            source.Append(indent).AppendLine("{");
            EmitInvalidRange(source, path, indent + "    ");
            source.Append(indent).AppendLine("}");
            return;
        }

        source.Append(indent).Append(output).AppendLine(" = scalar!;");
    }

    private static void EmitInvalidRange(StringBuilder source, string path, string indent)
    {
        source.Append(indent).Append("error = global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.InvalidValue(")
            .Append(path).AppendLine(", \"The DTO integer member is outside its declared range.\");");
        source.Append(indent).AppendLine("result = default;");
        source.Append(indent).AppendLine("return false;");
    }

    private static string WriteExpression(string value, MemberInfo member)
    {
        if (member.Kind.IsArray)
        {
            string itemExpression = member.Kind.Element!.IsDto
                ? NestedWriterName(member) + "(item)"
                : WriteScalarExpression("item", member.Kind.Element);
            return "global::Devolutions.PowerShell.Ffi.PowerShellValue.Array(global::System.Linq.Enumerable.Select(" + value + ", static item => " +
                   itemExpression + "))";
        }

        return member.Kind.IsDto
            ? NestedWriterName(member) + "(" + value + ")"
            : WriteScalarExpression(value, member.Kind);
    }

    private static string WriteScalarExpression(string value, ProjectionKind kind)
    {
        if (kind.IsNullable)
        {
            string present = kind.IsValueType ? value + ".Value" : value;
            return value + " is null ? global::Devolutions.PowerShell.Ffi.PowerShellValue.Null : " +
                   WriteScalarExpression(present, kind.WithoutNullable());
        }

        string method = kind.Name == "String" ? "String"
            : kind.Name == "Boolean" ? "Boolean"
            : kind.Name == "SignedInteger" ? "SignedInteger"
            : kind.Name == "UnsignedInteger" ? "UnsignedInteger"
            : kind.Name == "Double" ? "Double"
            : kind.Name == "Decimal" ? "Decimal"
            : kind.Name == "DateTime" ? "DateTime"
            : kind.Name == "DateTimeOffset" ? "DateTimeOffset"
            : kind.Name == "TimeSpan" ? "TimeSpan"
            : kind.Name == "Guid" ? "Guid"
            : kind.Name == "Uri" ? "Uri"
            : throw new InvalidOperationException();
        string converted = kind.IsEnum
            ? "(" + (kind.IsSignedInteger ? "long" : "ulong") + ")(" + kind.ConversionTypeName + ")" + value
            : kind.RequiresCheckedConversion || kind.IsInteger
                ? "(" + (kind.IsSignedInteger ? "long" : "ulong") + ")" + value
                : value;
        return "global::Devolutions.PowerShell.Ffi.PowerShellValue." + method + "(" + converted + ")";
    }

    private static void EmitNestedWriter(StringBuilder source, MemberInfo member, ProjectionKind nested)
    {
        source.AppendLine();
        source.Append("    private static global::Devolutions.PowerShell.Ffi.PowerShellValue ")
            .Append(NestedWriterName(member)).Append("(").Append(nested.DeclaredTypeName).AppendLine(" value)");
        source.AppendLine("    {");
        source.AppendLine("        try");
        source.AppendLine("        {");
        source.Append("            return ").Append(nested.DtoProjectionTypeName).AppendLine(".Write(value);");
        source.AppendLine("        }");
        source.AppendLine("        catch (global::Devolutions.PowerShell.Ffi.PowerShellDtoProjectionException exception)");
        source.AppendLine("        {");
        source.Append("            string path = global::System.String.IsNullOrEmpty(exception.Error.Path) ? ")
            .Append(Literal(member.WireName))
            .Append(" : global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.JoinPath(")
            .Append(Literal(member.WireName)).AppendLine(", exception.Error.Path);");
        source.AppendLine("            var error = exception.Error.Failure == global::Devolutions.PowerShell.Ffi.PowerShellDtoProjectionFailure.ValueTooLarge");
        source.AppendLine("                ? global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.ValueTooLarge(path, exception.Error.Message)");
        source.AppendLine("                : global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.InvalidValue(path, exception.Error.Message);");
        source.AppendLine("            throw global::Devolutions.PowerShell.Ffi.PowerShellDtoProjection.CreateException(error);");
        source.AppendLine("        }");
        source.AppendLine("    }");
    }

    private static string NestedWriterName(MemberInfo member) => "WriteNested_" + member.PropertyName;

    private static ProjectionKind GetProjectionKind(ITypeSymbol type)
    {
        if (type is IArrayTypeSymbol { Rank: 1 } array)
        {
            ProjectionKind element = GetProjectionKind(array.ElementType);
            return element.IsArray || element == ProjectionKind.Unsupported
                ? ProjectionKind.Unsupported
                : ProjectionKind.Array(
                    element,
                    type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat));
        }

        bool isNullable = false;
        bool isValueType = type.IsValueType;
        ITypeSymbol valueType = type;
        if (type is INamedTypeSymbol nullable &&
            nullable.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T)
        {
            valueType = nullable.TypeArguments[0];
            isNullable = true;
            isValueType = true;
        }
        else if (type.IsReferenceType && type.NullableAnnotation == NullableAnnotation.Annotated)
        {
            isNullable = true;
            isValueType = false;
        }

        if (valueType is INamedTypeSymbol enumType && enumType.TypeKind == TypeKind.Enum)
        {
            if (HasAttribute(enumType, "System.FlagsAttribute") || enumType.EnumUnderlyingType is null)
            {
                return ProjectionKind.Unsupported;
            }

            bool signed = IsSignedInteger(enumType.EnumUnderlyingType.SpecialType);
            if (!signed && !IsUnsignedInteger(enumType.EnumUnderlyingType.SpecialType))
            {
                return ProjectionKind.Unsupported;
            }

            return ProjectionKind.Enum(
                valueType.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
                enumType.EnumUnderlyingType.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
                signed,
                isNullable);
        }

        if (valueType is INamedTypeSymbol dto && HasAttribute(dto, ContractAttribute))
        {
            if (isNullable)
            {
                return ProjectionKind.Unsupported;
            }

            string dtoNamespace = dto.ContainingNamespace.IsGlobalNamespace
                ? "global::"
                : "global::" + dto.ContainingNamespace.ToDisplayString() + ".";
            return ProjectionKind.Dto(
                valueType.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
                dtoNamespace + dto.Name + "PowerShellDtoProjection");
        }

        string valueTypeDisplay = valueType
            .WithNullableAnnotation(NullableAnnotation.NotAnnotated)
            .ToDisplayString();
        ProjectionKind kind = valueType.SpecialType switch
        {
            SpecialType.System_String => ProjectionKind.Scalar("String", "string", false),
            SpecialType.System_Boolean => ProjectionKind.Scalar("Boolean", "bool", true),
            SpecialType.System_SByte => ProjectionKind.Integer("sbyte", true, true),
            SpecialType.System_Int16 => ProjectionKind.Integer("short", true, true),
            SpecialType.System_Int32 => ProjectionKind.Integer("int", true, true),
            SpecialType.System_Int64 => ProjectionKind.Integer("long", true, false),
            SpecialType.System_Byte => ProjectionKind.Integer("byte", false, true),
            SpecialType.System_UInt16 => ProjectionKind.Integer("ushort", false, true),
            SpecialType.System_UInt32 => ProjectionKind.Integer("uint", false, true),
            SpecialType.System_UInt64 => ProjectionKind.Integer("ulong", false, false),
            SpecialType.System_Double => ProjectionKind.Scalar("Double", "double", true),
            SpecialType.System_Decimal => ProjectionKind.Scalar("Decimal", "decimal", true),
            _ => valueTypeDisplay switch
            {
                "System.DateTime" => ProjectionKind.Scalar("DateTime", "global::System.DateTime", true),
                "System.DateTimeOffset" => ProjectionKind.Scalar("DateTimeOffset", "global::System.DateTimeOffset", true),
                "System.TimeSpan" => ProjectionKind.Scalar("TimeSpan", "global::System.TimeSpan", true),
                "System.Guid" => ProjectionKind.Scalar("Guid", "global::System.Guid", true),
                "System.Uri" => ProjectionKind.Scalar("Uri", "global::System.Uri", false),
                _ => ProjectionKind.Unsupported,
            },
        };
        return kind == ProjectionKind.Unsupported
            ? kind
            : kind.WithNullable(isNullable, isValueType);
    }

    private static bool IsFlagsEnum(ITypeSymbol type)
    {
        if (type is INamedTypeSymbol nullable &&
            nullable.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T)
        {
            type = nullable.TypeArguments[0];
        }

        return type is INamedTypeSymbol enumType &&
            enumType.TypeKind == TypeKind.Enum &&
            HasAttribute(enumType, "System.FlagsAttribute");
    }

    private static bool IsSignedInteger(SpecialType type) =>
        type == SpecialType.System_SByte ||
        type == SpecialType.System_Int16 ||
        type == SpecialType.System_Int32 ||
        type == SpecialType.System_Int64;

    private static bool IsUnsignedInteger(SpecialType type) =>
        type == SpecialType.System_Byte ||
        type == SpecialType.System_UInt16 ||
        type == SpecialType.System_UInt32 ||
        type == SpecialType.System_UInt64;

    private static bool HasAttribute(ISymbol symbol, string metadataName) => GetAttribute(symbol, metadataName) is not null;

    private static AttributeData? GetAttribute(ISymbol symbol, string metadataName) =>
        symbol.GetAttributes().FirstOrDefault(attribute =>
            attribute.AttributeClass?.ToDisplayString() == metadataName);

    private static int GetNamedInt(AttributeData attribute, string name, int defaultValue) =>
        attribute.NamedArguments.FirstOrDefault(pair => pair.Key == name).Value.Value is int value ? value : defaultValue;

    private static bool GetNamedBoolean(AttributeData attribute, string name, bool defaultValue) =>
        attribute.NamedArguments.FirstOrDefault(pair => pair.Key == name).Value.Value is bool value ? value : defaultValue;

    private static Location Location(ISymbol symbol) =>
        symbol.Locations.FirstOrDefault() ?? Microsoft.CodeAnalysis.Location.None;

    private static string GetHintName(INamedTypeSymbol type)
    {
        var hintName = new StringBuilder();
        foreach (char character in type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat))
        {
            if ((character >= 'a' && character <= 'z') ||
                (character >= 'A' && character <= 'Z') ||
                (character >= '0' && character <= '9'))
            {
                hintName.Append(character);
            }
            else
            {
                hintName.Append('_')
                    .Append(((int)character).ToString("X4", CultureInfo.InvariantCulture))
                    .Append('_');
            }
        }

        return hintName.Append(".PowerShellDtoProjection.g.cs").ToString();
    }

    private static string Literal(string value) => Microsoft.CodeAnalysis.CSharp.SymbolDisplay.FormatLiteral(value, quote: true);

    private sealed class ContractInfo
    {
        public ContractInfo(string @namespace, string typeName, string name, int version, bool rejectUnknown, List<MemberInfo> members)
        {
            Namespace = @namespace;
            TypeName = typeName;
            Name = name;
            Version = version;
            RejectUnknown = rejectUnknown;
            Members = members;
        }

        public string Namespace { get; }
        public string TypeName { get; }
        public string Name { get; }
        public int Version { get; }
        public bool RejectUnknown { get; }
        public List<MemberInfo> Members { get; }
    }

    private sealed class MemberInfo
    {
        public MemberInfo(string propertyName, string wireName, string typeName, ProjectionKind kind, bool required, int maximumStringLength, int maximumCollectionCount)
        {
            PropertyName = propertyName;
            WireName = wireName;
            TypeName = typeName;
            Kind = kind;
            Required = required;
            MaximumStringLength = maximumStringLength;
            MaximumCollectionCount = maximumCollectionCount;
        }

        public string PropertyName { get; }
        public string WireName { get; }
        public string TypeName { get; }
        public ProjectionKind Kind { get; }
        public bool Required { get; }
        public int MaximumStringLength { get; }
        public int MaximumCollectionCount { get; }
    }

    private sealed class ProjectionKind
    {
        private ProjectionKind(
            string name,
            string declaredTypeName,
            string valueTypeName,
            bool isValueType,
            bool isNullable = false,
            bool requiresCheckedConversion = false,
            bool isEnum = false,
            string? conversionTypeName = null,
            string? dtoProjectionTypeName = null,
            ProjectionKind? element = null)
        {
            Name = name;
            DeclaredTypeName = declaredTypeName;
            ValueTypeName = valueTypeName;
            IsValueType = isValueType;
            IsNullable = isNullable;
            RequiresCheckedConversion = requiresCheckedConversion;
            IsEnum = isEnum;
            ConversionTypeName = conversionTypeName;
            DtoProjectionTypeName = dtoProjectionTypeName;
            Element = element;
        }

        public static ProjectionKind Unsupported { get; } = new(
            "Unsupported",
            string.Empty,
            string.Empty,
            false);

        public string Name { get; }
        public string DeclaredTypeName { get; }
        public string ValueTypeName { get; }
        public bool IsValueType { get; }
        public bool IsNullable { get; }
        public bool RequiresCheckedConversion { get; }
        public bool IsEnum { get; }
        public string? ConversionTypeName { get; }
        public string? DtoProjectionTypeName { get; }
        public ProjectionKind? Element { get; }
        public bool IsArray => Element is not null;
        public bool IsString => Name == "String";
        public bool IsDto => DtoProjectionTypeName is not null;
        public bool IsSignedInteger => Name == "SignedInteger";
        public bool IsInteger => IsSignedInteger || Name == "UnsignedInteger";

        public ProjectionKind WithNullable(bool isNullable, bool isValueType)
        {
            if (!isNullable)
            {
                return this;
            }

            return new ProjectionKind(
                Name,
                DeclaredTypeName + (isValueType ? "?" : string.Empty),
                ValueTypeName,
                isValueType,
                true,
                RequiresCheckedConversion,
                IsEnum,
                ConversionTypeName,
                DtoProjectionTypeName,
                Element);
        }

        public ProjectionKind WithoutNullable()
        {
            return IsNullable
                ? new ProjectionKind(
                    Name,
                    ValueTypeName,
                    ValueTypeName,
                    IsValueType,
                    false,
                    RequiresCheckedConversion,
                    IsEnum,
                    ConversionTypeName,
                    DtoProjectionTypeName,
                    Element)
                : this;
        }

        public static ProjectionKind Scalar(string name, string typeName, bool isValueType) =>
            new(name, typeName, typeName, isValueType);

        public static ProjectionKind Integer(string typeName, bool signed, bool requiresCheckedConversion) =>
            new(
                signed ? "SignedInteger" : "UnsignedInteger",
                typeName,
                typeName,
                true,
                requiresCheckedConversion: requiresCheckedConversion);

        public static ProjectionKind Enum(
            string typeName,
            string underlyingTypeName,
            bool signed,
            bool isNullable) =>
            new(
                signed ? "SignedInteger" : "UnsignedInteger",
                typeName + (isNullable ? "?" : string.Empty),
                typeName,
                true,
                isNullable,
                true,
                true,
                underlyingTypeName);

        public static ProjectionKind Dto(string typeName, string projectionTypeName) =>
            new(
                "Dto",
                typeName,
                typeName,
                false,
                dtoProjectionTypeName: projectionTypeName);

        public static ProjectionKind Array(ProjectionKind element, string typeName) =>
            new("Array", typeName, typeName, false, element: element);
    }
}
