using System;
using System.Buffers.Binary;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;
using System.Security;
using System.Threading;
using System.Text;
using System.Management.Automation;
using System.Management.Automation.Host;
using System.Management.Automation.Language;
using System.Management.Automation.Runspaces;
using Devolutions.PowerShell.Ffi.LiveObjects;

namespace NativeHost
{
    public static partial class Bindings
    {
        private const uint FfiBindingsAbiVersion = 1;
        private const uint FfiCallResultTruncatedDiagnostic = 1;
        private const int FfiStatusSuccess = 0;
        private const int FfiStatusInvalidArgument = -1;
        private const int FfiStatusInvalidHandle = -4;
        private const int FfiStatusManagedFailure = -6;
        private const int FfiStatusInputNotCompleted = -8;
        private const int FfiStatusBackpressure = -9;
        private const int FfiStatusUnsupportedValue = -10;
        private const int FfiStatusOperationCancelled = -11;
        private const int FfiMaxStreamRecords = 32;
        private const int FfiMaxStreamFieldLength = 4096;
        private const int FfiStreamCount = 7;
        private const int FfiMaxValuePayloadLength = 64 * 1024;
        private const int FfiMaxValueContainerEntries = 64;
        private const int FfiMaxValueDepth = 8;
        private const int FfiMaxInputValues = 64;
        private const int FfiMaxInputPayloadLength = 64 * 1024;
        private const int FfiMaxSnapshotPropertyEntries = 16;
        private const int FfiMaxSnapshotPropertyNameLength = 128;
        private const int FfiMaxSnapshotScalarPayloadLength = 1024;
        private const int FfiMaxSnapshotPropertyBagPayloadLength = 16 * 1024;
        private const uint FfiResultTerminatingFailure = 1;
        private const uint FfiResultSequenceTruncated = 2;
        private const uint FfiStreamTruncated = 1;
        private const uint FfiRecordFieldsTruncated = 1;
        private const uint FfiRecordScalarValuePresent = 1 << 1;
        private const uint FfiRecordPropertyBagPresent = 1 << 2;
        private const uint FfiRecordPropertyBagTruncated = 1 << 3;
        private const uint FfiRecordTypeNamesTruncated = 1 << 4;
        private const uint FfiRecordErrorTargetValuePresent = 1 << 5;
        private const ulong FfiFeatureAsyncOperationPrimitives = 1UL << 8;
        private const ulong FfiFeatureSessionPrimitives = 1UL << 10;
        private const ulong FfiFeatureSessionPolling = 1UL << 11;
        private const ulong FfiFeatureSnapshotProjections = 1UL << 13;
        private const ulong FfiFeatureSessionConfiguration = 1UL << 14;
        private const ulong FfiFeatureSessionVariables = 1UL << 15;
        private const ulong FfiFeatureCapabilityRpc = 1UL << 16;
        private const ulong FfiFeatureLiveObjectProbe = 1UL << 17;
        private const ulong FfiFeatureLiveSessionObjectProbe = 1UL << 18;
        private const ulong FfiFeatureLiveObjectContracts = 1UL << 19;
        private const ulong FfiFeatureLiveStreamPolling = 1UL << 20;
        private const ulong FfiFeatureTypedResultPaging = 1UL << 21;
        private const ulong FfiFeatureObservedInvocation = 1UL << 22;
        private const ulong FfiFeatureSessionPreflight = 1UL << 23;
        private const ulong FfiFeatureRuntimeDiagnostics = 1UL << 24;
        private const ulong FfiFeatureDuplexBrokerChannel = 1UL << 25;
        private const ulong FfiFeatureGeneratedBridgeAttachment = 1UL << 26;
        private const ulong FfiFeatureReliableBridgeEvents = 1UL << 28;
        private const ulong FfiFeatureObservedPresentation = 1UL << 29;
        private const ulong FfiFeatureSecretAdapters = 1UL << 30;
        private const ulong FfiFeatureCredentialResult = 1UL << 31;
        private const ulong FfiFeatureRemoteProviderDiagnostics = 1UL << 32;
        private const int FfiMaxSecretLength = 4_096;
        private const int FfiMaxSecretUserNameLength = 256;
        private const int FfiMaxCredentialTextUtf8Bytes = 4 * 1024;
        private const int FfiMaxCredentialMessages = 16;
        private const int FfiMaxCredentialMessageUtf8Bytes = 4 * 1024;
        private const uint FfiTypedResultPageTerminal = 1;
        private const uint FfiTypedResultPageTruncated = 1 << 1;
        private const uint FfiTypedResultPageComplete = 1 << 2;
        private const int FfiMaxSessionEvents = 32;

        [StructLayout(LayoutKind.Sequential)]
        public unsafe struct FfiCallResult
        {
            public uint Size;
            public int Status;
            public uint Flags;
            public byte* Diagnostic;
            public int DiagnosticCapacity;
            public int DiagnosticRequiredLength;
            public int DiagnosticWrittenLength;
        }

        [StructLayout(LayoutKind.Sequential)]
        public unsafe struct FfiCredentialResult
        {
            public uint Size;
            public uint IsCancelled;
            public byte* Username;
            public int UsernameCapacity;
            public int UsernameLength;
            public byte* Domain;
            public int DomainCapacity;
            public int DomainLength;
            public char* Password;
            public int PasswordCapacity;
            public int PasswordLength;
            public byte* OutputMessages;
            public int OutputMessagesCapacity;
            public int OutputMessagesLength;
            public byte* ErrorMessages;
            public int ErrorMessagesCapacity;
            public int ErrorMessagesLength;
            public byte* LogMessage;
            public int LogMessageCapacity;
            public int LogMessageLength;
        }

        private static bool TryGetConfigurationStrings(object[] values, out string[] result)
        {
            if (values.Length > FfiMaxSessionEvents)
            {
                result = Array.Empty<string>();
                return false;
            }

            result = new string[values.Length];
            var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            for (int index = 0; index < values.Length; index++)
            {
                if (values[index] is not string value ||
                    string.IsNullOrWhiteSpace(value) ||
                    value.IndexOf('\0') >= 0 ||
                    value.Length > 4096 ||
                    !seen.Add(value))
                {
                    result = Array.Empty<string>();
                    return false;
                }
                result[index] = value;
            }

            return true;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct FfiApiV1
        {
            public nuint Size;
            public uint AbiVersion;
            public ulong FeatureFlags;
            public IntPtr PowerShell_Create;
            public IntPtr PowerShell_Release;
            public IntPtr PowerShell_AddArgumentUtf8;
            public IntPtr PowerShell_AddParameterStringUtf8;
            public IntPtr PowerShell_AddParameterInt64;
            public IntPtr PowerShell_AddCommandUtf8;
            public IntPtr PowerShell_AddScriptUtf8;
            public IntPtr PowerShell_AddStatement;
            public IntPtr PowerShell_InvokeToUtf8;
            public IntPtr PowerShell_GetInvocationErrorCount;
            public IntPtr PowerShell_CopyInvocationErrorFieldToUtf8;
            public IntPtr PowerShell_Clear;
            public IntPtr PowerShell_Stop;
            public IntPtr PowerShell_InvokeToResult;
            public IntPtr InvocationResult_Release;
            public IntPtr InvocationResult_GetInfo;
            public IntPtr InvocationResult_GetStreamInfo;
            public IntPtr InvocationResult_GetStreamRecordInfo;
            public IntPtr InvocationResult_CopyStreamRecordFieldToUtf8;
            public IntPtr InvocationResult_GetSequenceRecord;
            public IntPtr PowerShell_AddCommandUtf8Local;
            public IntPtr PowerShell_AddScriptUtf8Local;
            public IntPtr PowerShell_AddArgumentValue;
            public IntPtr PowerShell_AddParameterValue;
            public IntPtr PowerShell_AddParameterSwitch;
            public IntPtr PowerShell_AddInputValue;
            public IntPtr PowerShell_CompleteInput;
            public IntPtr PowerShell_ResetInput;
            public IntPtr InvocationResult_GetMetadata;
            public IntPtr PowerShellSession_Create;
            public IntPtr PowerShellSession_Release;
            public IntPtr PowerShellSession_CreateBuilder;
            public IntPtr PowerShellSession_GetSnapshot;
            public IntPtr PowerShellSession_GetEventInfo;
            public IntPtr InvocationResult_GetStreamTotals;
            public IntPtr InvocationResult_GetStreamRecordProjectionInfo;
            public IntPtr InvocationResult_CopyStreamRecordValue;
            public IntPtr PowerShellSession_CreateConfigured;
            public IntPtr PowerShellSession_SetVariable;
            public IntPtr PowerShellSession_RemoveVariable;
            public IntPtr PowerShellSession_GetVariableSnapshot;
            public IntPtr PowerShell_SetCapabilityContext;
            public IntPtr LiveObjectProbe_Create;
            public IntPtr LiveObjectProbe_Release;
            public IntPtr LiveObjectProbe_Unregister;
            public IntPtr PowerShell_AddArgumentLiveObject;
            public IntPtr PowerShellSession_SetLiveObjectVariable;
            public IntPtr LiveObjectContractPack_Register;
            public IntPtr PowerShellSession_SetLiveObjectContractVariable;
            public IntPtr LiveObjectContractPack_RegisterMany;
            public IntPtr PowerShell_BeginLiveInvocation;
            public IntPtr LiveInvocation_Poll;
            public IntPtr LiveInvocation_ReadBatch;
            public IntPtr LiveInvocationBatch_GetInfo;
            public IntPtr LiveInvocationBatch_GetRecordInfo;
            public IntPtr LiveInvocationBatch_CopyRecordTextToUtf8;
            public IntPtr LiveInvocationBatch_Release;
            public IntPtr LiveInvocation_Complete;
            public IntPtr LiveInvocation_Stop;
            public IntPtr LiveInvocation_Release;
            public IntPtr PowerShell_BeginTypedResultInvocation;
            public IntPtr TypedResultInvocation_Poll;
            public IntPtr TypedResultInvocation_ReadPage;
            public IntPtr TypedResultInvocation_Complete;
            public IntPtr TypedResultInvocation_Stop;
            public IntPtr TypedResultInvocation_Release;
            public IntPtr TypedResultPage_GetInfo;
            public IntPtr TypedResultPage_GetRecordInfo;
            public IntPtr TypedResultPage_CopyRecordValue;
            public IntPtr TypedResultPage_Release;
            public IntPtr PowerShell_BeginObservedInvocation;
            public IntPtr ObservedInvocation_Poll;
            public IntPtr ObservedInvocation_ReadResultPage;
            public IntPtr ObservedInvocation_ReadDiagnosticPage;
            public IntPtr ObservedInvocation_Complete;
            public IntPtr ObservedInvocation_Stop;
            public IntPtr ObservedInvocation_Release;
            public IntPtr ObservedDiagnosticPage_GetInfo;
            public IntPtr ObservedDiagnosticPage_GetRecordInfo;
            public IntPtr ObservedDiagnosticPage_CopyRecordTextToUtf8;
            public IntPtr ObservedDiagnosticPage_Release;
            public IntPtr PowerShellSession_PreflightConfigured;
            public IntPtr RuntimeDiagnostics_CopyPowerShellFileVersionUtf8;
            public IntPtr PowerShell_SetBrokerContext;
            public IntPtr PowerShell_SetBridgeContext;
            public IntPtr ObservedDiagnosticPage_CopyRecordValue;
            public IntPtr PowerShell_InvokeSecretResult;
            public IntPtr PowerShell_InvokeCredentialResult;
            public IntPtr RuntimeDiagnostics_GetDevolutionsWsManProviderInfo;
        }

        private const int FfiPreflightMaximumTextLength = 128;
        private const int FfiPreflightMaximumPathLength = 256;
        private const int FfiPreflightMaximumDeclaredCommands = 4;
        private const int FfiPreflightMaximumDeclaredCommandLength = 64;
        private const int FfiPreflightMaximumVersionLength = 128;
        private const int FfiPreflightMaximumManifestBytes = 64 * 1024;

        private const uint FfiPreflightValid = 0;
        private const uint FfiPreflightInvalidConfiguration = 1;
        private const uint FfiPreflightInvalidModuleRoots = 2;
        private const uint FfiPreflightUnresolvableModuleImports = 3;
        private const uint FfiPreflightInvalidModuleManifest = 4;
        private const uint FfiPreflightExternalModuleDeclarations = 5;
        private const uint FfiPreflightInvalidWorkingDirectory = 6;

        private const uint FfiModuleRootValid = 0;
        private const uint FfiModuleRootMissing = 1;
        private const uint FfiModuleRootInvalid = 2;

        private const uint FfiModuleImportResolved = 0;
        private const uint FfiModuleImportUnresolvable = 1;
        private const uint FfiModuleImportManifestInvalid = 2;
        private const uint FfiModuleImportManifestUnreadable = 3;
        private const uint FfiModuleImportManifestDeclarationsUnavailable = 4;
        private const uint FfiModuleImportManifestDeclaresExternalPath = 5;

        // Manifest keys whose values name files that PowerShell loads while importing the
        // module. Path-like entries must resolve beneath the same approved module root as
        // the manifest, otherwise importing the module would load code the application never
        // approved. Name-only entries are resolved by PowerShell from its module path and are
        // deliberately left alone.
        private static readonly string[] FfiModuleLoadManifestKeys =
        {
            "RootModule",
            "ModuleToProcess",
            "NestedModules",
            "RequiredModules",
            "RequiredAssemblies",
            "ScriptsToProcess",
            "TypesToProcess",
            "FormatsToProcess",
        };

        private static readonly string[] FfiModuleLoadExtensions =
        {
            ".psd1",
            ".psm1",
            ".ps1",
            ".dll",
            ".exe",
            ".ps1xml",
            ".cdxml",
        };

        private sealed class FfiModuleRootResolution
        {
            public FfiModuleRootResolution(string path, string canonicalPath, uint status, string diagnostic)
            {
                Path = path;
                CanonicalPath = canonicalPath;
                Status = status;
                Diagnostic = diagnostic;
            }

            public string Path { get; }

            public string CanonicalPath { get; }

            public uint Status { get; }

            public string Diagnostic { get; }
        }

        private sealed class FfiModuleImportResolution
        {
            public FfiModuleImportResolution(
                string moduleImport,
                string resolvedPath,
                string manifestPath,
                uint status,
                string declaredVersion,
                string[] declaredCommands,
                bool declaredCommandsTruncated,
                string diagnostic)
            {
                ModuleImport = moduleImport;
                ResolvedPath = resolvedPath;
                ManifestPath = manifestPath;
                Status = status;
                DeclaredVersion = declaredVersion;
                DeclaredCommands = declaredCommands;
                DeclaredCommandsTruncated = declaredCommandsTruncated;
                Diagnostic = diagnostic;
            }

            public string ModuleImport { get; }

            public string ResolvedPath { get; }

            public string ManifestPath { get; }

            public uint Status { get; }

            public string DeclaredVersion { get; }

            public string[] DeclaredCommands { get; }

            public bool DeclaredCommandsTruncated { get; }

            public string Diagnostic { get; }
        }

        private sealed class FfiSessionPreflightPayload
        {
            public FfiSessionPreflightPayload(
                uint status,
                string diagnostic,
                FfiModuleRootResolution[] moduleRoots,
                FfiModuleImportResolution[] moduleImports)
            {
                Status = status;
                Diagnostic = diagnostic;
                ModuleRoots = moduleRoots;
                ModuleImports = moduleImports;
            }

            public uint Status { get; }

            public string Diagnostic { get; }

            public FfiModuleRootResolution[] ModuleRoots { get; }

            public FfiModuleImportResolution[] ModuleImports { get; }
        }

        private sealed class FfiManifestDeclaration
        {
            public FfiManifestDeclaration(
                uint status,
                string version,
                string[] commands,
                bool commandsTruncated,
                string diagnostic)
            {
                Status = status;
                Version = version;
                Commands = commands;
                CommandsTruncated = commandsTruncated;
                Diagnostic = diagnostic;
            }

            public uint Status { get; }

            public string Version { get; }

            public string[] Commands { get; }

            public bool CommandsTruncated { get; }

            public string Diagnostic { get; }
        }

        private static string[] NormalizeDirectories(string[] directories, string description)
        {
            FfiModuleRootResolution[] resolutions = ResolveModuleRoots(directories, description);
            if (resolutions.Any(resolution => resolution.Status != FfiModuleRootValid))
            {
                throw new InvalidOperationException($"{description} must name unique existing directories.");
            }

            return resolutions.Select(static resolution => resolution.CanonicalPath).ToArray();
        }

        private static FfiModuleRootResolution[] ResolveModuleRoots(string[] directories, string description)
        {
            if (directories is null || directories.Length > FfiMaxSessionEvents)
            {
                throw new InvalidOperationException($"{description} count exceeds its bound.");
            }

            var resolutions = new FfiModuleRootResolution[directories.Length];
            var unique = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            for (int index = 0; index < directories.Length; index++)
            {
                string directory = directories[index];
                if (string.IsNullOrWhiteSpace(directory) || !Path.IsPathFullyQualified(directory))
                {
                    resolutions[index] = new FfiModuleRootResolution(
                        directory ?? string.Empty,
                        string.Empty,
                        FfiModuleRootInvalid,
                        "Module root must be an absolute directory.");
                    continue;
                }

                try
                {
                    string fullPath = Path.GetFullPath(directory);
                    if (!Directory.Exists(fullPath))
                    {
                        resolutions[index] = new FfiModuleRootResolution(
                            directory,
                            string.Empty,
                            FfiModuleRootMissing,
                            "Module root does not exist.");
                        continue;
                    }

                    string canonicalPath = CanonicalizeExistingPath(fullPath, isDirectory: true);
                    if (!unique.Add(canonicalPath))
                    {
                        resolutions[index] = new FfiModuleRootResolution(
                            directory,
                            canonicalPath,
                            FfiModuleRootInvalid,
                            "Module roots must be unique after canonicalization.");
                        continue;
                    }

                    resolutions[index] = new FfiModuleRootResolution(
                        directory,
                        canonicalPath,
                        FfiModuleRootValid,
                        string.Empty);
                }
                catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException)
                {
                    resolutions[index] = new FfiModuleRootResolution(
                        directory,
                        string.Empty,
                        FfiModuleRootInvalid,
                        "Module root cannot be canonicalized.");
                }
            }

            return resolutions;
        }

        private static string ResolveModuleImport(string[] allowedModulePaths, string moduleImport)
        {
            FfiModuleRootResolution[] roots = allowedModulePaths
                .Select(path => new FfiModuleRootResolution(path, path, FfiModuleRootValid, string.Empty))
                .ToArray();
            FfiModuleImportResolution resolution = ResolveModuleImport(roots, moduleImport);
            if (resolution.Status != FfiModuleImportResolved &&
                resolution.Status != FfiModuleImportManifestDeclarationsUnavailable)
            {
                throw new InvalidOperationException(resolution.Diagnostic.Length != 0
                    ? $"An approved module import could not be resolved beneath an approved module path. {resolution.Diagnostic}"
                    : "An approved module import could not be resolved beneath an approved module path.");
            }

            return resolution.ResolvedPath;
        }

        private static FfiModuleImportResolution ResolveModuleImport(
            FfiModuleRootResolution[] allowedModulePaths,
            string moduleImport)
        {
            if (string.IsNullOrWhiteSpace(moduleImport) ||
                moduleImport.Length > 128 ||
                !moduleImport.All(character => char.IsLetterOrDigit(character) || character is '.' or '_' or '-'))
            {
                return new FfiModuleImportResolution(
                    moduleImport ?? string.Empty,
                    string.Empty,
                    string.Empty,
                    FfiModuleImportUnresolvable,
                    string.Empty,
                    Array.Empty<string>(),
                    false,
                    "Module import name is invalid.");
            }

            foreach (FfiModuleRootResolution root in allowedModulePaths)
            {
                if (root.Status != FfiModuleRootValid)
                {
                    continue;
                }

                foreach (string candidate in new[]
                {
                    Path.Combine(root.CanonicalPath, moduleImport, $"{moduleImport}.psd1"),
                    Path.Combine(root.CanonicalPath, moduleImport, $"{moduleImport}.psm1"),
                    Path.Combine(root.CanonicalPath, $"{moduleImport}.psd1"),
                    Path.Combine(root.CanonicalPath, $"{moduleImport}.psm1"),
                    Path.Combine(root.CanonicalPath, $"{moduleImport}.dll"),
                })
                {
                    if (!File.Exists(candidate))
                    {
                        continue;
                    }

                    string canonicalCandidate;
                    try
                    {
                        canonicalCandidate = CanonicalizeExistingPath(candidate, isDirectory: false);
                    }
                    catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException)
                    {
                        continue;
                    }

                    if (!IsBeneathRoot(root.CanonicalPath, canonicalCandidate))
                    {
                        continue;
                    }

                    return DescribeModuleImport(moduleImport, canonicalCandidate, root.CanonicalPath);
                }
            }

            return new FfiModuleImportResolution(
                moduleImport,
                string.Empty,
                string.Empty,
                FfiModuleImportUnresolvable,
                string.Empty,
                Array.Empty<string>(),
                false,
                "Module import could not be resolved beneath an approved module root.");
        }

        private static FfiModuleImportResolution DescribeModuleImport(
            string moduleImport,
            string resolvedPath,
            string root)
        {
            string manifestPath = string.Equals(Path.GetExtension(resolvedPath), ".psd1", StringComparison.OrdinalIgnoreCase)
                ? resolvedPath
                : Path.ChangeExtension(resolvedPath, ".psd1");
            if (!File.Exists(manifestPath))
            {
                return new FfiModuleImportResolution(
                    moduleImport,
                    resolvedPath,
                    string.Empty,
                    FfiModuleImportResolved,
                    string.Empty,
                    Array.Empty<string>(),
                    false,
                    string.Empty);
            }

            try
            {
                manifestPath = CanonicalizeExistingPath(manifestPath, isDirectory: false);
                if (!IsBeneathRoot(root, manifestPath))
                {
                    return new FfiModuleImportResolution(
                        moduleImport,
                        resolvedPath,
                        string.Empty,
                        FfiModuleImportManifestInvalid,
                        string.Empty,
                        Array.Empty<string>(),
                        false,
                        "Module manifest resolves outside its approved module root.");
                }
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException)
            {
                return new FfiModuleImportResolution(
                    moduleImport,
                    resolvedPath,
                    string.Empty,
                    FfiModuleImportManifestUnreadable,
                    string.Empty,
                    Array.Empty<string>(),
                    false,
                    "Module manifest cannot be read.");
            }

            FfiManifestDeclaration declaration = ReadManifestDeclaration(manifestPath, root);
            return new FfiModuleImportResolution(
                moduleImport,
                resolvedPath,
                manifestPath,
                declaration.Status,
                declaration.Version,
                declaration.Commands,
                declaration.CommandsTruncated,
                declaration.Diagnostic);
        }

        private static FfiManifestDeclaration ReadManifestDeclaration(string manifestPath, string root)
        {
            try
            {
                FileInfo info = new FileInfo(manifestPath);
                if (info.Length > FfiPreflightMaximumManifestBytes)
                {
                    return InvalidManifest("Module manifest exceeds the preflight size bound.");
                }

                string source = File.ReadAllText(manifestPath);
                Token[] tokens;
                ParseError[] errors;
                ScriptBlockAst script = Parser.ParseInput(source, out tokens, out errors);
                if (errors.Length != 0 || !TryGetManifestHashtable(script, out HashtableAst manifest))
                {
                    return InvalidManifest("Module manifest is not a valid static data file.");
                }

                string version = string.Empty;
                var commands = new List<string>(FfiPreflightMaximumDeclaredCommands);
                bool commandsTruncated = false;
                bool declarationsUnavailable = false;
                string manifestDirectory = Path.GetDirectoryName(manifestPath) ?? string.Empty;
                string externalDeclaration = string.Empty;
                foreach (Tuple<ExpressionAst, StatementAst> pair in manifest.KeyValuePairs)
                {
                    if (pair.Item1 is not StringConstantExpressionAst key)
                    {
                        continue;
                    }

                    if (string.Equals(key.Value, "ModuleVersion", StringComparison.OrdinalIgnoreCase))
                    {
                        if (!TryGetManifestString(pair.Item2, out string declaredVersion))
                        {
                            declarationsUnavailable = true;
                            continue;
                        }

                        version = BoundPreflightText(declaredVersion, FfiPreflightMaximumVersionLength);
                        continue;
                    }

                    if (IsModuleLoadManifestKey(key.Value))
                    {
                        if (!TryGetManifestLoadPaths(pair.Item2, out string[] declaredPaths))
                        {
                            return InvalidManifest(
                                $"Module manifest has a non-static '{key.Value}' module-loading declaration.");
                        }

                        foreach (string declaredPath in declaredPaths)
                        {
                            if (externalDeclaration.Length != 0 ||
                                !IsPathLikeModuleDeclaration(declaredPath) ||
                                DeclarationResolvesBeneathRoot(manifestDirectory, root, declaredPath))
                            {
                                continue;
                            }

                            externalDeclaration = BoundPreflightText(declaredPath, FfiPreflightMaximumDeclaredCommandLength);
                        }

                        continue;
                    }

                    if (!string.Equals(key.Value, "FunctionsToExport", StringComparison.OrdinalIgnoreCase) &&
                        !string.Equals(key.Value, "CmdletsToExport", StringComparison.OrdinalIgnoreCase))
                    {
                        continue;
                    }

                    if (!TryGetManifestStringArray(pair.Item2, out string[] declaredCommands))
                    {
                        declarationsUnavailable = true;
                        continue;
                    }

                    foreach (string command in declaredCommands)
                    {
                        if (commands.Count == FfiPreflightMaximumDeclaredCommands)
                        {
                            commandsTruncated = true;
                            continue;
                        }

                        commands.Add(BoundPreflightText(command, FfiPreflightMaximumDeclaredCommandLength));
                    }
                }

                return externalDeclaration.Length != 0
                    ? new FfiManifestDeclaration(
                        FfiModuleImportManifestDeclaresExternalPath,
                        version,
                        commands.ToArray(),
                        commandsTruncated,
                        $"Module manifest loads '{externalDeclaration}' from outside its approved module root.")
                    : new FfiManifestDeclaration(
                        declarationsUnavailable ? FfiModuleImportManifestDeclarationsUnavailable : FfiModuleImportResolved,
                        version,
                        commands.ToArray(),
                        commandsTruncated,
                        declarationsUnavailable ? "Module manifest declarations are not entirely static." : string.Empty);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException)
            {
                return new FfiManifestDeclaration(
                    FfiModuleImportManifestUnreadable,
                    string.Empty,
                    Array.Empty<string>(),
                    false,
                    "Module manifest cannot be read.");
            }
        }

        private static FfiManifestDeclaration InvalidManifest(string diagnostic)
        {
            return new FfiManifestDeclaration(
                FfiModuleImportManifestInvalid,
                string.Empty,
                Array.Empty<string>(),
                false,
                diagnostic);
        }

        private static bool TryGetManifestHashtable(ScriptBlockAst script, out HashtableAst manifest)
        {
            manifest = null;
            if (script.EndBlock.Statements.Count != 1 ||
                script.EndBlock.Statements[0] is not PipelineAst pipeline ||
                pipeline.PipelineElements.Count != 1 ||
                pipeline.PipelineElements[0] is not CommandExpressionAst command ||
                command.Expression is not HashtableAst hashtable)
            {
                return false;
            }

            manifest = hashtable;
            return true;
        }

        private static bool TryGetManifestString(StatementAst statement, out string value)
        {
            value = string.Empty;
            return TryGetManifestExpression(statement, out ExpressionAst expression) &&
                expression is StringConstantExpressionAst stringExpression &&
                (value = stringExpression.Value) is not null;
        }

        private static bool TryGetManifestStringArray(StatementAst statement, out string[] values)
        {
            values = Array.Empty<string>();
            if (!TryGetManifestExpression(statement, out ExpressionAst expression))
            {
                return false;
            }

            if (expression is StringConstantExpressionAst stringExpression)
            {
                values = [stringExpression.Value];
                return true;
            }

            if (expression is ArrayExpressionAst arrayExpression)
            {
                if (arrayExpression.SubExpression.Statements.Count == 0)
                {
                    return true;
                }

                if (arrayExpression.SubExpression.Statements.Count != 1 ||
                    arrayExpression.SubExpression.Statements[0] is not PipelineAst arrayPipeline ||
                    arrayPipeline.PipelineElements.Count != 1 ||
                    arrayPipeline.PipelineElements[0] is not CommandExpressionAst arrayCommand)
                {
                    return false;
                }

                // A single-element array literal such as @('Get-Thing') parses as the element
                // expression itself rather than as an ArrayLiteralAst.
                if (arrayCommand.Expression is StringConstantExpressionAst singleElement)
                {
                    values = [singleElement.Value];
                    return true;
                }

                if (arrayCommand.Expression is not ArrayLiteralAst nestedArray)
                {
                    return false;
                }

                expression = nestedArray;
            }

            if (expression is not ArrayLiteralAst array)
            {
                return false;
            }

            var result = new List<string>();
            foreach (ExpressionAst element in array.Elements)
            {
                if (element is not StringConstantExpressionAst stringElement)
                {
                    return false;
                }

                result.Add(stringElement.Value);
            }

            values = result.ToArray();
            return true;
        }

        private static bool TryGetManifestExpression(StatementAst statement, out ExpressionAst expression)
        {
            expression = null;
            if (statement is not PipelineAst pipeline ||
                pipeline.PipelineElements.Count != 1 ||
                pipeline.PipelineElements[0] is not CommandExpressionAst command)
            {
                return false;
            }

            expression = command.Expression;
            return true;
        }

        private static bool IsModuleLoadManifestKey(string key)
        {
            return FfiModuleLoadManifestKeys.Any(candidate => string.Equals(candidate, key, StringComparison.OrdinalIgnoreCase));
        }

        private static bool IsPathLikeModuleDeclaration(string value)
        {
            if (string.IsNullOrWhiteSpace(value))
            {
                return false;
            }

            if (value.IndexOf(Path.DirectorySeparatorChar) >= 0 ||
                value.IndexOf(Path.AltDirectorySeparatorChar) >= 0 ||
                Path.IsPathRooted(value))
            {
                return true;
            }

            string extension = Path.GetExtension(value);
            return extension.Length != 0 &&
                FfiModuleLoadExtensions.Any(candidate => string.Equals(candidate, extension, StringComparison.OrdinalIgnoreCase));
        }

        private static bool DeclarationResolvesBeneathRoot(string manifestDirectory, string root, string declaredPath)
        {
            if (root.Length == 0)
            {
                return false;
            }

            try
            {
                string resolvedPath = Path.IsPathRooted(declaredPath)
                    ? Path.GetFullPath(declaredPath)
                    : Path.GetFullPath(Path.Combine(manifestDirectory, declaredPath));
                if (File.Exists(resolvedPath))
                {
                    resolvedPath = CanonicalizeExistingPath(resolvedPath, isDirectory: false);
                }

                return IsBeneathRoot(root, resolvedPath);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException)
            {
                return false;
            }
        }

        /// <summary>
        /// Reads statically declared module-loading entries. Non-string elements such as the
        /// module specification hashtables accepted by RequiredModules name a module rather
        /// than a path, so they are skipped instead of failing the whole declaration.
        /// </summary>
        private static bool TryGetManifestLoadPaths(StatementAst statement, out string[] values)
        {
            values = Array.Empty<string>();
            if (!TryGetManifestExpression(statement, out ExpressionAst expression))
            {
                return false;
            }

            if (expression is StringConstantExpressionAst stringExpression)
            {
                values = [stringExpression.Value];
                return true;
            }

            if (expression is ArrayExpressionAst arrayExpression)
            {
                if (arrayExpression.SubExpression.Statements.Count == 0)
                {
                    return true;
                }

                if (arrayExpression.SubExpression.Statements.Count != 1 ||
                    arrayExpression.SubExpression.Statements[0] is not PipelineAst arrayPipeline ||
                    arrayPipeline.PipelineElements.Count != 1 ||
                    arrayPipeline.PipelineElements[0] is not CommandExpressionAst arrayCommand)
                {
                    return false;
                }

                if (arrayCommand.Expression is StringConstantExpressionAst singleElement)
                {
                    values = [singleElement.Value];
                    return true;
                }

                if (arrayCommand.Expression is HashtableAst)
                {
                    return true;
                }

                if (arrayCommand.Expression is not ArrayLiteralAst nestedArray)
                {
                    return false;
                }

                expression = nestedArray;
            }

            if (expression is not ArrayLiteralAst array)
            {
                return false;
            }

            var result = new List<string>();
            foreach (ExpressionAst element in array.Elements)
            {
                if (element is HashtableAst)
                {
                    continue;
                }

                if (element is not StringConstantExpressionAst stringElement)
                {
                    return false;
                }

                result.Add(stringElement.Value);
            }

            values = result.ToArray();
            return true;
        }

        private static string CanonicalizeExistingPath(string path, bool isDirectory)
        {
            string fullPath = Path.GetFullPath(path);
            if (isDirectory)
            {
                fullPath = Path.TrimEndingDirectorySeparator(fullPath);
            }

            string root = Path.GetPathRoot(fullPath)
                ?? throw new ArgumentException("The path has no filesystem root.", nameof(path));
            string canonicalPath = root;
            foreach (string component in fullPath[root.Length..].Split(
                [Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar],
                StringSplitOptions.RemoveEmptyEntries))
            {
                canonicalPath = Path.Combine(canonicalPath, component);
                FileSystemInfo info = Directory.Exists(canonicalPath)
                    ? new DirectoryInfo(canonicalPath)
                    : new FileInfo(canonicalPath);
                if (!info.Exists)
                {
                    throw new IOException("The path does not exist.");
                }

                // Resolve every existing component, not just the final entry. A regular
                // module file beneath a junction can otherwise pass a lexical root check
                // while its actual storage lies outside the approved root.
                FileSystemInfo target = info.ResolveLinkTarget(returnFinalTarget: true);
                if (target is not null)
                {
                    canonicalPath = Path.GetFullPath(target.FullName);
                }
            }

            if (!isDirectory)
            {
                return canonicalPath;
            }

            while (canonicalPath.EndsWith($"{Path.DirectorySeparatorChar}.", StringComparison.Ordinal) ||
                canonicalPath.EndsWith($"{Path.AltDirectorySeparatorChar}.", StringComparison.Ordinal))
            {
                canonicalPath = canonicalPath.Substring(0, canonicalPath.Length - 2) + Path.DirectorySeparatorChar;
            }

            return Path.TrimEndingDirectorySeparator(canonicalPath);
        }

        private static bool IsBeneathRoot(string root, string path)
        {
            try
            {
                string relative = Path.GetRelativePath(root, path);
                return relative.Length != 0 &&
                    !Path.IsPathFullyQualified(relative) &&
                    relative != ".." &&
                    !relative.StartsWith($"..{Path.DirectorySeparatorChar}", StringComparison.Ordinal);
            }
            catch (ArgumentException)
            {
                return false;
            }
        }

        private static string BoundPreflightText(string value, int maximumLength)
        {
            value ??= string.Empty;
            if (value.Length <= maximumLength)
            {
                return value;
            }

            int length = maximumLength;
            if (length != 0 &&
                char.IsHighSurrogate(value[length - 1]) &&
                length < value.Length &&
                char.IsLowSurrogate(value[length]))
            {
                length--;
            }

            return value.Substring(0, length);
        }

        private static class FfiPreflightValueEncoder
        {
            public static byte[] Encode(FfiSessionPreflightPayload report)
            {
                ArgumentNullException.ThrowIfNull(report);
                object[] roots = report.ModuleRoots.Select(static root => CreatePropertyBag(
                    ("Path", BoundPreflightText(root.Path, FfiPreflightMaximumPathLength)),
                    ("CanonicalPath", BoundPreflightText(root.CanonicalPath, FfiPreflightMaximumPathLength)),
                    ("Status", root.Status),
                    ("Diagnostic", BoundPreflightText(root.Diagnostic, FfiPreflightMaximumTextLength)))).Cast<object>().ToArray();
                object[] imports = report.ModuleImports.Select(static import => CreatePropertyBag(
                    ("ModuleImport", BoundPreflightText(import.ModuleImport, FfiPreflightMaximumTextLength)),
                    ("ResolvedPath", BoundPreflightText(import.ResolvedPath, FfiPreflightMaximumPathLength)),
                    ("ManifestPath", BoundPreflightText(import.ManifestPath, FfiPreflightMaximumPathLength)),
                    ("Status", import.Status),
                    ("DeclaredVersion", BoundPreflightText(import.DeclaredVersion, FfiPreflightMaximumVersionLength)),
                    ("DeclaredCommands", import.DeclaredCommands
                        .Select(static command => (object)BoundPreflightText(command, FfiPreflightMaximumDeclaredCommandLength))
                        .ToArray()),
                    ("DeclaredCommandsTruncated", import.DeclaredCommandsTruncated),
                    ("Diagnostic", BoundPreflightText(import.Diagnostic, FfiPreflightMaximumTextLength)))).Cast<object>().ToArray();
                PSObject value = CreatePropertyBag(
                    ("Status", report.Status),
                    ("Diagnostic", BoundPreflightText(report.Diagnostic, FfiPreflightMaximumTextLength)),
                    ("ModuleRoots", roots),
                    ("ModuleImports", imports));
                if (!FfiSnapshotCollector.TryEncodeCopiedValue(value, depth: 0, out FfiSnapshotValue encoded) ||
                    encoded.Kind != (uint)FfiValueKind.PropertyBag ||
                    encoded.Payload.Length > FfiMaxValuePayloadLength)
                {
                    throw new InvalidOperationException("PowerShell session preflight report exceeds its copied-value bounds.");
                }

                return encoded.Payload;
            }

            private static PSObject CreatePropertyBag(params (string Name, object Value)[] properties)
            {
                var propertyBag = new PSObject();
                foreach ((string name, object value) in properties)
                {
                    propertyBag.Properties.Add(new PSNoteProperty(name, value));
                }

                return propertyBag;
            }
        }

        private static readonly object FfiApiV1Lock = new object();
        private static readonly ConcurrentDictionary<IntPtr, InvocationResult> FfiInvocationResults =
            new ConcurrentDictionary<IntPtr, InvocationResult>();
        private static readonly ConcurrentDictionary<IntPtr, FfiInputBuffer> FfiInputBuffers =
            new ConcurrentDictionary<IntPtr, FfiInputBuffer>();
        private static readonly ConcurrentDictionary<IntPtr, FfiLiveObjectProbeEntry> FfiLiveObjectProbes =
            new ConcurrentDictionary<IntPtr, FfiLiveObjectProbeEntry>();
        private static readonly StrategyBasedComWrappers FfiLiveObjectComWrappers = new();
        private static readonly PowerShellLiveObjectContract FfiLiveObjectProbeContract = new(
            typeof(IPowerShellLiveObjectProbe).GUID,
            majorVersion: 1,
            minorVersion: 0,
            PowerShellLiveObjectDirection.ConsumerToSession);
        private static readonly FfiLiveObjectContractPackRegistry FfiLiveObjectContracts = new(
            FfiLiveObjectProbeContract,
            static pointer => new FfiManagedLiveObjectLease(FfiLiveSessionObjectProbeProxy.Create(pointer)));
        private static long FfiNextInvocationId;
        private static IntPtr FfiApiV1Ptr = IntPtr.Zero;

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_GetFfiApiV1()
        {
            try
            {
                lock (FfiApiV1Lock)
                {
                    if (FfiApiV1Ptr == IntPtr.Zero)
                    {
                        FfiApiV1 api = CreateFfiApiV1();
                        FfiApiV1Ptr = Marshal.AllocCoTaskMem(Marshal.SizeOf<FfiApiV1>());
                        Marshal.StructureToPtr(api, FfiApiV1Ptr, false);
                    }

                    return FfiApiV1Ptr;
                }
            }
            catch
            {
                return IntPtr.Zero;
            }
        }

        private static unsafe FfiApiV1 CreateFfiApiV1()
        {
            return new FfiApiV1
            {
                Size = (nuint)Marshal.SizeOf<FfiApiV1>(),
                AbiVersion = FfiBindingsAbiVersion,
                FeatureFlags = (1UL << 4) | (1UL << 5) | (1UL << 6) | FfiFeatureAsyncOperationPrimitives |
                    FfiFeatureSessionPrimitives | FfiFeatureSessionPolling | FfiFeatureSnapshotProjections |
                    FfiFeatureSessionConfiguration | FfiFeatureSessionVariables | FfiFeatureCapabilityRpc |
                    FfiFeatureLiveObjectProbe | FfiFeatureLiveSessionObjectProbe | FfiFeatureLiveObjectContracts |
                    FfiFeatureLiveStreamPolling | FfiFeatureTypedResultPaging | FfiFeatureObservedInvocation |
                    FfiFeatureSessionPreflight | FfiFeatureRuntimeDiagnostics | FfiFeatureDuplexBrokerChannel |
                    FfiFeatureGeneratedBridgeAttachment | FfiFeatureReliableBridgeEvents |
                    FfiFeatureObservedPresentation | FfiFeatureSecretAdapters | FfiFeatureCredentialResult |
                    FfiFeatureRemoteProviderDiagnostics,
                PowerShell_Create = (IntPtr)(delegate* unmanaged<IntPtr*, FfiCallResult*, int>)&FfiPowerShell_Create,
                PowerShell_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_Release,
                PowerShell_AddArgumentUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddArgumentUtf8,
                PowerShell_AddParameterStringUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddParameterStringUtf8,
                PowerShell_AddParameterInt64 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, long, FfiCallResult*, int>)&FfiPowerShell_AddParameterInt64,
                PowerShell_AddCommandUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddCommandUtf8,
                PowerShell_AddScriptUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddScriptUtf8,
                PowerShell_AddStatement = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_AddStatement,
                PowerShell_InvokeToUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, int*, FfiCallResult*, int>)&FfiPowerShell_InvokeToUtf8,
                PowerShell_GetInvocationErrorCount = (IntPtr)(delegate* unmanaged<IntPtr, int*, FfiCallResult*, int>)&FfiPowerShell_GetInvocationErrorCount,
                PowerShell_CopyInvocationErrorFieldToUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, int, int, byte*, int, int*, FfiCallResult*, int>)&FfiPowerShell_CopyInvocationErrorFieldToUtf8,
                PowerShell_Clear = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_Clear,
                PowerShell_Stop = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_Stop,
                PowerShell_InvokeToResult = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr*, FfiCallResult*, int>)&FfiPowerShell_InvokeToResult,
                InvocationResult_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiInvocationResult_Release,
                InvocationResult_GetInfo = (IntPtr)(delegate* unmanaged<IntPtr, uint*, int*, FfiCallResult*, int>)&FfiInvocationResult_GetInfo,
                InvocationResult_GetStreamInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, int*, uint*, FfiCallResult*, int>)&FfiInvocationResult_GetStreamInfo,
                InvocationResult_GetStreamRecordInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, int, long*, uint*, FfiCallResult*, int>)&FfiInvocationResult_GetStreamRecordInfo,
                InvocationResult_CopyStreamRecordFieldToUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, int, int, int, byte*, int, int*, FfiCallResult*, int>)&FfiInvocationResult_CopyStreamRecordFieldToUtf8,
                InvocationResult_GetSequenceRecord = (IntPtr)(delegate* unmanaged<IntPtr, int, int*, int*, long*, FfiCallResult*, int>)&FfiInvocationResult_GetSequenceRecord,
                PowerShell_AddCommandUtf8Local = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, int, FfiCallResult*, int>)&FfiPowerShell_AddCommandUtf8Local,
                PowerShell_AddScriptUtf8Local = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, int, FfiCallResult*, int>)&FfiPowerShell_AddScriptUtf8Local,
                PowerShell_AddArgumentValue = (IntPtr)(delegate* unmanaged<IntPtr, uint, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddArgumentValue,
                PowerShell_AddParameterValue = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, uint, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddParameterValue,
                PowerShell_AddParameterSwitch = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddParameterSwitch,
                PowerShell_AddInputValue = (IntPtr)(delegate* unmanaged<IntPtr, uint, byte*, int, FfiCallResult*, int>)&FfiPowerShell_AddInputValue,
                PowerShell_CompleteInput = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_CompleteInput,
                PowerShell_ResetInput = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShell_ResetInput,
                InvocationResult_GetMetadata = (IntPtr)(delegate* unmanaged<IntPtr, uint*, long*, int*, FfiCallResult*, int>)&FfiInvocationResult_GetMetadata,
                PowerShellSession_Create = (IntPtr)(delegate* unmanaged<uint, uint, uint, uint, uint, uint, uint, uint, byte*, int, IntPtr*, FfiCallResult*, int>)&FfiPowerShellSession_Create,
                PowerShellSession_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiPowerShellSession_Release,
                PowerShellSession_CreateBuilder = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr*, FfiCallResult*, int>)&FfiPowerShellSession_CreateBuilder,
                PowerShellSession_GetSnapshot = (IntPtr)(delegate* unmanaged<IntPtr, uint*, uint*, uint*, uint*, uint*, long*, long*, FfiCallResult*, int>)&FfiPowerShellSession_GetSnapshot,
                PowerShellSession_GetEventInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, long*, uint*, uint*, FfiCallResult*, int>)&FfiPowerShellSession_GetEventInfo,
                InvocationResult_GetStreamTotals = (IntPtr)(delegate* unmanaged<IntPtr, int, long*, long*, FfiCallResult*, int>)&FfiInvocationResult_GetStreamTotals,
                InvocationResult_GetStreamRecordProjectionInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, int, int*, int*, int*, int*, int*, FfiCallResult*, int>)&FfiInvocationResult_GetStreamRecordProjectionInfo,
                InvocationResult_CopyStreamRecordValue = (IntPtr)(delegate* unmanaged<IntPtr, int, int, int, uint*, byte*, int, int*, FfiCallResult*, int>)&FfiInvocationResult_CopyStreamRecordValue,
                PowerShellSession_CreateConfigured = (IntPtr)(delegate* unmanaged<uint, uint, uint, uint, uint, uint, uint, uint, uint, byte*, int, byte*, int, byte*, int, byte*, int, byte*, int, IntPtr*, FfiCallResult*, int>)&FfiPowerShellSession_CreateConfigured,
                PowerShellSession_SetVariable = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, uint, byte*, int, FfiCallResult*, int>)&FfiPowerShellSession_SetVariable,
                PowerShellSession_RemoveVariable = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, uint*, FfiCallResult*, int>)&FfiPowerShellSession_RemoveVariable,
                PowerShellSession_GetVariableSnapshot = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, uint*, uint*, byte*, int, int*, FfiCallResult*, int>)&FfiPowerShellSession_GetVariableSnapshot,
                PowerShell_SetCapabilityContext = (IntPtr)(delegate* unmanaged<IntPtr, ulong, ulong, IntPtr, FfiCallResult*, int>)&FfiPowerShell_SetCapabilityContext,
                LiveObjectProbe_Create = (IntPtr)(delegate* unmanaged<long, IntPtr*, FfiCallResult*, int>)&FfiLiveObjectProbe_Create,
                LiveObjectProbe_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveObjectProbe_Release,
                LiveObjectProbe_Unregister = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveObjectProbe_Unregister,
                PowerShell_AddArgumentLiveObject = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, FfiCallResult*, int>)&FfiPowerShell_AddArgumentLiveObject,
                PowerShellSession_SetLiveObjectVariable = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, IntPtr, FfiCallResult*, int>)&FfiPowerShellSession_SetLiveObjectVariable,
                LiveObjectContractPack_Register = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveObjectContractPack_Register,
                PowerShellSession_SetLiveObjectContractVariable = (IntPtr)(delegate* unmanaged<IntPtr, byte*, int, NativeLiveObjectContractDescriptor*, IntPtr, FfiCallResult*, int>)&FfiPowerShellSession_SetLiveObjectContractVariable,
                LiveObjectContractPack_RegisterMany = (IntPtr)(delegate* unmanaged<IntPtr*, uint, FfiCallResult*, int>)&FfiLiveObjectContractPack_RegisterMany,
                PowerShell_BeginLiveInvocation = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr*, FfiCallResult*, int>)&FfiPowerShell_BeginLiveInvocation,
                LiveInvocation_Poll = (IntPtr)(delegate* unmanaged<IntPtr, int*, FfiCallResult*, int>)&FfiLiveInvocation_Poll,
                LiveInvocation_ReadBatch = (IntPtr)(delegate* unmanaged<IntPtr, long, int, IntPtr*, FfiCallResult*, int>)&FfiLiveInvocation_ReadBatch,
                LiveInvocationBatch_GetInfo = (IntPtr)(delegate* unmanaged<IntPtr, long*, long*, long*, int*, FfiCallResult*, int>)&FfiLiveInvocationBatch_GetInfo,
                LiveInvocationBatch_GetRecordInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, int*, long*, uint*, FfiCallResult*, int>)&FfiLiveInvocationBatch_GetRecordInfo,
                LiveInvocationBatch_CopyRecordTextToUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, int, byte*, int, int*, FfiCallResult*, int>)&FfiLiveInvocationBatch_CopyRecordTextToUtf8,
                LiveInvocationBatch_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveInvocationBatch_Release,
                LiveInvocation_Complete = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr*, FfiCallResult*, int>)&FfiLiveInvocation_Complete,
                LiveInvocation_Stop = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveInvocation_Stop,
                LiveInvocation_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiLiveInvocation_Release,
                PowerShell_BeginTypedResultInvocation = (IntPtr)(delegate* unmanaged<IntPtr, int, int, IntPtr*, FfiCallResult*, int>)&FfiPowerShell_BeginTypedResultInvocation,
                TypedResultInvocation_Poll = (IntPtr)(delegate* unmanaged<IntPtr, int*, FfiCallResult*, int>)&FfiTypedResultInvocation_Poll,
                TypedResultInvocation_ReadPage = (IntPtr)(delegate* unmanaged<IntPtr, long, int, IntPtr*, FfiCallResult*, int>)&FfiTypedResultInvocation_ReadPage,
                TypedResultInvocation_Complete = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiTypedResultInvocation_Complete,
                TypedResultInvocation_Stop = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiTypedResultInvocation_Stop,
                TypedResultInvocation_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiTypedResultInvocation_Release,
                TypedResultPage_GetInfo = (IntPtr)(delegate* unmanaged<IntPtr, long*, long*, long*, long*, int*, uint*, int*, FfiCallResult*, int>)&FfiTypedResultPage_GetInfo,
                TypedResultPage_GetRecordInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, long*, uint*, FfiCallResult*, int>)&FfiTypedResultPage_GetRecordInfo,
                TypedResultPage_CopyRecordValue = (IntPtr)(delegate* unmanaged<IntPtr, int, uint*, byte*, int, int*, FfiCallResult*, int>)&FfiTypedResultPage_CopyRecordValue,
                TypedResultPage_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiTypedResultPage_Release,
                PowerShell_BeginObservedInvocation = (IntPtr)(delegate* unmanaged<IntPtr, int, int, int, int, IntPtr*, FfiCallResult*, int>)&FfiPowerShell_BeginObservedInvocation,
                ObservedInvocation_Poll = (IntPtr)(delegate* unmanaged<IntPtr, int*, FfiCallResult*, int>)&FfiObservedInvocation_Poll,
                ObservedInvocation_ReadResultPage = (IntPtr)(delegate* unmanaged<IntPtr, long, int, IntPtr*, FfiCallResult*, int>)&FfiObservedInvocation_ReadResultPage,
                ObservedInvocation_ReadDiagnosticPage = (IntPtr)(delegate* unmanaged<IntPtr, long, int, IntPtr*, FfiCallResult*, int>)&FfiObservedInvocation_ReadDiagnosticPage,
                ObservedInvocation_Complete = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiObservedInvocation_Complete,
                ObservedInvocation_Stop = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiObservedInvocation_Stop,
                ObservedInvocation_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiObservedInvocation_Release,
                ObservedDiagnosticPage_GetInfo = (IntPtr)(delegate* unmanaged<IntPtr, long*, long*, long*, long*, int*, uint*, int*, FfiCallResult*, int>)&FfiObservedDiagnosticPage_GetInfo,
                ObservedDiagnosticPage_GetRecordInfo = (IntPtr)(delegate* unmanaged<IntPtr, int, int*, long*, FfiCallResult*, int>)&FfiObservedDiagnosticPage_GetRecordInfo,
                ObservedDiagnosticPage_CopyRecordTextToUtf8 = (IntPtr)(delegate* unmanaged<IntPtr, int, byte*, int, int*, FfiCallResult*, int>)&FfiObservedDiagnosticPage_CopyRecordTextToUtf8,
                ObservedDiagnosticPage_Release = (IntPtr)(delegate* unmanaged<IntPtr, FfiCallResult*, int>)&FfiObservedDiagnosticPage_Release,
                PowerShellSession_PreflightConfigured = (IntPtr)(delegate* unmanaged<uint, uint, uint, uint, uint, uint, uint, uint, uint, byte*, int, byte*, int, byte*, int, byte*, int, byte*, int, byte*, int, int*, FfiCallResult*, int>)&FfiPowerShellSession_PreflightConfigured,
                RuntimeDiagnostics_CopyPowerShellFileVersionUtf8 = (IntPtr)(delegate* unmanaged<byte*, int, int*, int*, FfiCallResult*, int>)&FfiRuntimeDiagnostics_CopyPowerShellFileVersionUtf8,
                PowerShell_SetBrokerContext = (IntPtr)(delegate* unmanaged<IntPtr, ulong, ulong, IntPtr, IntPtr, uint, FfiCallResult*, int>)&FfiPowerShell_SetBrokerContext,
                PowerShell_SetBridgeContext = (IntPtr)(delegate* unmanaged<IntPtr, ulong, ulong, ulong, ushort, ushort, uint, uint, byte*, int, FfiCallResult*, int>)&FfiPowerShell_SetBridgeContext,
                ObservedDiagnosticPage_CopyRecordValue = (IntPtr)(delegate* unmanaged<IntPtr, int, uint*, byte*, int, int*, FfiCallResult*, int>)&FfiObservedDiagnosticPage_CopyRecordValue,
                PowerShell_InvokeSecretResult = (IntPtr)(delegate* unmanaged<IntPtr, uint, byte*, int, int*, char*, int, int*, FfiCallResult*, int>)&FfiPowerShell_InvokeSecretResult,
                PowerShell_InvokeCredentialResult = (IntPtr)(delegate* unmanaged<IntPtr, FfiCredentialResult*, FfiCallResult*, int>)&FfiPowerShell_InvokeCredentialResult,
                RuntimeDiagnostics_GetDevolutionsWsManProviderInfo = (IntPtr)(delegate* unmanaged<uint*, uint*, ulong*, FfiCallResult*, int>)&FfiRuntimeDiagnostics_GetDevolutionsWsManProviderInfo,
            };
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiRuntimeDiagnostics_CopyPowerShellFileVersionUtf8(
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            int* available,
            FfiCallResult* result)
        {
            if (requiredLength == null || available == null || bufferLength < 0 || (buffer == null && bufferLength != 0))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Runtime diagnostic output buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            IntPtr outputAvailable = (IntPtr)available;
            return Execute(result, () =>
            {
                string fileVersion = TryGetPowerShellFileVersion();
                Marshal.WriteInt32(outputAvailable, fileVersion.Length == 0 ? 0 : 1);
                int required = Encoding.UTF8.GetByteCount(fileVersion);
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] versionBytes = Encoding.UTF8.GetBytes(fileVersion);
                    Marshal.Copy(versionBytes, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_Create(IntPtr* ptrHandle, FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            if (ptrHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell handle output pointer is null.");
            }

            try
            {
                GCHandle handle = GCHandle.Alloc(new FfiPowerShellPipeline(PowerShell.Create(), null), GCHandleType.Normal);
                *ptrHandle = GCHandle.ToIntPtr(handle);
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusManagedFailure, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_Release(IntPtr ptrHandle, FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrHandle);
                if (!handle.IsAllocated || handle.Target is not FfiPowerShellPipeline pipeline)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "PowerShell handle is invalid.");
                }

                FfiInvocationResults.TryRemove(ptrHandle, out _);
                FfiInputBuffers.TryRemove(ptrHandle, out _);
                pipeline.Dispose();
                handle.Free();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_Create(
            uint runspaceMode,
            uint initialConfiguration,
            uint historyMode,
            uint errorPreference,
            uint warningPreference,
            uint verbosePreference,
            uint debugPreference,
            uint informationPreference,
            byte* allowedModulePath,
            int allowedModulePathLength,
            IntPtr* ptrSessionHandle,
            FfiCallResult* result)
        {
            if (ptrSessionHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session handle output pointer is null.");
            }

            int status = ReadUtf8(allowedModulePath, allowedModulePathLength, result, out string modulePath);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                var session = FfiPowerShellSession.Create(
                    runspaceMode,
                    initialConfiguration,
                    historyMode,
                    errorPreference,
                    warningPreference,
                    verbosePreference,
                    debugPreference,
                    informationPreference,
                    modulePath);
                GCHandle handle = GCHandle.Alloc(session, GCHandleType.Normal);
                *ptrSessionHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_CreateConfigured(
            uint runspaceMode,
            uint initialConfiguration,
            uint historyMode,
            uint errorPreference,
            uint warningPreference,
            uint verbosePreference,
            uint debugPreference,
            uint informationPreference,
            uint executionPolicy,
            byte* initialVariables,
            int initialVariablesLength,
            byte* moduleImports,
            int moduleImportsLength,
            byte* allowedModulePaths,
            int allowedModulePathsLength,
            byte* workingDirectory,
            int workingDirectoryLength,
            byte* environment,
            int environmentLength,
            IntPtr* ptrSessionHandle,
            FfiCallResult* result)
        {
            if (ptrSessionHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session handle output pointer is null.");
            }

            int status = ReadSessionConfiguration(
                initialVariables,
                initialVariablesLength,
                moduleImports,
                moduleImportsLength,
                allowedModulePaths,
                allowedModulePathsLength,
                workingDirectory,
                workingDirectoryLength,
                environment,
                environmentLength,
                result,
                out FfiSessionConfigurationInput configuration);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                var session = FfiPowerShellSession.CreateConfigured(
                    runspaceMode,
                    initialConfiguration,
                    historyMode,
                    errorPreference,
                    warningPreference,
                    verbosePreference,
                    debugPreference,
                    informationPreference,
                    executionPolicy,
                    configuration.InitialVariables,
                    configuration.ModuleImports,
                    configuration.AllowedModulePaths,
                    configuration.WorkingDirectory,
                    configuration.Environment);
                GCHandle handle = GCHandle.Alloc(session, GCHandleType.Normal);
                *ptrSessionHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_PreflightConfigured(
            uint runspaceMode,
            uint initialConfiguration,
            uint historyMode,
            uint errorPreference,
            uint warningPreference,
            uint verbosePreference,
            uint debugPreference,
            uint informationPreference,
            uint executionPolicy,
            byte* initialVariables,
            int initialVariablesLength,
            byte* moduleImports,
            int moduleImportsLength,
            byte* allowedModulePaths,
            int allowedModulePathsLength,
            byte* workingDirectory,
            int workingDirectoryLength,
            byte* environment,
            int environmentLength,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session preflight output arguments are invalid.");
            }

            int status = ReadSessionConfiguration(
                initialVariables,
                initialVariablesLength,
                moduleImports,
                moduleImportsLength,
                allowedModulePaths,
                allowedModulePathsLength,
                workingDirectory,
                workingDirectoryLength,
                environment,
                environmentLength,
                result,
                out FfiSessionConfigurationInput configuration);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                byte[] payload = FfiPreflightValueEncoder.Encode(FfiPowerShellSession.PreflightConfigured(
                    runspaceMode,
                    initialConfiguration,
                    historyMode,
                    errorPreference,
                    warningPreference,
                    verbosePreference,
                    debugPreference,
                    informationPreference,
                    executionPolicy,
                    configuration.InitialVariables,
                    configuration.ModuleImports,
                    configuration.AllowedModulePaths,
                    configuration.WorkingDirectory,
                    configuration.Environment));
                Marshal.WriteInt32(outputRequiredLength, payload.Length);
                if (bufferLength < payload.Length)
                {
                    throw new BufferTooSmallException();
                }

                if (payload.Length != 0)
                {
                    Marshal.Copy(payload, 0, outputBuffer, payload.Length);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        private sealed class FfiSessionConfigurationInput
        {
            public FfiSessionConfigurationInput(
                PSObject initialVariables,
                string[] moduleImports,
                string[] allowedModulePaths,
                string workingDirectory,
                PSObject environment)
            {
                InitialVariables = initialVariables;
                ModuleImports = moduleImports;
                AllowedModulePaths = allowedModulePaths;
                WorkingDirectory = workingDirectory;
                Environment = environment;
            }

            public PSObject InitialVariables { get; }

            public string[] ModuleImports { get; }

            public string[] AllowedModulePaths { get; }

            public string WorkingDirectory { get; }

            public PSObject Environment { get; }
        }

        private static unsafe int ReadSessionConfiguration(
            byte* initialVariables,
            int initialVariablesLength,
            byte* moduleImports,
            int moduleImportsLength,
            byte* allowedModulePaths,
            int allowedModulePathsLength,
            byte* workingDirectory,
            int workingDirectoryLength,
            byte* environment,
            int environmentLength,
            FfiCallResult* result,
            out FfiSessionConfigurationInput configuration)
        {
            configuration = null;
            int status = ReadValue((uint)FfiValueKind.PropertyBag, initialVariables, initialVariablesLength, result, out object initialVariablesValue);
            if (status != FfiStatusSuccess)
            {
                return status;
            }
            status = ReadValue((uint)FfiValueKind.Array, moduleImports, moduleImportsLength, result, out object moduleImportsValue);
            if (status != FfiStatusSuccess)
            {
                return status;
            }
            status = ReadValue((uint)FfiValueKind.Array, allowedModulePaths, allowedModulePathsLength, result, out object modulePathsValue);
            if (status != FfiStatusSuccess)
            {
                return status;
            }
            status = ReadUtf8(workingDirectory, workingDirectoryLength, result, out string workingDirectoryValue);
            if (status != FfiStatusSuccess)
            {
                return status;
            }
            status = ReadValue((uint)FfiValueKind.PropertyBag, environment, environmentLength, result, out object environmentValue);
            if (status != FfiStatusSuccess)
            {
                return status;
            }
            if (initialVariablesValue is not PSObject initialVariablesObject ||
                moduleImportsValue is not object[] moduleImportObjects ||
                modulePathsValue is not object[] modulePathObjects ||
                environmentValue is not PSObject environmentObject)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session configuration has an invalid tagged shape.");
            }
            if (!TryGetConfigurationStrings(moduleImportObjects, out string[] moduleImportNames) ||
                !TryGetConfigurationStrings(modulePathObjects, out string[] modulePathNames))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session module configuration must contain only strings.");
            }

            configuration = new FfiSessionConfigurationInput(
                initialVariablesObject,
                moduleImportNames,
                modulePathNames,
                workingDirectoryValue,
                environmentObject);
            return FfiStatusSuccess;
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_Release(IntPtr ptrSessionHandle, FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrSessionHandle);
                if (!handle.IsAllocated || handle.Target is not FfiPowerShellSession session)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "PowerShell session handle is invalid.");
                }

                session.ReleaseOwner();
                handle.Free();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_CreateBuilder(
            IntPtr ptrSessionHandle,
            IntPtr* ptrBuilderHandle,
            FfiCallResult* result)
        {
            if (ptrBuilderHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell builder handle output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiPowerShellSession session = GetPowerShellSession(ptrSessionHandle);
                FfiPowerShellPipeline pipeline = session.CreatePipeline();
                GCHandle handle = GCHandle.Alloc(pipeline, GCHandleType.Normal);
                *ptrBuilderHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_GetSnapshot(
            IntPtr ptrSessionHandle,
            uint* state,
            uint* runspaceState,
            uint* flags,
            uint* activePipelineCount,
            uint* eventCount,
            long* invocationCount,
            long* historyCount,
            FfiCallResult* result)
        {
            if (state == null || runspaceState == null || flags == null || activePipelineCount == null ||
                eventCount == null || invocationCount == null || historyCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session snapshot output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiSessionSnapshot snapshot = GetPowerShellSession(ptrSessionHandle).Snapshot();
                *state = snapshot.State;
                *runspaceState = snapshot.RunspaceState;
                *flags = snapshot.Flags;
                *activePipelineCount = snapshot.ActivePipelineCount;
                *eventCount = snapshot.EventCount;
                *invocationCount = snapshot.InvocationCount;
                *historyCount = snapshot.HistoryCount;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_GetEventInfo(
            IntPtr ptrSessionHandle,
            int eventIndex,
            long* sequence,
            uint* state,
            uint* flags,
            FfiCallResult* result)
        {
            if (sequence == null || state == null || flags == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "PowerShell session event output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiSessionEvent sessionEvent = GetPowerShellSession(ptrSessionHandle).GetEvent(eventIndex);
                *sequence = sessionEvent.Sequence;
                *state = sessionEvent.State;
                *flags = sessionEvent.Flags;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_SetVariable(
            IntPtr ptrSessionHandle,
            byte* name,
            int nameLength,
            uint kind,
            byte* data,
            int dataLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            status = ReadValue(kind, data, dataLength, result, out object value);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () => GetPowerShellSession(ptrSessionHandle).SetVariable(nameText, value));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_SetLiveObjectVariable(
            IntPtr ptrSessionHandle,
            byte* name,
            int nameLength,
            IntPtr ptrComObject,
            FfiCallResult* result)
        {
            if (ptrComObject == IntPtr.Zero)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live session object probe pointer is null.");
            }

            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () => GetPowerShellSession(ptrSessionHandle).SetLiveObjectVariable(nameText, ptrComObject));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveObjectContractPack_Register(
            IntPtr ptrPackApi,
            FfiCallResult* result)
        {
            return Execute(result, () => FfiLiveObjectContracts.Register(ptrPackApi));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveObjectContractPack_RegisterMany(
            IntPtr* ptrPackApis,
            uint packCount,
            FfiCallResult* result)
        {
            return Execute(result, () => FfiLiveObjectContracts.RegisterMany(ptrPackApis, packCount));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_SetLiveObjectContractVariable(
            IntPtr ptrSessionHandle,
            byte* name,
            int nameLength,
            NativeLiveObjectContractDescriptor* contract,
            IntPtr ptrComObject,
            FfiCallResult* result)
        {
            if (contract == null || ptrComObject == IntPtr.Zero)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live object contract transfer is invalid.");
            }

            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShellLiveObjectContract descriptor =
                    PowerShellLiveObjectContract.FromNative(*contract);
                GetPowerShellSession(ptrSessionHandle).SetLiveObjectVariable(
                    nameText,
                    descriptor,
                    ptrComObject);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_RemoveVariable(
            IntPtr ptrSessionHandle,
            byte* name,
            int nameLength,
            uint* removed,
            FfiCallResult* result)
        {
            if (removed == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Session variable removal output pointer is null.");
            }

            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                *removed = GetPowerShellSession(ptrSessionHandle).RemoveVariable(nameText) ? 1u : 0u;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShellSession_GetVariableSnapshot(
            IntPtr ptrSessionHandle,
            byte* name,
            int nameLength,
            uint* found,
            uint* kind,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (found == null || kind == null || requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Session variable snapshot output arguments are invalid.");
            }

            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                if (!GetPowerShellSession(ptrSessionHandle).TryGetVariableSnapshot(nameText, out FfiSnapshotValue snapshot))
                {
                    *found = 0;
                    *kind = 0;
                    Marshal.WriteInt32(outputRequiredLength, 0);
                    return;
                }

                *found = 1;
                *kind = snapshot.Kind;
                int required = snapshot.Payload.Length;
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required != 0)
                {
                    Marshal.Copy(snapshot.Payload, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddArgumentUtf8(
            IntPtr ptrHandle,
            byte* argument,
            int argumentLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(argument, argumentLength, result, out string argumentText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddArgument(argumentText);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddParameterStringUtf8(
            IntPtr ptrHandle,
            byte* name,
            int nameLength,
            byte* value,
            int valueLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            status = ReadUtf8(value, valueLength, result, out string valueText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddParameter(nameText, valueText);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddParameterInt64(
            IntPtr ptrHandle,
            byte* name,
            int nameLength,
            long value,
            FfiCallResult* result)
        {
            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddParameter(nameText, value);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddCommandUtf8(
            IntPtr ptrHandle,
            byte* command,
            int commandLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(command, commandLength, result, out string commandText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddCommand(commandText);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddScriptUtf8(
            IntPtr ptrHandle,
            byte* script,
            int scriptLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(script, scriptLength, result, out string scriptText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddScript(scriptText);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddCommandUtf8Local(
            IntPtr ptrHandle,
            byte* command,
            int commandLength,
            int useLocalScope,
            FfiCallResult* result)
        {
            if (useLocalScope is not 0 and not 1)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Local scope must be zero or one.");
            }

            int status = ReadUtf8(command, commandLength, result, out string commandText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddCommand(commandText, useLocalScope != 0);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddScriptUtf8Local(
            IntPtr ptrHandle,
            byte* script,
            int scriptLength,
            int useLocalScope,
            FfiCallResult* result)
        {
            if (useLocalScope is not 0 and not 1)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Local scope must be zero or one.");
            }

            int status = ReadUtf8(script, scriptLength, result, out string scriptText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddScript(scriptText, useLocalScope != 0);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddArgumentValue(
            IntPtr ptrHandle,
            uint kind,
            byte* data,
            int dataLength,
            FfiCallResult* result)
        {
            int status = ReadValue(kind, data, dataLength, result, out object value);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddArgument(value);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddArgumentLiveObject(
            IntPtr ptrHandle,
            IntPtr ptrComObject,
            FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddArgument(GetLiveObjectProbe(ptrComObject).Value);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveObjectProbe_Create(
            long initialCount,
            IntPtr* ptrComObject,
            FfiCallResult* result)
        {
            if (ptrComObject == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live object probe output pointer is null.");
            }

            return Execute(result, () =>
            {
                using var powerShell = PowerShell.Create();
                powerShell
                    .AddScript("param([long]$initialCount) [pscustomobject]@{ Count = $initialCount }", useLocalScope: true)
                    .AddArgument(initialCount);
                Collection<PSObject> output = powerShell.Invoke();
                if (output.Count != 1)
                {
                    throw new InvalidOperationException("Live object probe did not produce exactly one PowerShell object.");
                }

                *ptrComObject = ExportLiveObjectProbe(output[0]);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveObjectProbe_Release(IntPtr ptrComObject, FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                if (!FfiLiveObjectProbes.TryGetValue(ptrComObject, out FfiLiveObjectProbeEntry entry) ||
                    !entry.TryReleaseTransitReference())
                {
                    throw new InvalidOperationException("Live object probe transit reference is invalid.");
                }

                ReleaseComInterface(ptrComObject);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveObjectProbe_Unregister(IntPtr ptrComObject, FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                if (!FfiLiveObjectProbes.TryRemove(ptrComObject, out FfiLiveObjectProbeEntry entry) ||
                    !entry.IsTransitReferenceReleased)
                {
                    throw new InvalidOperationException("Live object probe pointer is invalid.");
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddParameterValue(
            IntPtr ptrHandle,
            byte* name,
            int nameLength,
            uint kind,
            byte* data,
            int dataLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            if (kind is (uint)FfiValueKind.SecretUtf16 or (uint)FfiValueKind.Credential)
            {
                return AddSecretParameter(ptrHandle, nameText, kind, data, dataLength, result);
            }

            status = ReadValue(kind, data, dataLength, result, out object value);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddParameter(nameText, value);
            });
        }

        private static unsafe int AddSecretParameter(
            IntPtr ptrHandle,
            string name,
            uint kind,
            byte* data,
            int dataLength,
            FfiCallResult* result)
        {
            if (dataLength is < 2 or > (sizeof(int) + FfiMaxSecretUserNameLength * 4 + FfiMaxSecretLength * sizeof(char)) ||
                data == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Secret adapter payload is invalid.");
            }

            return ExecuteSecret(result, () =>
            {
                ReadOnlySpan<byte> payload = new(data, dataLength);
                string userName = null;
                ReadOnlySpan<byte> secretPayload = payload;
                if (kind == (uint)FfiValueKind.Credential)
                {
                    if (payload.Length < sizeof(int))
                    {
                        throw new InvalidOperationException("Secret adapter payload is invalid.");
                    }

                    int userNameLength = BitConverter.ToInt32(payload[..sizeof(int)]);
                    if (userNameLength is < 1 or > FfiMaxSecretUserNameLength * 4 ||
                        payload.Length <= sizeof(int) + userNameLength)
                    {
                        throw new InvalidOperationException("Secret adapter payload is invalid.");
                    }

                    userName = Encoding.UTF8.GetString(payload.Slice(sizeof(int), userNameLength));
                    if (string.IsNullOrWhiteSpace(userName) ||
                        userName.Length > FfiMaxSecretUserNameLength ||
                        userName.IndexOf('\0') >= 0)
                    {
                        throw new InvalidOperationException("Secret adapter payload is invalid.");
                    }

                    secretPayload = payload[(sizeof(int) + userNameLength)..];
                }

                if (secretPayload.Length is < 2 or > FfiMaxSecretLength * sizeof(char) || secretPayload.Length % sizeof(char) != 0)
                {
                    throw new InvalidOperationException("Secret adapter payload is invalid.");
                }

                var secureString = new SecureString();
                try
                {
                    foreach (char character in MemoryMarshal.Cast<byte, char>(secretPayload))
                    {
                        if (character == '\0')
                        {
                            throw new InvalidOperationException("Secret adapter payload is invalid.");
                        }

                        secureString.AppendChar(character);
                    }

                    secureString.MakeReadOnly();
                    FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                    FfiInvocationResults.TryRemove(ptrHandle, out _);
                    object boundValue = kind == (uint)FfiValueKind.Credential
                        ? pipeline.CreatePayloadCredential(userName, secureString)
                        : secureString;
                    pipeline.PowerShell.AddParameter(name, boundValue);
                    pipeline.AddSecretBinding(secureString);
                    secureString = null;
                }
                finally
                {
                    secureString?.Dispose();
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddParameterSwitch(
            IntPtr ptrHandle,
            byte* name,
            int nameLength,
            FfiCallResult* result)
        {
            int status = ReadUtf8(name, nameLength, result, out string nameText);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddParameter(nameText);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddInputValue(
            IntPtr ptrHandle,
            uint kind,
            byte* data,
            int dataLength,
            FfiCallResult* result)
        {
            int status = ReadValue(kind, data, dataLength, result, out object value);
            if (status != FfiStatusSuccess)
            {
                return status;
            }

            return Execute(result, () => AddInputValue(ptrHandle, value, dataLength));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_CompleteInput(IntPtr ptrHandle, FfiCallResult* result)
        {
            return Execute(result, () => CompleteInput(ptrHandle));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_ResetInput(IntPtr ptrHandle, FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                GetPowerShell(ptrHandle);
                FfiInputBuffers.TryRemove(ptrHandle, out _);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_AddStatement(IntPtr ptrHandle, FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                ps.AddStatement();
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_InvokeToUtf8(
            IntPtr ptrHandle,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Output buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                PowerShell ps = pipeline.PowerShell;
                InvocationResult invocation = FfiInvocationResults.GetOrAdd(
                    ptrHandle,
                    _ => InvokeAndCaptureLegacy(ps, TakeCompletedInput(ptrHandle), pipeline.Session));
                if (invocation.Status != FfiStatusSuccess)
                {
                    throw new InvalidOperationException(invocation.Errors.Length == 0
                        ? "PowerShell invocation failed."
                        : invocation.Errors[0].Message);
                }

                int required = Encoding.UTF8.GetByteCount(invocation.Output);
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] output = Encoding.UTF8.GetBytes(invocation.Output);
                    Marshal.Copy(output, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_GetInvocationErrorCount(
            IntPtr ptrHandle,
            int* errorCount,
            FfiCallResult* result)
        {
            if (errorCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Error count output pointer is null.");
            }

            IntPtr errorCountPointer = (IntPtr)errorCount;
            return Execute(result, () =>
            {
                GetPowerShell(ptrHandle);
                if (!FfiInvocationResults.TryGetValue(ptrHandle, out InvocationResult invocation))
                {
                    throw new InvalidOperationException("No invocation result is available.");
                }

                Marshal.WriteInt32(errorCountPointer, invocation.Errors.Length);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_CopyInvocationErrorFieldToUtf8(
            IntPtr ptrHandle,
            int errorIndex,
            int field,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Error buffer arguments are invalid.");
            }

            IntPtr errorBuffer = (IntPtr)buffer;
            IntPtr errorRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                GetPowerShell(ptrHandle);
                if (!FfiInvocationResults.TryGetValue(ptrHandle, out InvocationResult invocation) ||
                    errorIndex < 0 ||
                    errorIndex >= invocation.Errors.Length)
                {
                    throw new InvalidOperationException("Invocation error index is invalid.");
                }

                InvocationError error = invocation.Errors[errorIndex];
                string value = field switch
                {
                    0 => error.Message,
                    1 => error.FullyQualifiedErrorId,
                    2 => error.Category,
                    3 => error.ExceptionType,
                    _ => throw new InvalidOperationException("Invocation error field is invalid."),
                };

                int required = Encoding.UTF8.GetByteCount(value);
                Marshal.WriteInt32(errorRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] valueBytes = Encoding.UTF8.GetBytes(value);
                    Marshal.Copy(valueBytes, 0, errorBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_InvokeToResult(
            IntPtr ptrHandle,
            IntPtr* ptrResultHandle,
            FfiCallResult* result)
        {
            if (ptrResultHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation result handle output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                PowerShell ps = pipeline.PowerShell;
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                FfiInvocationResultSnapshot snapshot = InvokeAndCaptureStreamSnapshot(
                    ps,
                    TakeCompletedInput(ptrHandle),
                    pipeline.Session,
                    pipeline.TakeCapabilityContext());
                GCHandle handle = GCHandle.Alloc(snapshot, GCHandleType.Normal);
                *ptrResultHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_InvokeSecretResult(
            IntPtr ptrHandle,
            uint expectedKind,
            byte* userNameBuffer,
            int userNameCapacity,
            int* userNameLength,
            char* secretBuffer,
            int secretCapacity,
            int* secretLength,
            FfiCallResult* result)
        {
            if (expectedKind > (uint)FfiValueKind.Credential ||
                expectedKind > 2 ||
                userNameLength == null ||
                secretLength == null ||
                userNameCapacity < 0 ||
                secretCapacity < 0 ||
                (userNameCapacity != 0 && userNameBuffer == null) ||
                (secretCapacity != 0 && secretBuffer == null))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Secret result arguments are invalid.");
            }

            return ExecuteSecret(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                if (!pipeline.HasSecretBindings)
                {
                    throw new InvalidOperationException("Secret result invocation requires an explicit secret parameter.");
                }

                SecureString resultSecret = null;
                try
                {
                    FfiSecretInvocationOutput invocation = InvokeSecretPipeline(
                        pipeline,
                        TakeCompletedInput(ptrHandle),
                        captureOutput: expectedKind != 0);
                    if (invocation.HadErrors)
                    {
                        throw new InvalidOperationException("Secret-bound PowerShell invocation failed.");
                    }

                    *userNameLength = 0;
                    *secretLength = 0;
                    if (expectedKind == 0)
                    {
                        return;
                    }

                    if (invocation.OutputCount != 1)
                    {
                        throw new InvalidOperationException("Secret result shape is invalid.");
                    }

                    PSObject output = invocation.Output;
                    object value = output.BaseObject;
                    SecureString secureString;
                    string userName = null;
                    if (expectedKind == 1 && value is SecureString resultSecureString)
                    {
                        secureString = resultSecureString;
                    }
                    else if (expectedKind == 2)
                    {
                        if (value is PSCredential credential)
                        {
                            secureString = credential.Password;
                            userName = credential.UserName;
                        }
                        else if (!TryProjectCredentialResult(output, value, out userName, out secureString))
                        {
                            throw new InvalidOperationException("Secret result shape is invalid.");
                        }
                    }
                    else
                    {
                        throw new InvalidOperationException("Secret result shape is invalid.");
                    }
                    resultSecret = secureString;

                    int requiredSecretLength = secureString.Length;
                    if (requiredSecretLength is < 1 or > FfiMaxSecretLength || secretCapacity < requiredSecretLength)
                    {
                        throw new InvalidOperationException("Secret result exceeds its bound.");
                    }

                    if (userName is not null)
                    {
                        if (string.IsNullOrWhiteSpace(userName) ||
                            userName.Length > FfiMaxSecretUserNameLength ||
                            userName.IndexOf('\0') >= 0)
                        {
                            throw new InvalidOperationException("Secret result shape is invalid.");
                        }

                        int requiredUserNameLength = Encoding.UTF8.GetByteCount(userName);
                        if (requiredUserNameLength > userNameCapacity)
                        {
                            throw new InvalidOperationException("Secret result exceeds its bound.");
                        }

                        Encoding.UTF8.GetBytes(userName, new Span<byte>(userNameBuffer, requiredUserNameLength));
                        *userNameLength = requiredUserNameLength;
                    }

                    IntPtr bstr = Marshal.SecureStringToBSTR(secureString);
                    try
                    {
                        new ReadOnlySpan<char>((void*)bstr, requiredSecretLength)
                            .CopyTo(new Span<char>(secretBuffer, requiredSecretLength));
                        *secretLength = requiredSecretLength;
                    }
                    finally
                    {
                        Marshal.ZeroFreeBSTR(bstr);
                    }
                }
                finally
                {
                    pipeline.DisposeSecretResult(resultSecret);
                    pipeline.ClearSecretBindings();
                    pipeline.PowerShell.Commands.Clear();
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_InvokeCredentialResult(
            IntPtr ptrHandle,
            FfiCredentialResult* credentialResult,
            FfiCallResult* result)
        {
            if (credentialResult == null ||
                credentialResult->Size < (uint)sizeof(FfiCredentialResult) ||
                !HasValidCredentialBuffer(credentialResult->Username, credentialResult->UsernameCapacity) ||
                !HasValidCredentialBuffer(credentialResult->Domain, credentialResult->DomainCapacity) ||
                !HasValidCredentialSecretBuffer(credentialResult->Password, credentialResult->PasswordCapacity) ||
                !HasValidCredentialBuffer(credentialResult->OutputMessages, credentialResult->OutputMessagesCapacity) ||
                !HasValidCredentialBuffer(credentialResult->ErrorMessages, credentialResult->ErrorMessagesCapacity) ||
                !HasValidCredentialBuffer(credentialResult->LogMessage, credentialResult->LogMessageCapacity))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Credential result arguments are invalid.");
            }

            return ExecuteSecret(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                var sink = new FfiCredentialResultSink();
                FfiPowerShellSession session = pipeline.Session;
                bool sessionInvocationStarted = false;
                Exception terminatingException = null;
                Runspace runspace = null;
                Runspace ownedCredentialRunspace = null;
                PSVariable previousResult = null;
                bool resultVariableRead = false;
                try
                {
                    if (session is not null)
                    {
                        session.BeginInvocation();
                        sessionInvocationStarted = true;
                    }

                    runspace = pipeline.PowerShell.Runspace;
                    if (runspace is null)
                    {
                        ownedCredentialRunspace = RunspaceFactory.CreateRunspace();
                        ownedCredentialRunspace.Open();
                        pipeline.PowerShell.Runspace = ownedCredentialRunspace;
                        runspace = ownedCredentialRunspace;
                    }

                    previousResult = runspace.SessionStateProxy.PSVariable.Get("Result");
                    resultVariableRead = true;
                    runspace.SessionStateProxy.PSVariable.Set(new PSVariable("Result", sink));
                    FfiCredentialInvocationOutput invocation = InvokeCredentialPipeline(
                        pipeline,
                        TakeCompletedInput(ptrHandle));
                    if (invocation.OutputCount != 0 || invocation.HadErrors)
                    {
                        throw new InvalidOperationException("Credential result invocation emitted output or errors.");
                    }

                    WriteCredentialResult(credentialResult, sink);
                }
                catch (Exception exception)
                {
                    terminatingException = exception;
                    throw;
                }
                finally
                {
                    try
                    {
                        if (resultVariableRead && runspace is not null && previousResult is null)
                        {
                            runspace.SessionStateProxy.PSVariable.Remove("Result");
                        }
                        else if (resultVariableRead && runspace is not null)
                        {
                            runspace.SessionStateProxy.PSVariable.Set(previousResult);
                        }
                    }
                    finally
                    {
                        try
                        {
                            try
                            {
                                sink.Dispose();
                                pipeline.PowerShell.Commands.Clear();
                            }
                            finally
                            {
                                if (ownedCredentialRunspace is not null)
                                {
                                    try
                                    {
                                        pipeline.PowerShell.Runspace = null;
                                    }
                                    finally
                                    {
                                        ownedCredentialRunspace.Dispose();
                                    }
                                }
                            }
                        }
                        finally
                        {
                            if (sessionInvocationStarted)
                            {
                                session.EndInvocation(terminatingException is not null);
                            }
                        }
                    }
                }
            });
        }

        private static unsafe bool HasValidCredentialBuffer(byte* buffer, int capacity)
        {
            return capacity >= 0 && (capacity == 0 || buffer != null);
        }

        private static unsafe bool HasValidCredentialSecretBuffer(char* buffer, int capacity)
        {
            return capacity >= 0 && (capacity == 0 || buffer != null);
        }

        private static unsafe void WriteCredentialResult(
            FfiCredentialResult* result,
            FfiCredentialResultSink sink)
        {
            result->IsCancelled = sink.IsCancelled ? 1U : 0U;
            result->UsernameLength = WriteCredentialText(
                sink.UsernameValue,
                result->Username,
                result->UsernameCapacity);
            result->DomainLength = WriteCredentialText(
                sink.DomainValue,
                result->Domain,
                result->DomainCapacity);
            result->LogMessageLength = WriteCredentialText(
                sink.LogMessageValue,
                result->LogMessage,
                result->LogMessageCapacity);
            result->OutputMessagesLength = WriteCredentialMessages(
                sink.OutputMessageValues,
                result->OutputMessages,
                result->OutputMessagesCapacity);
            result->ErrorMessagesLength = WriteCredentialMessages(
                sink.ErrorMessageValues,
                result->ErrorMessages,
                result->ErrorMessagesCapacity);
            result->PasswordLength = sink.IsCancelled
                ? 0
                : sink.CopyPassword(result->Password, result->PasswordCapacity);
        }

        private static unsafe int WriteCredentialText(string value, byte* destination, int capacity)
        {
            if (value is null)
            {
                return 0;
            }

            int required = Encoding.UTF8.GetByteCount(value);
            if (required > capacity)
            {
                throw new InvalidOperationException("Credential result exceeds its bound.");
            }

            Encoding.UTF8.GetBytes(value, new Span<byte>(destination, required));
            return required;
        }

        private static unsafe int WriteCredentialMessages(
            IReadOnlyList<string> messages,
            byte* destination,
            int capacity)
        {
            int required = sizeof(uint);
            foreach (string message in messages)
            {
                required = checked(required + sizeof(uint) + Encoding.UTF8.GetByteCount(message));
            }
            if (required > capacity)
            {
                throw new InvalidOperationException("Credential result messages exceed their bound.");
            }

            Span<byte> target = new(destination, required);
            BinaryPrimitives.WriteUInt32LittleEndian(target, checked((uint)messages.Count));
            int offset = sizeof(uint);
            foreach (string message in messages)
            {
                int length = Encoding.UTF8.GetByteCount(message);
                BinaryPrimitives.WriteUInt32LittleEndian(target[offset..], checked((uint)length));
                offset += sizeof(uint);
                Encoding.UTF8.GetBytes(message, target[offset..(offset + length)]);
                offset += length;
            }

            return required;
        }

        private static FfiCredentialInvocationOutput InvokeCredentialPipeline(
            FfiPowerShellPipeline pipeline,
            object[] input)
        {
            PowerShell powerShell = pipeline.PowerShell;
            FfiPowerShellSession session = pipeline.Session;
            var output = new PSDataCollection<PSObject> { DataAddedCount = 1 };
            int outputCount = 0;
            bool hadErrors = false;
            PSDataCollection<object> inputCollection = null;

            EventHandler<DataAddedEventArgs> outputAdded = (_, _) =>
            {
                outputCount++;
                output.Clear();
            };
            EventHandler<DataAddedEventArgs> errorAdded = (_, _) =>
            {
                hadErrors = true;
                powerShell.Streams.Error.Clear();
            };
            EventHandler<DataAddedEventArgs> warningAdded = (_, _) => powerShell.Streams.Warning.Clear();
            EventHandler<DataAddedEventArgs> verboseAdded = (_, _) => powerShell.Streams.Verbose.Clear();
            EventHandler<DataAddedEventArgs> debugAdded = (_, _) => powerShell.Streams.Debug.Clear();
            EventHandler<DataAddedEventArgs> informationAdded = (_, _) => powerShell.Streams.Information.Clear();
            EventHandler<DataAddedEventArgs> progressAdded = (_, _) => powerShell.Streams.Progress.Clear();

            ClearStreamBuffers(powerShell);
            output.DataAdded += outputAdded;
            powerShell.Streams.Error.DataAdded += errorAdded;
            powerShell.Streams.Warning.DataAdded += warningAdded;
            powerShell.Streams.Verbose.DataAdded += verboseAdded;
            powerShell.Streams.Debug.DataAdded += debugAdded;
            powerShell.Streams.Information.DataAdded += informationAdded;
            powerShell.Streams.Progress.DataAdded += progressAdded;
            try
            {
                PSInvocationSettings invocationSettings = session?.CreateInvocationSettings();
                if (input is null)
                {
                    powerShell.Invoke<PSObject, PSObject>(null, output, invocationSettings);
                }
                else
                {
                    inputCollection = new PSDataCollection<object>();
                    foreach (object value in input)
                    {
                        inputCollection.Add(value);
                    }

                    inputCollection.Complete();
                    powerShell.Invoke<object, PSObject>(inputCollection, output, invocationSettings);
                }

                hadErrors |= powerShell.HadErrors;
                return new FfiCredentialInvocationOutput(outputCount, hadErrors);
            }
            finally
            {
                output.DataAdded -= outputAdded;
                powerShell.Streams.Error.DataAdded -= errorAdded;
                powerShell.Streams.Warning.DataAdded -= warningAdded;
                powerShell.Streams.Verbose.DataAdded -= verboseAdded;
                powerShell.Streams.Debug.DataAdded -= debugAdded;
                powerShell.Streams.Information.DataAdded -= informationAdded;
                powerShell.Streams.Progress.DataAdded -= progressAdded;
                inputCollection?.Clear();
                output.Clear();
                ClearStreamBuffers(powerShell);
            }
        }

        private sealed class FfiCredentialInvocationOutput
        {
            public FfiCredentialInvocationOutput(int outputCount, bool hadErrors)
            {
                OutputCount = outputCount;
                HadErrors = hadErrors;
            }

            public int OutputCount { get; }

            public bool HadErrors { get; }
        }

        private sealed unsafe class FfiCredentialResultSink : IDisposable
        {
            private SecureString password;
            private string[] outputMessages = Array.Empty<string>();
            private string[] errorMessages = Array.Empty<string>();

            public object Username
            {
                set => username = ValidateCredentialText(value, "Username", allowNull: true);
            }

            public object Domain
            {
                set => domain = ValidateCredentialText(value, "Domain", allowNull: true);
            }

            public object Password
            {
                set => SetPassword(value, secure: false);
            }

            public object SecurePassword
            {
                set => SetPassword(value, secure: true);
            }

            public object OutputMessages
            {
                set => outputMessages = ValidateCredentialMessages(value, "OutputMessages");
            }

            public object ErrorMessages
            {
                set => errorMessages = ValidateCredentialMessages(value, "ErrorMessages");
            }

            public object LogMessage
            {
                set => logMessage = ValidateCredentialText(value, "LogMessage", allowNull: true);
            }

            public object Cancel
            {
                set
                {
                    if (value is not bool cancelled)
                    {
                        throw new InvalidOperationException("Credential result Cancel must be Boolean.");
                    }

                    isCancelled = cancelled;
                }
            }

            private string username;
            private string domain;
            private string logMessage;
            private bool isCancelled;

            internal string UsernameValue => username;
            internal string DomainValue => domain;
            internal string LogMessageValue => logMessage;
            internal IReadOnlyList<string> OutputMessageValues => outputMessages;
            internal IReadOnlyList<string> ErrorMessageValues => errorMessages;
            internal bool IsCancelled => isCancelled;

            internal int CopyPassword(char* destination, int capacity)
            {
                if (password is null)
                {
                    return 0;
                }

                int length = password.Length;
                if (length > capacity)
                {
                    throw new InvalidOperationException("Credential password exceeds its bound.");
                }

                IntPtr bstr = Marshal.SecureStringToBSTR(password);
                try
                {
                    new ReadOnlySpan<char>((void*)bstr, length).CopyTo(new Span<char>(destination, length));
                    return length;
                }
                finally
                {
                    Marshal.ZeroFreeBSTR(bstr);
                }
            }

            public void Dispose()
            {
                SecureString value = Interlocked.Exchange(ref password, null);
                value?.Dispose();
            }

            private void SetPassword(object value, bool secure)
            {
                SecureString next = null;
                if (value is null)
                {
                    ReplacePassword(null);
                    return;
                }

                if (secure)
                {
                    if (value is not SecureString secureString)
                    {
                        throw new InvalidOperationException("Credential result SecurePassword must be a SecureString.");
                    }

                    next = CopySecureString(secureString);
                }
                else
                {
                    if (value is not string text)
                    {
                        throw new InvalidOperationException("Credential result Password must be a string.");
                    }

                    next = CreateSecureString(text);
                }

                ReplacePassword(next);
            }

            private void ReplacePassword(SecureString next)
            {
                SecureString previous = Interlocked.Exchange(ref password, next);
                previous?.Dispose();
            }

            private static string ValidateCredentialText(object value, string name, bool allowNull)
            {
                if (value is null && allowNull)
                {
                    return null;
                }
                if (value is not string text ||
                    text.IndexOf('\0') >= 0 ||
                    Encoding.UTF8.GetByteCount(text) > FfiMaxCredentialTextUtf8Bytes)
                {
                    throw new InvalidOperationException($"Credential result {name} is invalid.");
                }

                return text;
            }

            private static string[] ValidateCredentialMessages(object value, string name)
            {
                if (value is not object[] values || values.Length > FfiMaxCredentialMessages)
                {
                    throw new InvalidOperationException($"Credential result {name} must be a bounded string array.");
                }

                var result = new string[values.Length];
                for (int index = 0; index < values.Length; index++)
                {
                    if (values[index] is not string text ||
                        text.Length == 0 ||
                        text.IndexOf('\0') >= 0 ||
                        Encoding.UTF8.GetByteCount(text) > FfiMaxCredentialMessageUtf8Bytes)
                    {
                        throw new InvalidOperationException($"Credential result {name} is invalid.");
                    }

                    result[index] = text;
                }

                return result;
            }

            private static SecureString CreateSecureString(string text)
            {
                if (text.Length is < 1 or > FfiMaxSecretLength || text.IndexOf('\0') >= 0)
                {
                    throw new InvalidOperationException("Credential password is invalid.");
                }

                var result = new SecureString();
                foreach (char character in text)
                {
                    result.AppendChar(character);
                }
                result.MakeReadOnly();
                return result;
            }

            private static SecureString CopySecureString(SecureString source)
            {
                if (source.Length is < 1 or > FfiMaxSecretLength)
                {
                    throw new InvalidOperationException("Credential password is invalid.");
                }

                IntPtr bstr = Marshal.SecureStringToBSTR(source);
                try
                {
                    var result = new SecureString();
                    foreach (char character in new ReadOnlySpan<char>((void*)bstr, source.Length))
                    {
                        if (character == '\0')
                        {
                            result.Dispose();
                            throw new InvalidOperationException("Credential password is invalid.");
                        }
                        result.AppendChar(character);
                    }
                    result.MakeReadOnly();
                    return result;
                }
                finally
                {
                    Marshal.ZeroFreeBSTR(bstr);
                }
            }
        }

        private static bool TryProjectCredentialResult(
            PSObject result,
            object value,
            out string userName,
            out SecureString secret)
        {
            userName = null;
            secret = null;
            if (value is null)
            {
                return false;
            }

            Type valueType = value.GetType();
            if (!string.Equals(valueType.FullName, typeof(PSCredential).FullName, StringComparison.Ordinal) ||
                !string.Equals(
                    valueType.Assembly.GetName().Name,
                    typeof(PSCredential).Assembly.GetName().Name,
                    StringComparison.Ordinal))
            {
                return false;
            }

            // The selected payload may load SMA in a different context. Project only
            // the fixed PSCredential members from its already-known PSObject wrapper.
            PSPropertyInfo userNameProperty = result.Properties["UserName"];
            PSPropertyInfo passwordProperty = result.Properties["Password"];
            if (userNameProperty?.Value is not string credentialUserName ||
                passwordProperty?.Value is not SecureString credentialSecret)
            {
                return false;
            }

            userName = credentialUserName;
            secret = credentialSecret;
            return true;
        }

        private static FfiSecretInvocationOutput InvokeSecretPipeline(
            FfiPowerShellPipeline pipeline,
            object[] input,
            bool captureOutput)
        {
            PowerShell powerShell = pipeline.PowerShell;
            FfiPowerShellSession session = pipeline.Session;
            var output = new PSDataCollection<PSObject> { DataAddedCount = 1 };
            PSObject capturedOutput = null;
            int outputCount = 0;
            bool hadErrors = false;
            bool sessionInvocationStarted = false;
            Exception terminatingException = null;
            PSDataCollection<object> inputCollection = null;

            EventHandler<DataAddedEventArgs> outputAdded = (_, args) =>
            {
                if (captureOutput)
                {
                    PSObject value = output[args.Index];
                    outputCount++;
                    if (outputCount == 1)
                    {
                        capturedOutput = value;
                    }
                    else
                    {
                        output.Clear();
                        throw new InvalidOperationException("Secret result shape is invalid.");
                    }
                }

                output.Clear();
            };
            EventHandler<DataAddedEventArgs> errorAdded = (_, _) =>
            {
                hadErrors = true;
                powerShell.Streams.Error.Clear();
            };
            EventHandler<DataAddedEventArgs> warningAdded = (_, _) => powerShell.Streams.Warning.Clear();
            EventHandler<DataAddedEventArgs> verboseAdded = (_, _) => powerShell.Streams.Verbose.Clear();
            EventHandler<DataAddedEventArgs> debugAdded = (_, _) => powerShell.Streams.Debug.Clear();
            EventHandler<DataAddedEventArgs> informationAdded = (_, _) => powerShell.Streams.Information.Clear();
            EventHandler<DataAddedEventArgs> progressAdded = (_, _) => powerShell.Streams.Progress.Clear();

            ClearStreamBuffers(powerShell);
            output.DataAdded += outputAdded;
            powerShell.Streams.Error.DataAdded += errorAdded;
            powerShell.Streams.Warning.DataAdded += warningAdded;
            powerShell.Streams.Verbose.DataAdded += verboseAdded;
            powerShell.Streams.Debug.DataAdded += debugAdded;
            powerShell.Streams.Information.DataAdded += informationAdded;
            powerShell.Streams.Progress.DataAdded += progressAdded;
            try
            {
                if (session is not null)
                {
                    session.BeginInvocation();
                    sessionInvocationStarted = true;
                }

                PSInvocationSettings invocationSettings = session?.CreateInvocationSettings();
                if (input is null)
                {
                    powerShell.Invoke<PSObject, PSObject>(null, output, invocationSettings);
                }
                else
                {
                    inputCollection = new PSDataCollection<object>();
                    foreach (object value in input)
                    {
                        inputCollection.Add(value);
                    }

                    inputCollection.Complete();
                    powerShell.Invoke<object, PSObject>(inputCollection, output, invocationSettings);
                }

                hadErrors |= powerShell.HadErrors;
                return new FfiSecretInvocationOutput(capturedOutput, outputCount, hadErrors);
            }
            catch (Exception exception)
            {
                terminatingException = exception;
                throw;
            }
            finally
            {
                output.DataAdded -= outputAdded;
                powerShell.Streams.Error.DataAdded -= errorAdded;
                powerShell.Streams.Warning.DataAdded -= warningAdded;
                powerShell.Streams.Verbose.DataAdded -= verboseAdded;
                powerShell.Streams.Debug.DataAdded -= debugAdded;
                powerShell.Streams.Information.DataAdded -= informationAdded;
                powerShell.Streams.Progress.DataAdded -= progressAdded;
                inputCollection?.Clear();
                output.Clear();
                ClearStreamBuffers(powerShell);
                if (sessionInvocationStarted)
                {
                    session.EndInvocation(terminatingException is not null);
                }
            }
        }

        private sealed class FfiSecretInvocationOutput
        {
            public FfiSecretInvocationOutput(PSObject output, int outputCount, bool hadErrors)
            {
                Output = output;
                OutputCount = outputCount;
                HadErrors = hadErrors;
            }

            public PSObject Output { get; }

            public int OutputCount { get; }

            public bool HadErrors { get; }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_BeginLiveInvocation(
            IntPtr ptrHandle,
            IntPtr* ptrLiveInvocationHandle,
            FfiCallResult* result)
        {
            if (ptrLiveInvocationHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation handle output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                var liveInvocation = new FfiLiveInvocation(
                    pipeline.PowerShell,
                    TakeCompletedInput(ptrHandle),
                    pipeline.Session,
                    pipeline.TakeCapabilityContext(),
                    pipeline.TakeBrokerContext(),
                    pipeline.TakeBridgeContext());
                try
                {
                    liveInvocation.Start();
                    GCHandle handle = GCHandle.Alloc(liveInvocation, GCHandleType.Normal);
                    *ptrLiveInvocationHandle = GCHandle.ToIntPtr(handle);
                }
                catch
                {
                    liveInvocation.Dispose();
                    throw;
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_BeginTypedResultInvocation(
            IntPtr ptrHandle,
            int maximumBufferedRecords,
            int maximumPageRecords,
            IntPtr* ptrTypedResultInvocationHandle,
            FfiCallResult* result)
        {
            if (ptrTypedResultInvocationHandle == null ||
                maximumBufferedRecords < 1 ||
                maximumBufferedRecords > FfiMaxValueContainerEntries ||
                maximumPageRecords < 1 ||
                maximumPageRecords > maximumBufferedRecords)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result invocation bounds or output pointer are invalid.");
            }

            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                var typedResults = new FfiTypedResultQueue(maximumBufferedRecords, maximumPageRecords);
                var liveInvocation = new FfiLiveInvocation(
                    pipeline.PowerShell,
                    TakeCompletedInput(ptrHandle),
                    pipeline.Session,
                    pipeline.TakeCapabilityContext(),
                    pipeline.TakeBrokerContext(),
                    pipeline.TakeBridgeContext(),
                    typedResults);
                try
                {
                    liveInvocation.Start();
                    GCHandle handle = GCHandle.Alloc(liveInvocation, GCHandleType.Normal);
                    *ptrTypedResultInvocationHandle = GCHandle.ToIntPtr(handle);
                }
                catch
                {
                    liveInvocation.Dispose();
                    throw;
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultInvocation_Poll(
            IntPtr ptrTypedResultInvocationHandle,
            int* isCompleted,
            FfiCallResult* result)
        {
            if (isCompleted == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result invocation completion output pointer is null.");
            }

            return Execute(result, () =>
            {
                *isCompleted = GetLiveInvocation(ptrTypedResultInvocationHandle).IsCompleted ? 1 : 0;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultInvocation_ReadPage(
            IntPtr ptrTypedResultInvocationHandle,
            long acknowledgedThrough,
            int maximumRecords,
            IntPtr* ptrPageHandle,
            FfiCallResult* result)
        {
            if (ptrPageHandle == null || acknowledgedThrough < 0 || maximumRecords < 1 || maximumRecords > FfiMaxValueContainerEntries)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result page arguments are invalid.");
            }

            return Execute(result, () =>
            {
                FfiTypedResultPage page = GetLiveInvocation(ptrTypedResultInvocationHandle)
                    .ReadTypedResultPage(acknowledgedThrough, maximumRecords);
                GCHandle handle = GCHandle.Alloc(page, GCHandleType.Normal);
                *ptrPageHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultInvocation_Complete(
            IntPtr ptrTypedResultInvocationHandle,
            FfiCallResult* result)
        {
            return Execute(result, () => GetLiveInvocation(ptrTypedResultInvocationHandle).CompleteTypedResults());
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultInvocation_Stop(
            IntPtr ptrTypedResultInvocationHandle,
            FfiCallResult* result)
        {
            return Execute(result, () => GetLiveInvocation(ptrTypedResultInvocationHandle).Stop());
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultInvocation_Release(
            IntPtr ptrTypedResultInvocationHandle,
            FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrTypedResultInvocationHandle);
                if (!handle.IsAllocated || handle.Target is not FfiLiveInvocation invocation)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "Typed result invocation handle is invalid.");
                }

                handle.Free();
                invocation.Dispose();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultPage_GetInfo(
            IntPtr ptrPageHandle,
            long* acknowledgedSequence,
            long* nextSequence,
            long* totalRecordCount,
            long* droppedRecordCount,
            int* terminalStatus,
            uint* flags,
            int* recordCount,
            FfiCallResult* result)
        {
            if (acknowledgedSequence == null ||
                nextSequence == null ||
                totalRecordCount == null ||
                droppedRecordCount == null ||
                terminalStatus == null ||
                flags == null ||
                recordCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result page info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiTypedResultPage page = GetTypedResultPage(ptrPageHandle);
                *acknowledgedSequence = page.AcknowledgedSequence;
                *nextSequence = page.NextSequence;
                *totalRecordCount = page.TotalRecordCount;
                *droppedRecordCount = page.DroppedRecordCount;
                *terminalStatus = page.TerminalStatus;
                *flags = page.Flags;
                *recordCount = page.Records.Length;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultPage_GetRecordInfo(
            IntPtr ptrPageHandle,
            int recordIndex,
            long* sequence,
            uint* kind,
            FfiCallResult* result)
        {
            if (sequence == null || kind == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result page record output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiTypedResultRecord record = GetTypedResultPage(ptrPageHandle).GetRecord(recordIndex);
                *sequence = record.Sequence;
                *kind = record.Value.Kind;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultPage_CopyRecordValue(
            IntPtr ptrPageHandle,
            int recordIndex,
            uint* kind,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (kind == null || requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Typed result page value buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                FfiSnapshotValue value = GetTypedResultPage(ptrPageHandle).GetRecord(recordIndex).Value;
                *kind = value.Kind;
                Marshal.WriteInt32(outputRequiredLength, value.Payload.Length);
                if (bufferLength < value.Payload.Length)
                {
                    throw new BufferTooSmallException();
                }

                if (value.Payload.Length != 0)
                {
                    Marshal.Copy(value.Payload, 0, outputBuffer, value.Payload.Length);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiTypedResultPage_Release(IntPtr ptrPageHandle, FfiCallResult* result)
        {
            return ReleaseLiveHandle<FfiTypedResultPage>(
                ptrPageHandle,
                result,
                "Typed result page handle is invalid.");
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_BeginObservedInvocation(
            IntPtr ptrHandle,
            int maximumBufferedResultRecords,
            int maximumResultPageRecords,
            int maximumBufferedDiagnosticRecords,
            int maximumDiagnosticPageRecords,
            IntPtr* ptrObservedInvocationHandle,
            FfiCallResult* result)
        {
            if (ptrObservedInvocationHandle == null ||
                maximumBufferedResultRecords < 1 ||
                maximumBufferedResultRecords > FfiMaxValueContainerEntries ||
                maximumResultPageRecords < 1 ||
                maximumResultPageRecords > maximumBufferedResultRecords ||
                maximumBufferedDiagnosticRecords < 1 ||
                maximumBufferedDiagnosticRecords > FfiMaxValueContainerEntries ||
                maximumDiagnosticPageRecords < 1 ||
                maximumDiagnosticPageRecords > maximumBufferedDiagnosticRecords)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed invocation bounds or output pointer are invalid.");
            }

            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                pipeline.ThrowIfSecretBound();
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                var typedResults = new FfiTypedResultQueue(
                    maximumBufferedResultRecords,
                    maximumResultPageRecords);
                var diagnostics = new FfiObservedDiagnosticQueue(
                    maximumBufferedDiagnosticRecords,
                    maximumDiagnosticPageRecords);
                var liveInvocation = new FfiLiveInvocation(
                    pipeline.PowerShell,
                    TakeCompletedInput(ptrHandle),
                    pipeline.Session,
                    pipeline.TakeCapabilityContext(),
                    pipeline.TakeBrokerContext(),
                    pipeline.TakeBridgeContext(),
                    typedResults,
                    diagnostics);
                try
                {
                    liveInvocation.Start();
                    GCHandle handle = GCHandle.Alloc(liveInvocation, GCHandleType.Normal);
                    *ptrObservedInvocationHandle = GCHandle.ToIntPtr(handle);
                }
                catch
                {
                    liveInvocation.Dispose();
                    throw;
                }
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_Poll(
            IntPtr ptrObservedInvocationHandle,
            int* isCompleted,
            FfiCallResult* result)
        {
            if (isCompleted == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed invocation completion output pointer is null.");
            }

            return Execute(result, () =>
            {
                *isCompleted = GetLiveInvocation(ptrObservedInvocationHandle).IsCompleted ? 1 : 0;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_ReadResultPage(
            IntPtr ptrObservedInvocationHandle,
            long acknowledgedThrough,
            int maximumRecords,
            IntPtr* ptrPageHandle,
            FfiCallResult* result)
        {
            if (ptrPageHandle == null || acknowledgedThrough < 0 || maximumRecords < 1 || maximumRecords > FfiMaxValueContainerEntries)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed result page arguments are invalid.");
            }

            return Execute(result, () =>
            {
                FfiTypedResultPage page = GetLiveInvocation(ptrObservedInvocationHandle)
                    .ReadObservedResultPage(acknowledgedThrough, maximumRecords);
                GCHandle handle = GCHandle.Alloc(page, GCHandleType.Normal);
                *ptrPageHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_ReadDiagnosticPage(
            IntPtr ptrObservedInvocationHandle,
            long acknowledgedThrough,
            int maximumRecords,
            IntPtr* ptrPageHandle,
            FfiCallResult* result)
        {
            if (ptrPageHandle == null || acknowledgedThrough < 0 || maximumRecords < 1 || maximumRecords > FfiMaxValueContainerEntries)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed diagnostic page arguments are invalid.");
            }

            return Execute(result, () =>
            {
                FfiObservedDiagnosticPage page = GetLiveInvocation(ptrObservedInvocationHandle)
                    .ReadObservedDiagnosticPage(acknowledgedThrough, maximumRecords);
                GCHandle handle = GCHandle.Alloc(page, GCHandleType.Normal);
                *ptrPageHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_Complete(
            IntPtr ptrObservedInvocationHandle,
            FfiCallResult* result)
        {
            return Execute(result, () => GetLiveInvocation(ptrObservedInvocationHandle).CompleteObservedInvocation());
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_Stop(
            IntPtr ptrObservedInvocationHandle,
            FfiCallResult* result)
        {
            return Execute(result, () => GetLiveInvocation(ptrObservedInvocationHandle).Stop());
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedInvocation_Release(
            IntPtr ptrObservedInvocationHandle,
            FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrObservedInvocationHandle);
                if (!handle.IsAllocated || handle.Target is not FfiLiveInvocation invocation)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "Observed invocation handle is invalid.");
                }

                handle.Free();
                invocation.Dispose();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedDiagnosticPage_GetInfo(
            IntPtr ptrPageHandle,
            long* acknowledgedSequence,
            long* nextSequence,
            long* totalRecordCount,
            long* droppedRecordCount,
            int* terminalStatus,
            uint* flags,
            int* recordCount,
            FfiCallResult* result)
        {
            if (acknowledgedSequence == null ||
                nextSequence == null ||
                totalRecordCount == null ||
                droppedRecordCount == null ||
                terminalStatus == null ||
                flags == null ||
                recordCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed diagnostic page info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiObservedDiagnosticPage page = GetObservedDiagnosticPage(ptrPageHandle);
                *acknowledgedSequence = page.AcknowledgedSequence;
                *nextSequence = page.NextSequence;
                *totalRecordCount = page.TotalRecordCount;
                *droppedRecordCount = page.DroppedRecordCount;
                *terminalStatus = page.TerminalStatus;
                *flags = page.Flags;
                *recordCount = page.Records.Length;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedDiagnosticPage_GetRecordInfo(
            IntPtr ptrPageHandle,
            int recordIndex,
            int* stream,
            long* sequence,
            FfiCallResult* result)
        {
            if (stream == null || sequence == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed diagnostic page record output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiObservedDiagnosticRecord record = GetObservedDiagnosticPage(ptrPageHandle).GetRecord(recordIndex);
                *stream = record.Stream;
                *sequence = record.Sequence;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedDiagnosticPage_CopyRecordTextToUtf8(
            IntPtr ptrPageHandle,
            int recordIndex,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed diagnostic page text buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                string value = GetObservedDiagnosticPage(ptrPageHandle).GetRecord(recordIndex).Text;
                int required = Encoding.UTF8.GetByteCount(value);
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] valueBytes = Encoding.UTF8.GetBytes(value);
                    Marshal.Copy(valueBytes, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedDiagnosticPage_CopyRecordValue(
            IntPtr ptrPageHandle,
            int recordIndex,
            uint* kind,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (kind == null || requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Observed diagnostic page value buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputKind = (IntPtr)kind;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                FfiSnapshotValue value = GetObservedDiagnosticPage(ptrPageHandle).GetRecord(recordIndex).Value
                    ?? throw new InvalidOperationException("Observed diagnostic record has no copied value.");
                Marshal.WriteInt32(outputKind, unchecked((int)value.Kind));
                int required = value.Payload.Length;
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    Marshal.Copy(value.Payload, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiObservedDiagnosticPage_Release(IntPtr ptrPageHandle, FfiCallResult* result)
        {
            return ReleaseLiveHandle<FfiObservedDiagnosticPage>(
                ptrPageHandle,
                result,
                "Observed diagnostic page handle is invalid.");
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocation_Poll(
            IntPtr ptrLiveInvocationHandle,
            int* isCompleted,
            FfiCallResult* result)
        {
            if (isCompleted == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation completion output pointer is null.");
            }

            return Execute(result, () =>
            {
                *isCompleted = GetLiveInvocation(ptrLiveInvocationHandle).IsCompleted ? 1 : 0;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocation_ReadBatch(
            IntPtr ptrLiveInvocationHandle,
            long afterSequence,
            int maximumRecords,
            IntPtr* ptrBatchHandle,
            FfiCallResult* result)
        {
            if (ptrBatchHandle == null || afterSequence < 0 || maximumRecords < 1 || maximumRecords > FfiMaxStreamRecords)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation stream batch arguments are invalid.");
            }

            return Execute(result, () =>
            {
                FfiLiveStreamBatch batch = GetLiveInvocation(ptrLiveInvocationHandle)
                    .ReadBatch(afterSequence, maximumRecords);
                GCHandle handle = GCHandle.Alloc(batch, GCHandleType.Normal);
                *ptrBatchHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocationBatch_GetInfo(
            IntPtr ptrBatchHandle,
            long* nextSequence,
            long* totalRecordCount,
            long* lostRecordCount,
            int* recordCount,
            FfiCallResult* result)
        {
            if (nextSequence == null || totalRecordCount == null || lostRecordCount == null || recordCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation stream batch info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiLiveStreamBatch batch = GetLiveStreamBatch(ptrBatchHandle);
                *nextSequence = batch.NextSequence;
                *totalRecordCount = batch.TotalRecordCount;
                *lostRecordCount = batch.LostRecordCount;
                *recordCount = batch.Records.Length;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocationBatch_GetRecordInfo(
            IntPtr ptrBatchHandle,
            int recordIndex,
            int* stream,
            long* sequence,
            uint* flags,
            FfiCallResult* result)
        {
            if (stream == null || sequence == null || flags == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation stream record info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiLiveStreamRecord record = GetLiveStreamBatch(ptrBatchHandle).GetRecord(recordIndex);
                *stream = record.Stream;
                *sequence = record.Sequence;
                *flags = record.Flags;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocationBatch_CopyRecordTextToUtf8(
            IntPtr ptrBatchHandle,
            int recordIndex,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Live invocation stream record buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                string value = GetLiveStreamBatch(ptrBatchHandle).GetRecord(recordIndex).DisplayText;
                int required = Encoding.UTF8.GetByteCount(value);
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] valueBytes = Encoding.UTF8.GetBytes(value);
                    Marshal.Copy(valueBytes, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocationBatch_Release(IntPtr ptrBatchHandle, FfiCallResult* result)
        {
            return ReleaseLiveHandle<FfiLiveStreamBatch>(
                ptrBatchHandle,
                result,
                "Live invocation stream batch handle is invalid.");
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocation_Complete(
            IntPtr ptrLiveInvocationHandle,
            IntPtr* ptrResultHandle,
            FfiCallResult* result)
        {
            if (ptrResultHandle == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation result handle output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiInvocationResultSnapshot snapshot = GetLiveInvocation(ptrLiveInvocationHandle).Complete();
                GCHandle handle = GCHandle.Alloc(snapshot, GCHandleType.Normal);
                *ptrResultHandle = GCHandle.ToIntPtr(handle);
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocation_Stop(IntPtr ptrLiveInvocationHandle, FfiCallResult* result)
        {
            return Execute(result, () => GetLiveInvocation(ptrLiveInvocationHandle).Stop());
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiLiveInvocation_Release(IntPtr ptrLiveInvocationHandle, FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrLiveInvocationHandle);
                if (!handle.IsAllocated || handle.Target is not FfiLiveInvocation invocation)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "Live invocation handle is invalid.");
                }

                handle.Free();
                invocation.Dispose();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_SetCapabilityContext(
            IntPtr ptrHandle,
            ulong registrationHandle,
            ulong invocationId,
            IntPtr dispatcher,
            FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                if (registrationHandle == 0 && invocationId == 0 && dispatcher == IntPtr.Zero)
                {
                    pipeline.ClearCapabilityContext();
                    return;
                }
                if (registrationHandle == 0 || invocationId == 0 || dispatcher == IntPtr.Zero)
                {
                    throw new InvalidOperationException("Capability context is invalid.");
                }

                pipeline.SetCapabilityContext(new FfiCapabilityContext(registrationHandle, invocationId, dispatcher));
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_SetBrokerContext(
            IntPtr ptrHandle,
            ulong channelHandle,
            ulong generation,
            IntPtr enqueue,
            IntPtr post,
            uint maximumBodyBytes,
            FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                if (channelHandle == 0 && generation == 0 && enqueue == IntPtr.Zero && post == IntPtr.Zero &&
                    maximumBodyBytes == 0)
                {
                    pipeline.ClearBrokerContext();
                    return;
                }
                if (channelHandle == 0 || generation == 0 || enqueue == IntPtr.Zero || post == IntPtr.Zero ||
                    maximumBodyBytes == 0 || maximumBodyBytes > 64 * 1024)
                {
                    throw new InvalidOperationException("Broker context is invalid.");
                }

                pipeline.SetBrokerContext(
                    new FfiBrokerContext(channelHandle, generation, enqueue, post, (int)maximumBodyBytes));
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_SetBridgeContext(
            IntPtr ptrHandle,
            ulong bindingId,
            ulong interfaceIdLow,
            ulong interfaceIdHigh,
            ushort majorVersion,
            ushort minorVersion,
            uint maximumRequestBytes,
            uint maximumReplyBytes,
            byte* variableName,
            int variableNameLength,
            FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                FfiPowerShellPipeline pipeline = GetPowerShellPipeline(ptrHandle);
                if (bindingId == 0 && interfaceIdLow == 0 && interfaceIdHigh == 0 &&
                    majorVersion == 0 && minorVersion == 0 && maximumRequestBytes == 0 &&
                    maximumReplyBytes == 0 && variableName == null && variableNameLength == 0)
                {
                    pipeline.ClearBridgeContext();
                    return;
                }

                if (bindingId == 0 || (interfaceIdLow == 0 && interfaceIdHigh == 0) ||
                    majorVersion == 0 || maximumRequestBytes == 0 || maximumReplyBytes == 0 ||
                    variableName == null || variableNameLength is < 1 or > 64)
                {
                    throw new InvalidOperationException("Bridge context is invalid.");
                }

                string name = DecodeBridgeUtf8(variableName, variableNameLength);
                if (!IsBridgeVariableName(name))
                {
                    throw new InvalidOperationException("Bridge variable name is invalid.");
                }

                FfiBrokerContext broker = pipeline.GetBrokerContext()
                    ?? throw new InvalidOperationException("Bridge context requires an attached broker context.");
                Span<byte> identity = stackalloc byte[16];
                BinaryPrimitives.WriteUInt64LittleEndian(identity, interfaceIdLow);
                BinaryPrimitives.WriteUInt64LittleEndian(identity[8..], interfaceIdHigh);
                var contract = new PowerShellLiveObjectContract(
                    new Guid(identity),
                    majorVersion,
                    minorVersion,
                    PowerShellLiveObjectDirection.ConsumerToSession |
                    PowerShellLiveObjectDirection.BridgeContract);
                pipeline.SetBridgeContext(new FfiBridgeContext(
                    name,
                    contract,
                    bindingId,
                    maximumRequestBytes,
                    maximumReplyBytes,
                    broker));
            });
        }

        private static bool IsBridgeVariableName(string value)
        {
            if (string.IsNullOrEmpty(value) || value.Length > 64 ||
                !((value[0] >= 'A' && value[0] <= 'Z') ||
                  (value[0] >= 'a' && value[0] <= 'z') ||
                  value[0] == '_'))
            {
                return false;
            }

            for (int index = 1; index < value.Length; index++)
            {
                char current = value[index];
                if (!((current >= 'A' && current <= 'Z') ||
                      (current >= 'a' && current <= 'z') ||
                      (current >= '0' && current <= '9') ||
                      current == '_'))
                {
                    return false;
                }
            }

            return true;
        }

        private static unsafe string DecodeBridgeUtf8(byte* value, int length)
        {
            return new UTF8Encoding(false, true).GetString(new ReadOnlySpan<byte>(value, length));
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_Release(IntPtr ptrResultHandle, FfiCallResult* result)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrResultHandle);
                if (!handle.IsAllocated || handle.Target is not FfiInvocationResultSnapshot)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, "Invocation result handle is invalid.");
                }

                handle.Free();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetInfo(
            IntPtr ptrResultHandle,
            uint* flags,
            int* sequenceCount,
            FfiCallResult* result)
        {
            if (flags == null || sequenceCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation result info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiInvocationResultSnapshot snapshot = GetInvocationResultSnapshot(ptrResultHandle);
                *flags = snapshot.Flags;
                *sequenceCount = snapshot.Sequence.Length;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetMetadata(
            IntPtr ptrResultHandle,
            uint* state,
            long* invocationId,
            int* hadErrors,
            FfiCallResult* result)
        {
            if (state == null || invocationId == null || hadErrors == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation metadata output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiInvocationResultSnapshot snapshot = GetInvocationResultSnapshot(ptrResultHandle);
                *state = snapshot.State;
                *invocationId = snapshot.InvocationId;
                *hadErrors = snapshot.HadErrors ? 1 : 0;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetStreamInfo(
            IntPtr ptrResultHandle,
            int stream,
            int* recordCount,
            uint* flags,
            FfiCallResult* result)
        {
            if (recordCount == null || flags == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiStreamSnapshot snapshot = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream);
                *recordCount = snapshot.Records.Length;
                *flags = snapshot.Flags;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetStreamRecordInfo(
            IntPtr ptrResultHandle,
            int stream,
            int recordIndex,
            long* sequence,
            uint* flags,
            FfiCallResult* result)
        {
            if (sequence == null || flags == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream record info output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiStreamRecord record = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream).GetRecord(recordIndex);
                *sequence = record.Sequence;
                *flags = record.Flags;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_CopyStreamRecordFieldToUtf8(
            IntPtr ptrResultHandle,
            int stream,
            int recordIndex,
            int field,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream record buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                FfiStreamRecord record = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream).GetRecord(recordIndex);
                string value = record.GetField(field);
                int required = Encoding.UTF8.GetByteCount(value);
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    byte[] valueBytes = Encoding.UTF8.GetBytes(value);
                    Marshal.Copy(valueBytes, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetStreamTotals(
            IntPtr ptrResultHandle,
            int stream,
            long* totalRecordCount,
            long* droppedRecordCount,
            FfiCallResult* result)
        {
            if (totalRecordCount == null || droppedRecordCount == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream totals output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiStreamSnapshot snapshot = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream);
                *totalRecordCount = snapshot.TotalRecordCount;
                *droppedRecordCount = snapshot.DroppedRecordCount;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetStreamRecordProjectionInfo(
            IntPtr ptrResultHandle,
            int stream,
            int recordIndex,
            int* propertyEntryCount,
            int* droppedPropertyEntryCount,
            int* typeNameCount,
            int* droppedTypeNameCount,
            int* projectionFlags,
            FfiCallResult* result)
        {
            if (propertyEntryCount == null ||
                droppedPropertyEntryCount == null ||
                typeNameCount == null ||
                droppedTypeNameCount == null ||
                projectionFlags == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream projection output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiStreamRecord record = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream).GetRecord(recordIndex);
                *propertyEntryCount = record.PropertyEntryCount;
                *droppedPropertyEntryCount = record.DroppedPropertyEntryCount;
                *typeNameCount = record.TypeNameCount;
                *droppedTypeNameCount = record.DroppedTypeNameCount;
                *projectionFlags = checked((int)(record.Flags &
                    (FfiRecordScalarValuePresent |
                     FfiRecordPropertyBagPresent |
                     FfiRecordPropertyBagTruncated |
                     FfiRecordTypeNamesTruncated |
                     FfiRecordErrorTargetValuePresent)));
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_CopyStreamRecordValue(
            IntPtr ptrResultHandle,
            int stream,
            int recordIndex,
            int valueSlot,
            uint* kind,
            byte* buffer,
            int bufferLength,
            int* requiredLength,
            FfiCallResult* result)
        {
            if (kind == null || requiredLength == null || bufferLength < 0)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation stream value buffer arguments are invalid.");
            }

            IntPtr outputBuffer = (IntPtr)buffer;
            IntPtr outputRequiredLength = (IntPtr)requiredLength;
            return Execute(result, () =>
            {
                FfiStreamRecord record = GetInvocationResultSnapshot(ptrResultHandle).GetStream(stream).GetRecord(recordIndex);
                FfiSnapshotValue value = record.GetValue(valueSlot);
                *kind = value.Kind;
                int required = value.Payload.Length;
                Marshal.WriteInt32(outputRequiredLength, required);
                if (bufferLength < required)
                {
                    throw new BufferTooSmallException();
                }

                if (required > 0)
                {
                    Marshal.Copy(value.Payload, 0, outputBuffer, required);
                }
            }, bufferTooSmallIsSuccess: true);
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiInvocationResult_GetSequenceRecord(
            IntPtr ptrResultHandle,
            int sequenceIndex,
            int* stream,
            int* recordIndex,
            long* sequence,
            FfiCallResult* result)
        {
            if (stream == null || recordIndex == null || sequence == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Invocation sequence record output pointer is null.");
            }

            return Execute(result, () =>
            {
                FfiSequenceRecord record = GetInvocationResultSnapshot(ptrResultHandle).GetSequenceRecord(sequenceIndex);
                *stream = record.Stream;
                *recordIndex = record.RecordIndex;
                *sequence = record.Sequence;
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_Clear(IntPtr ptrHandle, FfiCallResult* result)
        {
            return Execute(result, () =>
            {
                PowerShell ps = GetPowerShell(ptrHandle);
                FfiInvocationResults.TryRemove(ptrHandle, out _);
                FfiInputBuffers.TryRemove(ptrHandle, out _);
                ps.Commands.Clear();
                GetPowerShellPipeline(ptrHandle).ClearSecretBindings();
            });
        }

        [UnmanagedCallersOnly]
        public static unsafe int FfiPowerShell_Stop(IntPtr ptrHandle, FfiCallResult* result)
        {
            return Execute(result, () => GetPowerShell(ptrHandle).Stop());
        }

        private enum FfiValueKind : uint
        {
            Null = 0,
            StringUtf8 = 1,
            Switch = 2,
            Boolean = 3,
            SignedInteger = 4,
            UnsignedInteger = 5,
            Double = 6,
            DecimalUtf8 = 7,
            Bytes = 8,
            DateTime = 9,
            DateTimeOffset = 10,
            GuidUtf8 = 11,
            UriUtf8 = 12,
            Array = 13,
            PropertyBag = 14,
            SecretUtf16 = 15,
            Credential = 16,
            TimeSpan = 17,
        }

        private sealed class FfiInputBuffer
        {
            public object Gate { get; } = new object();

            public List<object> Values { get; } = new List<object>(FfiMaxInputValues);

            public int PayloadLength { get; set; }

            public bool IsCompleted { get; set; }
        }

        [GeneratedComClass]
        internal sealed partial class FfiLiveObjectProbeBroker : IPowerShellLiveObjectProbe
        {
            private const int EFail = unchecked((int)0x80004005);
            private readonly object gate = new object();

            public FfiLiveObjectProbeBroker(PSObject value)
            {
                Value = value ?? throw new ArgumentNullException(nameof(value));
            }

            public PSObject Value { get; }

            public int GetCount(out long count)
            {
                lock (gate)
                {
                    return TryGetCount(out count) ? FfiStatusSuccess : EFail;
                }
            }

            public int Increment(out long count)
            {
                lock (gate)
                {
                    if (!TryGetCount(out count) || count == long.MaxValue)
                    {
                        return EFail;
                    }

                    count++;
                    Value.Properties["Count"].Value = count;
                    return FfiStatusSuccess;
                }
            }

            private bool TryGetCount(out long count)
            {
                if (Value.Properties["Count"]?.Value is long value)
                {
                    count = value;
                    return true;
                }

                count = default;
                return false;
            }
        }

        private sealed class FfiLiveObjectProbeEntry
        {
            private readonly WeakReference<FfiLiveObjectProbeBroker> broker;
            private int transitReferenceActive = 1;

            public FfiLiveObjectProbeEntry(FfiLiveObjectProbeBroker broker)
            {
                this.broker = new WeakReference<FfiLiveObjectProbeBroker>(broker);
            }

            public bool TryGetBroker(out FfiLiveObjectProbeBroker value)
            {
                return broker.TryGetTarget(out value);
            }

            public bool TryReleaseTransitReference()
            {
                return Interlocked.Exchange(ref transitReferenceActive, 0) == 1;
            }

            public bool IsTransitReferenceReleased => Volatile.Read(ref transitReferenceActive) == 0;
        }

        private static IntPtr ExportLiveObjectProbe(PSObject value)
        {
            var broker = new FfiLiveObjectProbeBroker(value);
            IntPtr ptrComObject = FfiLiveObjectComWrappers.GetOrCreateComInterfaceForObject(
                broker,
                CreateComInterfaceFlags.None);
            if (ptrComObject == IntPtr.Zero)
            {
                throw new InvalidOperationException("Live object probe did not create an IUnknown pointer.");
            }

            var entry = new FfiLiveObjectProbeEntry(broker);
            while (!FfiLiveObjectProbes.TryAdd(ptrComObject, entry))
            {
                if (FfiLiveObjectProbes.TryGetValue(ptrComObject, out FfiLiveObjectProbeEntry existing) &&
                    existing.TryGetBroker(out _))
                {
                    ReleaseComInterface(ptrComObject);
                    throw new InvalidOperationException("Live object probe pointer collision.");
                }

                FfiLiveObjectProbes.TryRemove(ptrComObject, out _);
            }

            return ptrComObject;
        }

        private static FfiLiveObjectProbeBroker GetLiveObjectProbe(IntPtr ptrComObject)
        {
            if (ptrComObject == IntPtr.Zero ||
                !FfiLiveObjectProbes.TryGetValue(ptrComObject, out FfiLiveObjectProbeEntry entry) ||
                !entry.TryGetBroker(out FfiLiveObjectProbeBroker broker))
            {
                FfiLiveObjectProbes.TryRemove(ptrComObject, out _);
                throw new InvalidOperationException("Live object probe pointer is invalid or no longer alive.");
            }

            return broker;
        }

        private static unsafe void ReleaseComInterface(IntPtr ptrComObject)
        {
            if (ptrComObject == IntPtr.Zero)
            {
                throw new InvalidOperationException("Live object probe pointer is null.");
            }

            IntPtr* vtable = *(IntPtr**)ptrComObject;
            if (vtable == null || vtable[2] == IntPtr.Zero)
            {
                throw new InvalidOperationException("Live object probe pointer has an invalid IUnknown vtable.");
            }

            var release = (delegate* unmanaged[MemberFunction]<IntPtr, uint>)vtable[2];
            _ = release(ptrComObject);
        }

        private sealed class FfiPowerShellPipeline : IDisposable
        {
            private int disposed;
            private FfiCapabilityContext capabilityContext;
            private FfiBrokerContext brokerContext;
            private FfiBridgeContext bridgeContext;
            private readonly List<SecureString> secretBindings = new();

            public FfiPowerShellPipeline(PowerShell powerShell, FfiPowerShellSession session)
            {
                PowerShell = powerShell;
                Session = session;
            }

            public PowerShell PowerShell { get; }

            public FfiPowerShellSession Session { get; }

            public bool HasSecretBindings => secretBindings.Count != 0;

            public void AddSecretBinding(SecureString value)
            {
                ArgumentNullException.ThrowIfNull(value);
                if (Volatile.Read(ref disposed) != 0)
                {
                    throw new ObjectDisposedException(nameof(FfiPowerShellPipeline));
                }

                secretBindings.Add(value);
            }

            public object CreatePayloadCredential(string userName, SecureString secret)
            {
                ArgumentException.ThrowIfNullOrWhiteSpace(userName);
                ArgumentNullException.ThrowIfNull(secret);

                return Session is null
                    ? new PSCredential(userName, secret)
                    : Session.CreatePayloadCredential(userName, secret);
            }

            public void ThrowIfSecretBound()
            {
                if (HasSecretBindings)
                {
                    throw new InvalidOperationException(
                        "Secret-bound pipelines must use the explicit secret result invocation API.");
                }
            }

            public void ClearSecretBindings()
            {
                foreach (SecureString secret in secretBindings)
                {
                    secret.Dispose();
                }

                secretBindings.Clear();
            }

            public void DisposeSecretResult(SecureString value)
            {
                if (value is null)
                {
                    return;
                }

                foreach (SecureString binding in secretBindings)
                {
                    if (ReferenceEquals(binding, value))
                    {
                        return;
                    }
                }

                value.Dispose();
            }

            public void SetCapabilityContext(FfiCapabilityContext value)
            {
                if (Volatile.Read(ref disposed) != 0)
                {
                    throw new ObjectDisposedException(nameof(FfiPowerShellPipeline));
                }

                capabilityContext = value ?? throw new ArgumentNullException(nameof(value));
            }

            public FfiCapabilityContext TakeCapabilityContext()
            {
                FfiCapabilityContext value = capabilityContext;
                capabilityContext = null;
                return value;
            }

            public void ClearCapabilityContext()
            {
                capabilityContext = null;
            }

            public void SetBrokerContext(FfiBrokerContext value)
            {
                if (Volatile.Read(ref disposed) != 0)
                {
                    throw new ObjectDisposedException(nameof(FfiPowerShellPipeline));
                }

                brokerContext = value ?? throw new ArgumentNullException(nameof(value));
            }

            public FfiBrokerContext TakeBrokerContext()
            {
                FfiBrokerContext value = brokerContext;
                brokerContext = null;
                return value;
            }

            public FfiBrokerContext GetBrokerContext() => brokerContext;

            public void ClearBrokerContext()
            {
                brokerContext = null;
            }

            public void SetBridgeContext(FfiBridgeContext value)
            {
                if (Volatile.Read(ref disposed) != 0)
                {
                    throw new ObjectDisposedException(nameof(FfiPowerShellPipeline));
                }

                bridgeContext?.Dispose();
                bridgeContext = value ?? throw new ArgumentNullException(nameof(value));
            }

            public FfiBridgeContext TakeBridgeContext()
            {
                FfiBridgeContext value = bridgeContext;
                bridgeContext = null;
                return value;
            }

            public void ClearBridgeContext()
            {
                FfiBridgeContext value = bridgeContext;
                bridgeContext = null;
                value?.Dispose();
            }

            public void Dispose()
            {
                if (Interlocked.Exchange(ref disposed, 1) != 0)
                {
                    return;
                }

                try
                {
                    ClearBridgeContext();
                    ClearSecretBindings();
                    PowerShell.Dispose();
                }
                finally
                {
                    Session?.ReleasePipeline();
                }
            }
        }

        private sealed class FfiPowerShellSession
        {
            private const uint CurrentRunspace = 0;
            private const uint NewRunspace = 1;
            private const uint DefaultConfiguration = 0;
            private const uint ConstrainedLanguageConfiguration = 1;
            private const uint HistoryDisabled = 0;
            private const uint HistoryEnabled = 1;
            private const uint PreferenceInherit = 0;
            private const uint PreferenceContinue = 1;
            private const uint PreferenceSilentlyContinue = 2;
            private const uint PreferenceStop = 3;
            private const uint StateOpened = 1;
            private const uint StateRunning = 2;
            private const uint StateClosed = 3;
            private const uint StateFaulted = 4;
            private const uint EventsTruncated = 1;
            private const string CreateCredentialScript =
                "param([string]$UserName, [System.Security.SecureString]$Secret) " +
                "[System.Management.Automation.PSCredential]::new($UserName, $Secret)";

            private readonly object gate = new object();
            private readonly Runspace runspace;
            private readonly bool ownsRunspace;
            private readonly bool addToHistory;
            private readonly uint errorPreference;
            private readonly List<FfiSessionEvent> events = new List<FfiSessionEvent>(FfiMaxSessionEvents);
            private readonly Dictionary<string, FfiLiveObjectLease> liveObjectVariables =
                new Dictionary<string, FfiLiveObjectLease>(StringComparer.OrdinalIgnoreCase);
            private int leaseCount = 1;
            private int activePipelineCount;
            private long eventSequence;
            private long invocationCount;
            private long historyCount;
            private bool ownerReleased;
            private bool eventsTruncated;
            private bool disposed;

            private sealed class FfiApprovedModuleAuthorizationManager : AuthorizationManager
            {
                private readonly string[] approvedModuleRoots;

                public FfiApprovedModuleAuthorizationManager(string[] approvedModuleRoots)
                    : base("Devolutions.PowerShell.Ffi")
                {
                    this.approvedModuleRoots = approvedModuleRoots;
                }

                protected override bool ShouldRun(CommandInfo commandInfo, CommandOrigin origin, PSHost host, out Exception reason)
                {
                    if (commandInfo is ExternalScriptInfo script)
                    {
                        if (approvedModuleRoots.Any(root => IsBeneathRoot(root, script.Path)))
                        {
                            reason = null;
                            return true;
                        }

                        reason = new PSSecurityException(
                            "External scripts are allowed only from approved staged module roots.");
                        return false;
                    }

                    return base.ShouldRun(commandInfo, origin, host, out reason);
                }

                private static bool IsBeneathRoot(string root, string path)
                {
                    try
                    {
                        string relative = Path.GetRelativePath(root, Path.GetFullPath(path));
                        return relative.Length != 0 &&
                            !Path.IsPathFullyQualified(relative) &&
                            relative != ".." &&
                            !relative.StartsWith($"..{Path.DirectorySeparatorChar}", StringComparison.Ordinal);
                    }
                    catch (ArgumentException)
                    {
                        return false;
                    }
                }
            }

            private FfiPowerShellSession(Runspace runspace, bool ownsRunspace, bool addToHistory, uint errorPreference)
            {
                this.runspace = runspace;
                this.ownsRunspace = ownsRunspace;
                this.addToHistory = addToHistory;
                this.errorPreference = errorPreference;
                AddEventLocked(StateOpened);
            }

            public static FfiPowerShellSession Create(
                uint runspaceMode,
                uint initialConfiguration,
                uint historyMode,
                uint errorPreference,
                uint warningPreference,
                uint verbosePreference,
                uint debugPreference,
                uint informationPreference,
                string allowedModulePath)
            {
                return CreateConfigured(
                    runspaceMode,
                    initialConfiguration,
                    historyMode,
                    errorPreference,
                    warningPreference,
                    verbosePreference,
                    debugPreference,
                    informationPreference,
                    executionPolicy: 0,
                    new PSObject(),
                    Array.Empty<string>(),
                    string.IsNullOrEmpty(allowedModulePath) ? Array.Empty<string>() : [allowedModulePath],
                    string.Empty,
                    new PSObject());
            }

            public static FfiPowerShellSession CreateConfigured(
                uint runspaceMode,
                uint initialConfiguration,
                uint historyMode,
                uint errorPreference,
                uint warningPreference,
                uint verbosePreference,
                uint debugPreference,
                uint informationPreference,
                uint executionPolicy,
                PSObject initialVariables,
                string[] moduleImports,
                string[] allowedModulePaths,
                string workingDirectory,
                PSObject environment)
            {
                ValidateConfigurationInputs(
                    runspaceMode,
                    initialConfiguration,
                    historyMode,
                    errorPreference,
                    warningPreference,
                    verbosePreference,
                    debugPreference,
                    informationPreference,
                    executionPolicy,
                    initialVariables,
                    moduleImports,
                    allowedModulePaths,
                    workingDirectory,
                    environment);

                if (runspaceMode == CurrentRunspace)
                {
                    if (HasCurrentRunspaceConfiguration(
                        initialConfiguration,
                        historyMode,
                        errorPreference,
                        warningPreference,
                        verbosePreference,
                        debugPreference,
                        informationPreference,
                        executionPolicy,
                        initialVariables,
                        moduleImports,
                        allowedModulePaths,
                        workingDirectory,
                        environment))
                    {
                        throw new InvalidOperationException(
                            "Current-runspace sessions cannot change configuration, history, preferences, variables, imports, paths, working directory, or environment.");
                    }

                    Runspace current = Runspace.DefaultRunspace;
                    if (current is null || current.RunspaceStateInfo.State != RunspaceState.Opened)
                    {
                        throw new InvalidOperationException("No opened current default runspace is available.");
                    }

                    return new FfiPowerShellSession(
                        current,
                        ownsRunspace: false,
                        addToHistory: false,
                        errorPreference: PreferenceInherit);
                }

                InitialSessionState initialState = InitialSessionState.CreateDefault2();
                initialState.Commands.Add(new SessionStateCmdletEntry(
                    "ConvertTo-SecureString",
                    typeof(Microsoft.PowerShell.Commands.ConvertToSecureStringCommand),
                    null));
                if (initialConfiguration == ConstrainedLanguageConfiguration)
                {
                    initialState.LanguageMode = PSLanguageMode.ConstrainedLanguage;
                }
                if (executionPolicy == 1)
                {
                    initialState.ExecutionPolicy = Microsoft.PowerShell.ExecutionPolicy.Restricted;
                }
                string[] fullModulePaths = NormalizeDirectories(allowedModulePaths, "Allowed module path");
                if (fullModulePaths.Length != 0)
                {
                    // The application supplies only bounded module roots. This authorization
                    // manager permits external scripts only beneath those roots.
                    initialState.AuthorizationManager = new FfiApprovedModuleAuthorizationManager(fullModulePaths);
                }
                foreach (string moduleImport in moduleImports)
                {
                    initialState.ImportPSModule(ResolveModuleImport(fullModulePaths, moduleImport));
                }

                Runspace runspace = RunspaceFactory.CreateRunspace(initialState);
                try
                {
                    runspace.Open();
                    if (fullModulePaths.Length != 0)
                    {
                        runspace.SessionStateProxy.SetVariable("env:PSModulePath", string.Join(Path.PathSeparator, fullModulePaths));
                    }
                    foreach (PSPropertyInfo variable in initialVariables.Properties)
                    {
                        runspace.SessionStateProxy.SetVariable(variable.Name, variable.Value);
                    }
                    foreach (PSPropertyInfo variable in environment.Properties)
                    {
                        if (variable.Value is not string value)
                        {
                            throw new InvalidOperationException("Session environment values must be strings.");
                        }
                        runspace.SessionStateProxy.SetVariable($"env:{variable.Name}", value);
                    }
                    if (!string.IsNullOrEmpty(workingDirectory))
                    {
                        string fullWorkingDirectory = NormalizeDirectories(new[] { workingDirectory }, "Working directory")[0];
                        runspace.SessionStateProxy.Path.SetLocation(fullWorkingDirectory);
                    }

                    ApplyPreference(runspace, "ErrorActionPreference", errorPreference);
                    ApplyPreference(runspace, "WarningPreference", warningPreference);
                    ApplyPreference(runspace, "VerbosePreference", verbosePreference);
                    ApplyPreference(runspace, "DebugPreference", debugPreference);
                    ApplyPreference(runspace, "InformationPreference", informationPreference);
                    return new FfiPowerShellSession(
                        runspace,
                        ownsRunspace: true,
                        addToHistory: historyMode == HistoryEnabled,
                        errorPreference: errorPreference);
                }
                catch
                {
                    runspace.Dispose();
                    throw;
                }
            }

            public static FfiSessionPreflightPayload PreflightConfigured(
                uint runspaceMode,
                uint initialConfiguration,
                uint historyMode,
                uint errorPreference,
                uint warningPreference,
                uint verbosePreference,
                uint debugPreference,
                uint informationPreference,
                uint executionPolicy,
                PSObject initialVariables,
                string[] moduleImports,
                string[] allowedModulePaths,
                string workingDirectory,
                PSObject environment)
            {
                try
                {
                    ValidateConfigurationInputs(
                        runspaceMode,
                        initialConfiguration,
                        historyMode,
                        errorPreference,
                        warningPreference,
                        verbosePreference,
                        debugPreference,
                        informationPreference,
                        executionPolicy,
                        initialVariables,
                        moduleImports,
                        allowedModulePaths,
                        workingDirectory,
                        environment);
                }
                catch (InvalidOperationException)
                {
                    return new FfiSessionPreflightPayload(
                        FfiPreflightInvalidConfiguration,
                        "PowerShell session configuration is invalid.",
                        Array.Empty<FfiModuleRootResolution>(),
                        Array.Empty<FfiModuleImportResolution>());
                }

                if (runspaceMode == CurrentRunspace)
                {
                    return HasCurrentRunspaceConfiguration(
                        initialConfiguration,
                        historyMode,
                        errorPreference,
                        warningPreference,
                        verbosePreference,
                        debugPreference,
                        informationPreference,
                        executionPolicy,
                        initialVariables,
                        moduleImports,
                        allowedModulePaths,
                        workingDirectory,
                        environment)
                        ? new FfiSessionPreflightPayload(
                            FfiPreflightInvalidConfiguration,
                            "Current-runspace sessions cannot change configuration.",
                            Array.Empty<FfiModuleRootResolution>(),
                            Array.Empty<FfiModuleImportResolution>())
                        : new FfiSessionPreflightPayload(
                            FfiPreflightValid,
                            string.Empty,
                            Array.Empty<FfiModuleRootResolution>(),
                            Array.Empty<FfiModuleImportResolution>());
                }

                FfiModuleRootResolution[] moduleRoots = ResolveModuleRoots(allowedModulePaths, "Allowed module path");
                FfiModuleImportResolution[] moduleImportDiagnostics = moduleImports
                    .Select(moduleImport => ResolveModuleImport(moduleRoots, moduleImport))
                    .ToArray();
                if (moduleRoots.Any(root => root.Status != FfiModuleRootValid))
                {
                    return new FfiSessionPreflightPayload(
                        FfiPreflightInvalidModuleRoots,
                        string.Empty,
                        moduleRoots,
                        moduleImportDiagnostics);
                }

                if (!string.IsNullOrEmpty(workingDirectory))
                {
                    FfiModuleRootResolution workingDirectoryResolution =
                        ResolveModuleRoots(new[] { workingDirectory }, "Working directory")[0];
                    if (workingDirectoryResolution.Status != FfiModuleRootValid)
                    {
                        return new FfiSessionPreflightPayload(
                            FfiPreflightInvalidWorkingDirectory,
                            "Working directory must be an existing absolute directory.",
                            moduleRoots,
                            moduleImportDiagnostics);
                    }
                }

                uint status = moduleImportDiagnostics.Any(import => import.Status == FfiModuleImportUnresolvable)
                    ? FfiPreflightUnresolvableModuleImports
                    : moduleImportDiagnostics.Any(import =>
                        import.Status is FfiModuleImportManifestInvalid or FfiModuleImportManifestUnreadable)
                        ? FfiPreflightInvalidModuleManifest
                        : moduleImportDiagnostics.Any(import => import.Status == FfiModuleImportManifestDeclaresExternalPath)
                            ? FfiPreflightExternalModuleDeclarations
                            : FfiPreflightValid;
                return new FfiSessionPreflightPayload(
                    status,
                    string.Empty,
                    moduleRoots,
                    moduleImportDiagnostics);
            }

            private static void ValidateConfigurationInputs(
                uint runspaceMode,
                uint initialConfiguration,
                uint historyMode,
                uint errorPreference,
                uint warningPreference,
                uint verbosePreference,
                uint debugPreference,
                uint informationPreference,
                uint executionPolicy,
                PSObject initialVariables,
                string[] moduleImports,
                string[] allowedModulePaths,
                string workingDirectory,
                PSObject environment)
            {
                if (runspaceMode is not (CurrentRunspace or NewRunspace))
                {
                    throw new InvalidOperationException("PowerShell session runspace mode is invalid.");
                }
                if (initialConfiguration is not (DefaultConfiguration or ConstrainedLanguageConfiguration) ||
                    historyMode is not (HistoryDisabled or HistoryEnabled) ||
                    executionPolicy is > 1)
                {
                    throw new InvalidOperationException("PowerShell session configuration is invalid.");
                }
                ArgumentNullException.ThrowIfNull(initialVariables);
                ArgumentNullException.ThrowIfNull(moduleImports);
                ArgumentNullException.ThrowIfNull(allowedModulePaths);
                ArgumentNullException.ThrowIfNull(workingDirectory);
                ArgumentNullException.ThrowIfNull(environment);
                ValidatePreference(errorPreference, "error");
                ValidatePreference(warningPreference, "warning");
                ValidatePreference(verbosePreference, "verbose");
                ValidatePreference(debugPreference, "debug");
                ValidatePreference(informationPreference, "information");
            }

            private static bool HasCurrentRunspaceConfiguration(
                uint initialConfiguration,
                uint historyMode,
                uint errorPreference,
                uint warningPreference,
                uint verbosePreference,
                uint debugPreference,
                uint informationPreference,
                uint executionPolicy,
                PSObject initialVariables,
                string[] moduleImports,
                string[] allowedModulePaths,
                string workingDirectory,
                PSObject environment)
            {
                return initialConfiguration != DefaultConfiguration ||
                    historyMode != HistoryDisabled ||
                    errorPreference != PreferenceInherit ||
                    warningPreference != PreferenceInherit ||
                    verbosePreference != PreferenceInherit ||
                    debugPreference != PreferenceInherit ||
                    informationPreference != PreferenceInherit ||
                    executionPolicy != 0 ||
                    initialVariables.Properties.Count() != 0 ||
                    moduleImports.Length != 0 ||
                    allowedModulePaths.Length != 0 ||
                    !string.IsNullOrEmpty(workingDirectory) ||
                    environment.Properties.Count() != 0;
            }

            public FfiPowerShellPipeline CreatePipeline()
            {
                lock (gate)
                {
                    if (ownerReleased || disposed)
                    {
                        throw new InvalidOperationException("PowerShell session has been released.");
                    }

                    leaseCount++;
                    try
                    {
                        var powerShell = PowerShell.Create();
                        powerShell.Runspace = runspace;
                        return new FfiPowerShellPipeline(powerShell, this);
                    }
                    catch
                    {
                        leaseCount--;
                        throw;
                    }
                }
            }

            public object CreatePayloadCredential(string userName, SecureString secret)
            {
                ArgumentException.ThrowIfNullOrWhiteSpace(userName);
                ArgumentNullException.ThrowIfNull(secret);

                lock (gate)
                {
                    if (ownerReleased || disposed)
                    {
                        throw new InvalidOperationException("PowerShell session has been released.");
                    }

                    // The runspace creates this fixed, non-interpolated credential so its
                    // SMA type identity always matches the target pipeline's binder.
                    using var builder = PowerShell.Create();
                    builder.Runspace = runspace;
                    builder
                        .AddScript(CreateCredentialScript, useLocalScope: true)
                        .AddParameter("UserName", userName)
                        .AddParameter("Secret", secret);

                    Collection<PSObject> output = builder.Invoke();
                    if (builder.HadErrors || output.Count != 1 || output[0]?.BaseObject is null)
                    {
                        throw new InvalidOperationException("Payload credential construction failed.");
                    }

                    return output[0].BaseObject;
                }
            }

            public void BeginInvocation()
            {
                lock (gate)
                {
                    if (disposed)
                    {
                        throw new InvalidOperationException("PowerShell session has been closed.");
                    }

                    var bound = new List<(FfiLiveObjectLease Lease, object Previous)>();
                    try
                    {
                        foreach (FfiLiveObjectLease lease in GetInvocationBoundLeasesLocked())
                        {
                            object previous = lease.Value;
                            object current = lease.BeginInvocationBinding();
                            bound.Add((lease, previous));
                            ReplaceLiveObjectReferencesLocked(previous, current);
                        }
                    }
                    catch
                    {
                        for (int index = bound.Count - 1; index >= 0; index--)
                        {
                            (FfiLiveObjectLease lease, object previous) = bound[index];
                            object current = TryReadLeaseValue(lease);
                            try
                            {
                                lease.EndInvocationBinding();
                            }
                            finally
                            {
                                object unbound = TryReadLeaseValue(lease);
                                if (current is not null && unbound is not null)
                                {
                                    ReplaceLiveObjectReferencesLocked(current, unbound);
                                }
                            }
                        }

                        throw;
                    }

                    activePipelineCount++;
                    AddEventLocked(StateRunning);
                }
            }

            public void EndInvocation(bool faulted)
            {
                lock (gate)
                {
                    Exception failure = null;
                    activePipelineCount = Math.Max(0, activePipelineCount - 1);
                    foreach (FfiLiveObjectLease lease in GetInvocationBoundLeasesLocked())
                    {
                        object previous = TryReadLeaseValue(lease);
                        try
                        {
                            lease.EndInvocationBinding();
                        }
                        catch (Exception exception)
                        {
                            failure ??= exception;
                        }
                        finally
                        {
                            object current = TryReadLeaseValue(lease);
                            if (previous is not null && current is not null)
                            {
                                try
                                {
                                    ReplaceLiveObjectReferencesLocked(previous, current);
                                }
                                catch (Exception exception)
                                {
                                    failure ??= exception;
                                }
                            }
                        }
                    }

                    try
                    {
                        ReconcileLiveObjectVariablesLocked();
                    }
                    catch (Exception exception)
                    {
                        failure ??= exception;
                    }

                    invocationCount++;
                    if (addToHistory)
                    {
                        historyCount++;
                    }

                    AddEventLocked(faulted ? StateFaulted : StateOpened);
                    if (failure is not null)
                    {
                        throw failure;
                    }
                }
            }

            public PSInvocationSettings CreateInvocationSettings()
            {
                if (!addToHistory && errorPreference == PreferenceInherit)
                {
                    return null;
                }

                var settings = new PSInvocationSettings
                {
                    AddToHistory = addToHistory,
                };
                if (errorPreference != PreferenceInherit)
                {
                    settings.ErrorActionPreference = ToActionPreference(errorPreference);
                }

                return settings;
            }

            public FfiSessionSnapshot Snapshot()
            {
                lock (gate)
                {
                    uint state = disposed
                        ? StateClosed
                        : activePipelineCount > 0
                            ? StateRunning
                            : MapRunspaceState(runspace.RunspaceStateInfo.State);
                    return new FfiSessionSnapshot(
                        state,
                        MapRunspaceState(runspace.RunspaceStateInfo.State),
                        eventsTruncated ? EventsTruncated : 0,
                        checked((uint)activePipelineCount),
                        checked((uint)events.Count),
                        invocationCount,
                        historyCount);
                }
            }

            public FfiSessionEvent GetEvent(int eventIndex)
            {
                lock (gate)
                {
                    if (eventIndex < 0 || eventIndex >= events.Count)
                    {
                        throw new InvalidOperationException("PowerShell session event index is invalid.");
                    }

                    return events[eventIndex];
                }
            }

            public void SetVariable(string name, object value)
            {
                lock (gate)
                {
                    EnsureVariableMutationAllowed();
                    ValidateVariableName(name);
                    runspace.SessionStateProxy.SetVariable(name, value);
                    ReconcileLiveObjectVariablesLocked();
                }
            }

            public void SetLiveObjectVariable(string name, IntPtr ptrComObject)
            {
                SetLiveObjectVariable(name, FfiLiveObjectProbeContract, ptrComObject);
            }

            public void SetLiveObjectVariable(
                string name,
                PowerShellLiveObjectContract contract,
                IntPtr ptrComObject)
            {
                if (ptrComObject == IntPtr.Zero)
                {
                    throw new InvalidOperationException("Live session object pointer is null.");
                }

                if ((contract.Directions & PowerShellLiveObjectDirection.BridgeContract) != 0)
                {
                    lock (gate)
                    {
                        EnsureVariableMutationAllowed();
                        ValidateVariableName(name);
                        FfiBridgeContractLease shared = FindBridgeLeaseLocked(contract, ptrComObject);
                        if (shared is not null)
                        {
                            runspace.SessionStateProxy.SetVariable(name, shared.Value);
                            ReconcileLiveObjectVariablesLocked();
                            return;
                        }
                    }
                }

                FfiLiveObjectLease value = FfiLiveObjectContracts.CreateLease(contract, ptrComObject);

                lock (gate)
                {
                    try
                    {
                        EnsureVariableMutationAllowed();
                        ValidateVariableName(name);
                        if (value is FfiBridgeContractLease)
                        {
                            FfiBridgeContractLease shared = FindBridgeLeaseLocked(contract, ptrComObject);
                            if (shared is not null)
                            {
                                value.Dispose();
                                runspace.SessionStateProxy.SetVariable(name, shared.Value);
                                ReconcileLiveObjectVariablesLocked();
                                return;
                            }
                        }

                        runspace.SessionStateProxy.SetVariable(name, value.Value);
                    }
                    catch
                    {
                        value.Dispose();
                        throw;
                    }

                    try
                    {
                        ReconcileLiveObjectVariablesLocked();
                        liveObjectVariables.Add(name, value);
                    }
                    catch
                    {
                        // The variable is already published at this point, so the
                        // lease is reachable from script while being owned by
                        // nobody: it is not in the table, so neither reconciliation
                        // nor session teardown would ever release it. Take it back
                        // out of reach first, then release it.
                        UnpublishLiveObjectVariableLocked(name, value);
                        value.Dispose();
                        throw;
                    }
                }
            }

            /// <summary>
            /// Removes a variable that was published for a lease whose adoption then
            /// failed. Nothing here may throw: it runs while an earlier failure is
            /// already propagating, and replacing that failure would hide the reason
            /// the lease is being unwound at all.
            /// </summary>
            private void UnpublishLiveObjectVariableLocked(string name, FfiLiveObjectLease lease)
            {
                try
                {
                    object published = lease.Value;
                    PSVariable variable = runspace.SessionStateProxy.PSVariable.Get(name);

                    // Only reclaim the name if it still holds this lease's value.
                    // Anything else means something took the name over, and
                    // removing it would destroy an unrelated variable.
                    if (variable is not null && ReferenceEquals(variable.Value, published))
                    {
                        runspace.SessionStateProxy.PSVariable.Remove(name);
                    }
                }
                catch
                {
                    // Best effort by construction.
                }
            }

            public bool RemoveVariable(string name)
            {
                lock (gate)
                {
                    EnsureVariableMutationAllowed();
                    ValidateVariableName(name);
                    PSVariable variable = runspace.SessionStateProxy.PSVariable.Get(name);
                    if (variable is null)
                    {
                        return false;
                    }

                    runspace.SessionStateProxy.PSVariable.Remove(name);
                    ReconcileLiveObjectVariablesLocked();
                    return true;
                }
            }

            public bool TryGetVariableSnapshot(string name, out FfiSnapshotValue snapshot)
            {
                lock (gate)
                {
                    EnsureVariableMutationAllowed();
                    ValidateVariableName(name);
                    PSVariable variable = runspace.SessionStateProxy.PSVariable.Get(name);
                    if (variable is null)
                    {
                        snapshot = null;
                        return false;
                    }

                    if (!FfiSnapshotCollector.TryEncodeCopiedValue(variable.Value, depth: 0, out snapshot))
                    {
                        throw new FfiUnsupportedValueException(
                            "The requested session variable cannot be represented as a bounded copied PowerShell value.");
                    }

                    return true;
                }
            }

            public void ReleaseOwner()
            {
                lock (gate)
                {
                    if (ownerReleased)
                    {
                        throw new InvalidOperationException("PowerShell session has already been released.");
                    }

                    ownerReleased = true;
                    ReleaseLeaseLocked();
                }
            }

            public void ReleasePipeline()
            {
                lock (gate)
                {
                    ReleaseLeaseLocked();
                }
            }

            private void EnsureVariableMutationAllowed()
            {
                if (ownerReleased || disposed)
                {
                    throw new InvalidOperationException("PowerShell session has been released.");
                }

                if (activePipelineCount != 0)
                {
                    throw new FfiInputBackpressureException(
                        "Session variables cannot be read or changed while a PowerShell invocation is pending or running.");
                }
            }

            private static void ValidateVariableName(string value)
            {
                if (string.IsNullOrEmpty(value) ||
                    value.Length > 64 ||
                    (!IsAsciiLetter(value[0]) && value[0] != '_') ||
                    !value.All(character => IsAsciiLetter(character) || IsAsciiDigit(character) || character == '_'))
                {
                    throw new InvalidOperationException("Session variable names must be bounded ASCII identifiers.");
                }
            }

            private static bool IsAsciiLetter(char value)
            {
                return value is >= 'A' and <= 'Z' or >= 'a' and <= 'z';
            }

            private static bool IsAsciiDigit(char value)
            {
                return value is >= '0' and <= '9';
            }

            private static void ValidatePreference(uint preference, string name)
            {
                if (preference is not (PreferenceInherit or PreferenceContinue or PreferenceSilentlyContinue or PreferenceStop))
                {
                    throw new InvalidOperationException(
                        $"PowerShell session {name} preference is unsupported; only inherit, continue, silently continue, and stop are allowed.");
                }
            }

            private static void ApplyPreference(Runspace runspace, string variable, uint preference)
            {
                if (preference == PreferenceInherit)
                {
                    return;
                }

                runspace.SessionStateProxy.SetVariable(variable, ToActionPreference(preference));
            }

            private static ActionPreference ToActionPreference(uint preference)
            {
                return preference switch
                {
                    PreferenceContinue => ActionPreference.Continue,
                    PreferenceSilentlyContinue => ActionPreference.SilentlyContinue,
                    PreferenceStop => ActionPreference.Stop,
                    _ => throw new InvalidOperationException("PowerShell session preference is invalid."),
                };
            }

            private static uint MapRunspaceState(RunspaceState state)
            {
                return state switch
                {
                    RunspaceState.Opened => StateOpened,
                    RunspaceState.Opening => StateRunning,
                    RunspaceState.Closing or RunspaceState.Closed => StateClosed,
                    _ => StateFaulted,
                };
            }

            private void AddEventLocked(uint state)
            {
                if (events.Count == FfiMaxSessionEvents)
                {
                    eventsTruncated = true;
                    return;
                }

                events.Add(new FfiSessionEvent(checked(++eventSequence), state, 0));
            }

            private void ReleaseLeaseLocked()
            {
                if (leaseCount <= 0)
                {
                    throw new InvalidOperationException("PowerShell session lifetime is invalid.");
                }

                leaseCount--;
                if (leaseCount != 0 || disposed)
                {
                    return;
                }

                disposed = true;
                try
                {
                    AddEventLocked(StateClosed);
                    ReleaseAllLiveObjectVariablesLocked();
                }
                finally
                {
                    // The guard above is already set, so a skipped dispose here
                    // leaks the runspace permanently. A failing lease release must
                    // not strand it.
                    if (ownsRunspace)
                    {
                        runspace.Dispose();
                    }
                }
            }

            private void ReleaseLiveObjectVariableLocked(string name)
            {
                if (liveObjectVariables.Remove(name, out FfiLiveObjectLease value))
                {
                    value.Dispose();
                }
            }

            private void ReconcileLiveObjectVariablesLocked()
            {
                PSVariable[] variables = runspace.SessionStateProxy.InvokeProvider.Item
                    .Get("Variable:\\*")
                    .Select(static variable => variable.BaseObject as PSVariable)
                    .Where(static variable => variable is not null)
                    .Select(static variable => variable!)
                    .ToArray();
                var reconciled = new Dictionary<string, FfiLiveObjectLease>(StringComparer.OrdinalIgnoreCase);
                var dropped = new List<FfiLiveObjectLease>();
                foreach (KeyValuePair<string, FfiLiveObjectLease> entry in liveObjectVariables)
                {
                    // A lease whose value cannot be read is unusable, so it is
                    // dropped rather than allowed to abort the walk. Letting it
                    // throw here used to leave already-disposed leases in the
                    // table, and every later reconciliation then threw on the same
                    // entry -- one bad proxy permanently broke the session.
                    object value = TryReadLeaseValue(entry.Value);
                    PSVariable variable = value is null
                        ? null
                        : variables.FirstOrDefault(variable => ReferenceEquals(variable.Value, value));
                    if (variable is not null && !reconciled.ContainsKey(variable.Name))
                    {
                        reconciled.Add(variable.Name, entry.Value);
                    }
                    else
                    {
                        dropped.Add(entry.Value);
                    }
                }

                liveObjectVariables.Clear();
                foreach (KeyValuePair<string, FfiLiveObjectLease> entry in reconciled)
                {
                    liveObjectVariables.Add(entry.Key, entry.Value);
                }

                // Release only once the table is consistent. Disposing during the
                // walk meant a failure part-way left disposed leases still listed
                // as live.
                DisposeAll(dropped);
            }

            private IEnumerable<FfiLiveObjectLease> GetInvocationBoundLeasesLocked()
            {
                var seen = new HashSet<FfiLiveObjectLease>();
                foreach (FfiLiveObjectLease lease in liveObjectVariables.Values)
                {
                    if (lease.RequiresInvocationBinding && seen.Add(lease))
                    {
                        yield return lease;
                    }
                }
            }

            private FfiBridgeContractLease FindBridgeLeaseLocked(
                PowerShellLiveObjectContract contract,
                IntPtr ptrComObject)
            {
                var seen = new HashSet<FfiLiveObjectLease>();
                foreach (FfiLiveObjectLease lease in liveObjectVariables.Values)
                {
                    if (seen.Add(lease) &&
                        lease is FfiBridgeContractLease bridge &&
                        bridge.Matches(contract, ptrComObject))
                    {
                        return bridge;
                    }
                }

                return null;
            }

            private void ReplaceLiveObjectReferencesLocked(object previous, object current)
            {
                if (ReferenceEquals(previous, current))
                {
                    return;
                }

                PSVariable[] variables = runspace.SessionStateProxy.InvokeProvider.Item
                    .Get("Variable:\\*")
                    .Select(static variable => variable.BaseObject as PSVariable)
                    .Where(static variable => variable is not null)
                    .Select(static variable => variable!)
                    .ToArray();
                foreach (PSVariable variable in variables)
                {
                    if (ReferenceEquals(variable.Value, previous))
                    {
                        variable.Value = current;
                    }
                }
            }

            private static object TryReadLeaseValue(FfiLiveObjectLease lease)
            {
                try
                {
                    return lease.Value;
                }
                catch
                {
                    return null;
                }
            }

            /// <summary>
            /// Releases every lease even if one of them throws, then reports the
            /// first failure. Stopping at the first would leak the remainder, which
            /// is how one misbehaving pack could strand every other proxy.
            /// </summary>
            private static void DisposeAll(IReadOnlyList<FfiLiveObjectLease> leases)
            {
                Exception failure = null;
                foreach (FfiLiveObjectLease lease in leases)
                {
                    try
                    {
                        lease.Dispose();
                    }
                    catch (Exception exception)
                    {
                        failure = failure ?? exception;
                    }
                }

                if (failure is not null)
                {
                    throw failure;
                }
            }

            private void ReleaseAllLiveObjectVariablesLocked()
            {
                FfiLiveObjectLease[] values = liveObjectVariables.Values.ToArray();
                liveObjectVariables.Clear();
                DisposeAll(values);
            }
        }

        public sealed class FfiLiveSessionObjectProbeProxy : IDisposable
        {
            private readonly object gate = new object();
            private IPowerShellLiveObjectProbe probe;
            private ComObject comObject;

            private FfiLiveSessionObjectProbeProxy(
                IPowerShellLiveObjectProbe probe,
                ComObject comObject)
            {
                this.probe = probe;
                this.comObject = comObject;
            }

            public long Count => Invoke(static (IPowerShellLiveObjectProbe value, out long count) => value.GetCount(out count));

            public long Increment()
            {
                return Invoke(static (IPowerShellLiveObjectProbe value, out long count) => value.Increment(out count));
            }

            public static FfiLiveSessionObjectProbeProxy Create(IntPtr ptrComObject)
            {
                object projected = FfiLiveObjectComWrappers.GetOrCreateObjectForComInstance(
                    ptrComObject,
                    CreateObjectFlags.UniqueInstance);
                ComObject comObject = projected as ComObject
                    ?? throw new InvalidOperationException("Live session object probe did not create a source-generated COM wrapper.");
                IPowerShellLiveObjectProbe probe = projected as IPowerShellLiveObjectProbe;
                if (probe is null)
                {
                    comObject.FinalRelease();
                    throw new InvalidOperationException("Live session object probe has an unexpected COM contract.");
                }

                return new FfiLiveSessionObjectProbeProxy(probe, comObject);
            }

            public void Dispose()
            {
                lock (gate)
                {
                    ComObject value = comObject;
                    probe = null;
                    comObject = null;
                    value?.FinalRelease();
                }
            }

            private long Invoke(ProbeOperation operation)
            {
                lock (gate)
                {
                    if (probe is null)
                    {
                        throw new ObjectDisposedException(nameof(FfiLiveSessionObjectProbeProxy));
                    }

                    int hresult = operation(probe, out long count);
                    if (hresult != FfiStatusSuccess)
                    {
                        throw new COMException("The .NET session object probe call failed.", hresult);
                    }

                    return count;
                }
            }

            private delegate int ProbeOperation(IPowerShellLiveObjectProbe value, out long count);
        }

        private readonly struct FfiSessionSnapshot
        {
            public FfiSessionSnapshot(
                uint state,
                uint runspaceState,
                uint flags,
                uint activePipelineCount,
                uint eventCount,
                long invocationCount,
                long historyCount)
            {
                State = state;
                RunspaceState = runspaceState;
                Flags = flags;
                ActivePipelineCount = activePipelineCount;
                EventCount = eventCount;
                InvocationCount = invocationCount;
                HistoryCount = historyCount;
            }

            public uint State { get; }
            public uint RunspaceState { get; }
            public uint Flags { get; }
            public uint ActivePipelineCount { get; }
            public uint EventCount { get; }
            public long InvocationCount { get; }
            public long HistoryCount { get; }
        }

        private readonly struct FfiSessionEvent
        {
            public FfiSessionEvent(long sequence, uint state, uint flags)
            {
                Sequence = sequence;
                State = state;
                Flags = flags;
            }

            public long Sequence { get; }
            public uint State { get; }
            public uint Flags { get; }
        }

        private sealed class FfiValueFormatException : InvalidOperationException
        {
            public FfiValueFormatException(string message)
                : base(message)
            {
            }
        }

        private sealed class FfiUnsupportedValueException : InvalidOperationException
        {
            public FfiUnsupportedValueException(string message)
                : base(message)
            {
            }
        }

        private sealed class FfiInputNotCompletedException : InvalidOperationException
        {
            public FfiInputNotCompletedException()
                : base("Input was started but not completed. Call CompleteInput before Invoke.")
            {
            }
        }

        private sealed class FfiInputBackpressureException : InvalidOperationException
        {
            public FfiInputBackpressureException(string message)
                : base(message)
            {
            }
        }

        private static unsafe int ReadValue(uint kind, byte* data, int dataLength, FfiCallResult* result, out object value)
        {
            value = null;
            if (dataLength < 0 || dataLength > FfiMaxValuePayloadLength || (dataLength > 0 && data == null))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Tagged value payload is invalid or exceeds its bound.");
            }

            try
            {
                value = DecodeValue((FfiValueKind)kind, new ReadOnlySpan<byte>(data, dataLength), 0);
                return FfiStatusSuccess;
            }
            catch (FfiUnsupportedValueException exception)
            {
                return WriteFailure(result, FfiStatusUnsupportedValue, exception);
            }
            catch (FfiValueFormatException exception)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, exception);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, exception);
            }
        }

        private static object DecodeValue(FfiValueKind kind, ReadOnlySpan<byte> payload, int depth)
        {
            if (depth > FfiMaxValueDepth)
            {
                throw new FfiValueFormatException("Tagged value nesting exceeds its bound.");
            }

            return kind switch
            {
                FfiValueKind.Null => DecodeNull(payload),
                FfiValueKind.StringUtf8 => DecodeUtf8Value(payload),
                FfiValueKind.Switch => DecodeSwitch(payload),
                FfiValueKind.Boolean => DecodeBoolean(payload),
                FfiValueKind.SignedInteger => DecodeSignedInteger(payload),
                FfiValueKind.UnsignedInteger => DecodeUnsignedInteger(payload),
                FfiValueKind.Double => DecodeDouble(payload),
                FfiValueKind.DecimalUtf8 => DecodeDecimal(payload),
                FfiValueKind.Bytes => payload.ToArray(),
                FfiValueKind.DateTime => DecodeDateTime(payload),
                FfiValueKind.DateTimeOffset => DecodeDateTimeOffset(payload),
                FfiValueKind.TimeSpan => DecodeTimeSpan(payload),
                FfiValueKind.GuidUtf8 => DecodeGuid(payload),
                FfiValueKind.UriUtf8 => DecodeUri(payload),
                FfiValueKind.Array => DecodeArray(payload, depth + 1),
                FfiValueKind.PropertyBag => DecodePropertyBag(payload, depth + 1),
                _ => throw new FfiUnsupportedValueException($"Tagged value kind {(uint)kind} is not supported."),
            };
        }

        private static object DecodeNull(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, 0, "Null");
            return null;
        }

        private static object DecodeSwitch(ReadOnlySpan<byte> payload)
        {
            return new SwitchParameter(DecodeByteBoolean(payload, "Switch"));
        }

        private static object DecodeBoolean(ReadOnlySpan<byte> payload)
        {
            return DecodeByteBoolean(payload, "Boolean");
        }

        private static bool DecodeByteBoolean(ReadOnlySpan<byte> payload, string description)
        {
            RequireLength(payload, 1, description);
            return payload[0] switch
            {
                0 => false,
                1 => true,
                _ => throw new FfiValueFormatException($"{description} payload must be zero or one."),
            };
        }

        private static object DecodeSignedInteger(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(long), "Signed integer");
            return BinaryPrimitives.ReadInt64LittleEndian(payload);
        }

        private static object DecodeUnsignedInteger(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(ulong), "Unsigned integer");
            return BinaryPrimitives.ReadUInt64LittleEndian(payload);
        }

        private static object DecodeDouble(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(long), "Double");
            return BitConverter.Int64BitsToDouble(BinaryPrimitives.ReadInt64LittleEndian(payload));
        }

        private static object DecodeDecimal(ReadOnlySpan<byte> payload)
        {
            string text = DecodeUtf8Value(payload);
            if (!decimal.TryParse(text, NumberStyles.Number, CultureInfo.InvariantCulture, out decimal value))
            {
                throw new FfiValueFormatException("Decimal payload is invalid.");
            }

            return value;
        }

        private static object DecodeDateTime(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(long), "DateTime");
            return DateTime.FromBinary(BinaryPrimitives.ReadInt64LittleEndian(payload));
        }

        private static object DecodeDateTimeOffset(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(long) + sizeof(short), "DateTimeOffset");
            long ticks = BinaryPrimitives.ReadInt64LittleEndian(payload);
            short offsetMinutes = BinaryPrimitives.ReadInt16LittleEndian(payload.Slice(sizeof(long)));
            return new DateTimeOffset(ticks, TimeSpan.FromMinutes(offsetMinutes));
        }

        private static object DecodeTimeSpan(ReadOnlySpan<byte> payload)
        {
            RequireLength(payload, sizeof(long), "TimeSpan");
            return TimeSpan.FromTicks(BinaryPrimitives.ReadInt64LittleEndian(payload));
        }

        private static object DecodeGuid(ReadOnlySpan<byte> payload)
        {
            string text = DecodeUtf8Value(payload);
            if (!Guid.TryParseExact(text, "D", out Guid value))
            {
                throw new FfiValueFormatException("Guid payload must use the canonical D format.");
            }

            return value;
        }

        private static object DecodeUri(ReadOnlySpan<byte> payload)
        {
            string text = DecodeUtf8Value(payload);
            if (!Uri.TryCreate(text, UriKind.Absolute, out Uri value))
            {
                throw new FfiValueFormatException("URI payload must be an absolute URI.");
            }

            return value;
        }

        private static object DecodeArray(ReadOnlySpan<byte> payload, int depth)
        {
            int offset = 0;
            uint count = ReadUInt32(payload, ref offset, "Array count");
            if (count > FfiMaxValueContainerEntries)
            {
                throw new FfiValueFormatException("Array item count exceeds its bound.");
            }

            var values = new object[checked((int)count)];
            for (int index = 0; index < values.Length; index++)
            {
                values[index] = DecodeNestedValue(payload, ref offset, depth);
            }

            RequireComplete(payload, offset, "Array");
            return values;
        }

        private static object DecodePropertyBag(ReadOnlySpan<byte> payload, int depth)
        {
            int offset = 0;
            uint count = ReadUInt32(payload, ref offset, "Property bag count");
            if (count > FfiMaxValueContainerEntries)
            {
                throw new FfiValueFormatException("Property bag entry count exceeds its bound.");
            }

            var propertyBag = new PSObject();
            var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            for (uint index = 0; index < count; index++)
            {
                uint keyLength = ReadUInt32(payload, ref offset, "Property bag key length");
                ReadOnlySpan<byte> keyBytes = ReadBytes(payload, ref offset, keyLength, "Property bag key");
                string name = DecodeUtf8Value(keyBytes);
                if (name.Length == 0 || !names.Add(name))
                {
                    throw new FfiValueFormatException("Property bag keys must be non-empty and unique.");
                }

                propertyBag.Properties.Add(new PSNoteProperty(name, DecodeNestedValue(payload, ref offset, depth)));
            }

            RequireComplete(payload, offset, "Property bag");
            return propertyBag;
        }

        private static object DecodeNestedValue(ReadOnlySpan<byte> payload, ref int offset, int depth)
        {
            uint rawKind = ReadUInt32(payload, ref offset, "Nested value kind");
            uint length = ReadUInt32(payload, ref offset, "Nested value length");
            return DecodeValue((FfiValueKind)rawKind, ReadBytes(payload, ref offset, length, "Nested value"), depth);
        }

        private static uint ReadUInt32(ReadOnlySpan<byte> payload, ref int offset, string description)
        {
            ReadOnlySpan<byte> bytes = ReadBytes(payload, ref offset, sizeof(uint), description);
            return BinaryPrimitives.ReadUInt32LittleEndian(bytes);
        }

        private static ReadOnlySpan<byte> ReadBytes(ReadOnlySpan<byte> payload, ref int offset, uint length, string description)
        {
            if (offset < 0 || offset > payload.Length || length > (uint)(payload.Length - offset))
            {
                throw new FfiValueFormatException($"{description} is truncated.");
            }

            int valueLength = (int)length;
            ReadOnlySpan<byte> value = payload.Slice(offset, valueLength);
            offset += valueLength;
            return value;
        }

        private static string DecodeUtf8Value(ReadOnlySpan<byte> payload)
        {
            try
            {
                if (payload.IndexOf((byte)0) >= 0)
                {
                    throw new FfiValueFormatException("Tagged UTF-8 values cannot contain NUL.");
                }

                return new UTF8Encoding(false, true).GetString(payload);
            }
            catch (DecoderFallbackException exception)
            {
                throw new FfiValueFormatException(exception.Message);
            }
        }

        private static void RequireLength(ReadOnlySpan<byte> payload, int length, string description)
        {
            if (payload.Length != length)
            {
                throw new FfiValueFormatException($"{description} payload length is invalid.");
            }
        }

        private static void RequireComplete(ReadOnlySpan<byte> payload, int offset, string description)
        {
            if (offset != payload.Length)
            {
                throw new FfiValueFormatException($"{description} payload contains trailing bytes.");
            }
        }

        private static void AddInputValue(IntPtr ptrHandle, object value, int dataLength)
        {
            GetPowerShell(ptrHandle);
            FfiInputBuffer input = FfiInputBuffers.GetOrAdd(ptrHandle, _ => new FfiInputBuffer());
            lock (input.Gate)
            {
                if (input.IsCompleted)
                {
                    throw new FfiInputBackpressureException("Input is already completed. Call ResetInput before adding more values.");
                }

                if (input.Values.Count == FfiMaxInputValues ||
                    dataLength > FfiMaxInputPayloadLength - input.PayloadLength)
                {
                    throw new FfiInputBackpressureException("Input capacity has been reached. Invoke or reset the pipeline.");
                }

                input.Values.Add(value);
                input.PayloadLength += dataLength;
            }
        }

        private static void CompleteInput(IntPtr ptrHandle)
        {
            GetPowerShell(ptrHandle);
            FfiInputBuffer input = FfiInputBuffers.GetOrAdd(ptrHandle, _ => new FfiInputBuffer());
            lock (input.Gate)
            {
                input.IsCompleted = true;
            }
        }

        private static object[] TakeCompletedInput(IntPtr ptrHandle)
        {
            if (!FfiInputBuffers.TryGetValue(ptrHandle, out FfiInputBuffer input))
            {
                return null;
            }

            lock (input.Gate)
            {
                if (!input.IsCompleted)
                {
                    throw new FfiInputNotCompletedException();
                }

                object[] values = input.Values.ToArray();
                FfiInputBuffers.TryRemove(ptrHandle, out _);
                return values;
            }
        }

        private enum FfiStreamKind
        {
            Output = 0,
            Error = 1,
            Warning = 2,
            Verbose = 3,
            Debug = 4,
            Information = 5,
            Progress = 6,
        }

        private sealed class FfiInvocationResultSnapshot
        {
            public FfiInvocationResultSnapshot(
                FfiStreamSnapshot[] streams,
                FfiSequenceRecord[] sequence,
                uint flags,
                long invocationId)
            {
                Streams = streams;
                Sequence = sequence;
                Flags = flags;
                InvocationId = invocationId;
            }

            public FfiStreamSnapshot[] Streams { get; }

            public FfiSequenceRecord[] Sequence { get; }

            public uint Flags { get; }

            public long InvocationId { get; }

            public uint State => (Flags & FfiResultTerminatingFailure) != 0 ? 2u : 1u;

            public bool HadErrors => Streams[(int)FfiStreamKind.Error].TotalRecordCount != 0;

            public FfiStreamSnapshot GetStream(int stream)
            {
                if (stream < 0 || stream >= Streams.Length)
                {
                    throw new InvalidOperationException("Invocation stream is invalid.");
                }

                return Streams[stream];
            }

            public FfiSequenceRecord GetSequenceRecord(int index)
            {
                if (index < 0 || index >= Sequence.Length)
                {
                    throw new InvalidOperationException("Invocation sequence index is invalid.");
                }

                return Sequence[index];
            }
        }

        private sealed class FfiStreamSnapshot
        {
            public FfiStreamSnapshot(FfiStreamRecord[] records, uint flags, long totalRecordCount)
            {
                Records = records;
                Flags = flags;
                TotalRecordCount = totalRecordCount;
            }

            public FfiStreamRecord[] Records { get; }

            public uint Flags { get; }

            public long TotalRecordCount { get; }

            public long DroppedRecordCount => Math.Max(0, TotalRecordCount - Records.Length);

            public FfiStreamRecord GetRecord(int index)
            {
                if (index < 0 || index >= Records.Length)
                {
                    throw new InvalidOperationException("Invocation stream record index is invalid.");
                }

                return Records[index];
            }
        }

        private sealed class FfiStreamRecord
        {
            public FfiStreamRecord(
                long sequence,
                string[] fields,
                uint flags,
                FfiSnapshotValue scalarValue,
                FfiSnapshotValue propertyBagValue,
                FfiSnapshotValue errorTargetValue,
                int propertyEntryCount,
                int droppedPropertyEntryCount,
                int typeNameCount,
                int droppedTypeNameCount)
            {
                Sequence = sequence;
                Fields = fields;
                Flags = flags;
                ScalarValue = scalarValue;
                PropertyBagValue = propertyBagValue;
                ErrorTargetValue = errorTargetValue;
                PropertyEntryCount = propertyEntryCount;
                DroppedPropertyEntryCount = droppedPropertyEntryCount;
                TypeNameCount = typeNameCount;
                DroppedTypeNameCount = droppedTypeNameCount;
            }

            public long Sequence { get; }

            public string[] Fields { get; }

            public uint Flags { get; }

            public FfiSnapshotValue ScalarValue { get; }

            public FfiSnapshotValue PropertyBagValue { get; }

            public FfiSnapshotValue ErrorTargetValue { get; }

            public int PropertyEntryCount { get; }

            public int DroppedPropertyEntryCount { get; }

            public int TypeNameCount { get; }

            public int DroppedTypeNameCount { get; }

            public string GetField(int field)
            {
                if (field < 0 || field >= Fields.Length)
                {
                    throw new InvalidOperationException("Invocation stream record field is invalid.");
                }

                return Fields[field];
            }

            public FfiSnapshotValue GetValue(int slot)
            {
                return slot switch
                {
                    0 when ScalarValue != null => ScalarValue,
                    1 when PropertyBagValue != null => PropertyBagValue,
                    2 when ErrorTargetValue != null => ErrorTargetValue,
                    _ => throw new InvalidOperationException("Invocation stream record value is unavailable."),
                };
            }
        }

        private sealed class FfiSnapshotValue
        {
            public FfiSnapshotValue(uint kind, byte[] payload)
            {
                Kind = kind;
                Payload = payload;
            }

            public uint Kind { get; }

            public byte[] Payload { get; }
        }

        private sealed class FfiSequenceRecord
        {
            public FfiSequenceRecord(int stream, int recordIndex, long sequence)
            {
                Stream = stream;
                RecordIndex = recordIndex;
                Sequence = sequence;
            }

            public int Stream { get; }

            public int RecordIndex { get; }

            public long Sequence { get; }
        }

        private sealed class FfiLiveStreamRecord
        {
            public FfiLiveStreamRecord(int stream, long sequence, string displayText, uint flags)
            {
                Stream = stream;
                Sequence = sequence;
                DisplayText = displayText;
                Flags = flags;
            }

            public int Stream { get; }

            public long Sequence { get; }

            public string DisplayText { get; }

            public uint Flags { get; }
        }

        private sealed class FfiLiveStreamBatch
        {
            public FfiLiveStreamBatch(
                FfiLiveStreamRecord[] records,
                long nextSequence,
                long totalRecordCount,
                long lostRecordCount)
            {
                Records = records;
                NextSequence = nextSequence;
                TotalRecordCount = totalRecordCount;
                LostRecordCount = lostRecordCount;
            }

            public FfiLiveStreamRecord[] Records { get; }

            public long NextSequence { get; }

            public long TotalRecordCount { get; }

            public long LostRecordCount { get; }

            public FfiLiveStreamRecord GetRecord(int index)
            {
                if (index < 0 || index >= Records.Length)
                {
                    throw new InvalidOperationException("Live invocation stream record index is invalid.");
                }

                return Records[index];
            }
        }

        private sealed class FfiTypedResultRecord
        {
            public FfiTypedResultRecord(long sequence, FfiSnapshotValue value)
            {
                Sequence = sequence;
                Value = value;
            }

            public long Sequence { get; }

            public FfiSnapshotValue Value { get; }
        }

        private sealed class FfiTypedResultPage
        {
            public FfiTypedResultPage(
                FfiTypedResultRecord[] records,
                long acknowledgedSequence,
                long nextSequence,
                long totalRecordCount,
                long droppedRecordCount,
                int terminalStatus,
                uint flags)
            {
                Records = records;
                AcknowledgedSequence = acknowledgedSequence;
                NextSequence = nextSequence;
                TotalRecordCount = totalRecordCount;
                DroppedRecordCount = droppedRecordCount;
                TerminalStatus = terminalStatus;
                Flags = flags;
            }

            public FfiTypedResultRecord[] Records { get; }

            public long AcknowledgedSequence { get; }

            public long NextSequence { get; }

            public long TotalRecordCount { get; }

            public long DroppedRecordCount { get; }

            public int TerminalStatus { get; }

            public uint Flags { get; }

            public FfiTypedResultRecord GetRecord(int index)
            {
                if (index < 0 || index >= Records.Length)
                {
                    throw new InvalidOperationException("Typed result page record index is invalid.");
                }

                return Records[index];
            }

            public FfiTypedResultPage WithComplete(bool isComplete)
            {
                uint flags = isComplete
                    ? Flags | FfiTypedResultPageComplete
                    : Flags & ~FfiTypedResultPageComplete;
                return new FfiTypedResultPage(
                    Records,
                    AcknowledgedSequence,
                    NextSequence,
                    TotalRecordCount,
                    DroppedRecordCount,
                    TerminalStatus,
                    flags);
            }
        }

        private sealed class FfiObservedDiagnosticRecord
        {
            public FfiObservedDiagnosticRecord(int stream, long sequence, string text, FfiSnapshotValue value)
            {
                Stream = stream;
                Sequence = sequence;
                Text = text;
                Value = value;
            }

            public int Stream { get; }

            public long Sequence { get; }

            public string Text { get; }

            public FfiSnapshotValue Value { get; }
        }

        private sealed class FfiObservedDiagnosticPage
        {
            public FfiObservedDiagnosticPage(
                FfiObservedDiagnosticRecord[] records,
                long acknowledgedSequence,
                long nextSequence,
                long totalRecordCount,
                long droppedRecordCount,
                int terminalStatus,
                uint flags)
            {
                Records = records;
                AcknowledgedSequence = acknowledgedSequence;
                NextSequence = nextSequence;
                TotalRecordCount = totalRecordCount;
                DroppedRecordCount = droppedRecordCount;
                TerminalStatus = terminalStatus;
                Flags = flags;
            }

            public FfiObservedDiagnosticRecord[] Records { get; }

            public long AcknowledgedSequence { get; }

            public long NextSequence { get; }

            public long TotalRecordCount { get; }

            public long DroppedRecordCount { get; }

            public int TerminalStatus { get; }

            public uint Flags { get; }

            public FfiObservedDiagnosticRecord GetRecord(int index)
            {
                if (index < 0 || index >= Records.Length)
                {
                    throw new InvalidOperationException("Observed diagnostic page record index is invalid.");
                }

                return Records[index];
            }

            public FfiObservedDiagnosticPage WithComplete(bool isComplete)
            {
                uint flags = isComplete
                    ? Flags | FfiTypedResultPageComplete
                    : Flags & ~FfiTypedResultPageComplete;
                return new FfiObservedDiagnosticPage(
                    Records,
                    AcknowledgedSequence,
                    NextSequence,
                    TotalRecordCount,
                    DroppedRecordCount,
                    TerminalStatus,
                    flags);
            }
        }

        private sealed class FfiTypedResultQueue : IDisposable
        {
            private readonly object gate = new object();
            private readonly Queue<FfiTypedResultRecord> records;
            private readonly int maximumBufferedRecords;
            private readonly int maximumPageRecords;
            private long nextSequence = 1;
            private long acknowledgedSequence;
            private long maximumAcknowledgableSequence;
            private long totalRecordCount;
            private int terminalStatus = FfiStatusSuccess;
            private bool terminal;
            private bool disposed;

            public FfiTypedResultQueue(int maximumBufferedRecords, int maximumPageRecords)
            {
                this.maximumBufferedRecords = maximumBufferedRecords;
                this.maximumPageRecords = maximumPageRecords;
                records = new Queue<FfiTypedResultRecord>(maximumBufferedRecords);
            }

            public bool Write(FfiSnapshotValue value)
            {
                ArgumentNullException.ThrowIfNull(value);
                lock (gate)
                {
                    while (!terminal && !disposed && records.Count == maximumBufferedRecords)
                    {
                        Monitor.Wait(gate, TimeSpan.FromMilliseconds(50));
                    }

                    if (terminal || disposed)
                    {
                        return false;
                    }

                    if (nextSequence == long.MaxValue || totalRecordCount == long.MaxValue)
                    {
                        Fail(FfiStatusManagedFailure);
                        return false;
                    }

                    records.Enqueue(new FfiTypedResultRecord(
                        nextSequence++,
                        new FfiSnapshotValue(value.Kind, (byte[])value.Payload.Clone())));
                    totalRecordCount++;
                    Monitor.PulseAll(gate);
                    return true;
                }
            }

            public void Fail(int status)
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = status;
                    Monitor.PulseAll(gate);
                }
            }

            public FfiTypedResultPage Read(long acknowledgedThrough, int maximumRecords)
            {
                lock (gate)
                {
                    ThrowIfDisposed();
                    if (maximumRecords < 1 || maximumRecords > maximumPageRecords)
                    {
                        throw new InvalidOperationException("Typed result page limit exceeds the invocation bound.");
                    }

                    if (acknowledgedThrough < acknowledgedSequence ||
                        acknowledgedThrough > maximumAcknowledgableSequence)
                    {
                        throw new InvalidOperationException("Typed result acknowledgement is outside the most recently returned page.");
                    }

                    Acknowledge(acknowledgedThrough);
                    FfiTypedResultRecord[] page = records.Take(maximumRecords).ToArray();
                    long next = page.Length == 0 ? acknowledgedSequence : page[^1].Sequence;
                    maximumAcknowledgableSequence = next;
                    uint flags = terminal ? FfiTypedResultPageTerminal : 0;
                    if (terminal &&
                        terminalStatus == FfiStatusSuccess &&
                        totalRecordCount == acknowledgedSequence)
                    {
                        flags |= FfiTypedResultPageComplete;
                    }

                    return new FfiTypedResultPage(
                        page,
                        acknowledgedSequence,
                        next,
                        totalRecordCount,
                        0,
                        terminalStatus,
                        flags);
                }
            }

            public void Complete(int status)
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = status;
                    Monitor.PulseAll(gate);
                }
            }

            public void Cancel()
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = FfiStatusOperationCancelled;
                    Monitor.PulseAll(gate);
                }
            }

            public void Acknowledge(long sequence)
            {
                if (sequence < acknowledgedSequence || sequence > maximumAcknowledgableSequence)
                {
                    throw new InvalidOperationException("Typed result acknowledgement is outside the most recently returned page.");
                }

                while (records.Count != 0 && records.Peek().Sequence <= sequence)
                {
                    records.Dequeue();
                }

                acknowledgedSequence = sequence;
                maximumAcknowledgableSequence = acknowledgedSequence;
                Monitor.PulseAll(gate);
            }

            public bool IsSuccessfullyAcknowledged
            {
                get
                {
                    lock (gate)
                    {
                        return terminal &&
                            terminalStatus == FfiStatusSuccess &&
                            totalRecordCount == acknowledgedSequence;
                    }
                }
            }

            public void Dispose()
            {
                lock (gate)
                {
                    if (disposed)
                    {
                        return;
                    }

                    disposed = true;
                    terminal = true;
                    terminalStatus = FfiStatusOperationCancelled;
                    records.Clear();
                    Monitor.PulseAll(gate);
                }
            }

            private void ThrowIfDisposed()
            {
                if (disposed)
                {
                    throw new ObjectDisposedException(nameof(FfiTypedResultQueue));
                }
            }
        }

        private sealed class FfiObservedDiagnosticQueue : IDisposable
        {
            private readonly object gate = new object();
            private readonly Queue<FfiObservedDiagnosticRecord> records;
            private readonly int maximumBufferedRecords;
            private readonly int maximumPageRecords;
            private long nextSequence = 1;
            private long acknowledgedSequence;
            private long maximumAcknowledgableSequence;
            private long totalRecordCount;
            private int terminalStatus = FfiStatusSuccess;
            private bool terminal;
            private bool disposed;

            public FfiObservedDiagnosticQueue(int maximumBufferedRecords, int maximumPageRecords)
            {
                this.maximumBufferedRecords = maximumBufferedRecords;
                this.maximumPageRecords = maximumPageRecords;
                records = new Queue<FfiObservedDiagnosticRecord>(maximumBufferedRecords);
            }

            public bool Write(int stream, string text, FfiSnapshotValue value)
            {
                if (stream < 0 || stream >= FfiStreamCount || text is null ||
                    Encoding.UTF8.GetByteCount(text) > FfiMaxValuePayloadLength ||
                    (value is not null && (value.Kind != (uint)FfiValueKind.PropertyBag ||
                                           value.Payload.Length > FfiMaxValuePayloadLength)))
                {
                    Fail(FfiStatusUnsupportedValue);
                    return false;
                }

                lock (gate)
                {
                    while (!terminal && !disposed && records.Count == maximumBufferedRecords)
                    {
                        Monitor.Wait(gate, TimeSpan.FromMilliseconds(50));
                    }

                    if (terminal || disposed)
                    {
                        return false;
                    }

                    if (nextSequence == long.MaxValue || totalRecordCount == long.MaxValue)
                    {
                        Fail(FfiStatusManagedFailure);
                        return false;
                    }

                    records.Enqueue(new FfiObservedDiagnosticRecord(
                        stream,
                        nextSequence++,
                        text,
                        value));
                    totalRecordCount++;
                    Monitor.PulseAll(gate);
                    return true;
                }
            }

            public void Fail(int status)
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = status;
                    Monitor.PulseAll(gate);
                }
            }

            public FfiObservedDiagnosticPage Read(long acknowledgedThrough, int maximumRecords)
            {
                lock (gate)
                {
                    ThrowIfDisposed();
                    if (maximumRecords < 1 || maximumRecords > maximumPageRecords)
                    {
                        throw new InvalidOperationException("Observed diagnostic page limit exceeds the invocation bound.");
                    }

                    if (acknowledgedThrough < acknowledgedSequence ||
                        acknowledgedThrough > maximumAcknowledgableSequence)
                    {
                        throw new InvalidOperationException("Observed diagnostic acknowledgement is outside the most recently returned page.");
                    }

                    Acknowledge(acknowledgedThrough);
                    FfiObservedDiagnosticRecord[] page = records.Take(maximumRecords).ToArray();
                    long next = page.Length == 0 ? acknowledgedSequence : page[^1].Sequence;
                    maximumAcknowledgableSequence = next;
                    uint flags = terminal ? FfiTypedResultPageTerminal : 0;
                    return new FfiObservedDiagnosticPage(
                        page,
                        acknowledgedSequence,
                        next,
                        totalRecordCount,
                        0,
                        terminalStatus,
                        flags);
                }
            }

            public void Complete(int status)
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = status;
                    Monitor.PulseAll(gate);
                }
            }

            public void Cancel()
            {
                lock (gate)
                {
                    if (disposed || terminal)
                    {
                        return;
                    }

                    terminal = true;
                    terminalStatus = FfiStatusOperationCancelled;
                    Monitor.PulseAll(gate);
                }
            }

            public bool IsSuccessfullyAcknowledged
            {
                get
                {
                    lock (gate)
                    {
                        return terminal &&
                            terminalStatus == FfiStatusSuccess &&
                            totalRecordCount == acknowledgedSequence;
                    }
                }
            }

            public void Dispose()
            {
                lock (gate)
                {
                    if (disposed)
                    {
                        return;
                    }

                    disposed = true;
                    terminal = true;
                    terminalStatus = FfiStatusOperationCancelled;
                    records.Clear();
                    Monitor.PulseAll(gate);
                }
            }

            private void Acknowledge(long sequence)
            {
                if (sequence < acknowledgedSequence || sequence > maximumAcknowledgableSequence)
                {
                    throw new InvalidOperationException("Observed diagnostic acknowledgement is outside the most recently returned page.");
                }

                while (records.Count != 0 && records.Peek().Sequence <= sequence)
                {
                    records.Dequeue();
                }

                acknowledgedSequence = sequence;
                maximumAcknowledgableSequence = acknowledgedSequence;
                Monitor.PulseAll(gate);
            }

            private void ThrowIfDisposed()
            {
                if (disposed)
                {
                    throw new ObjectDisposedException(nameof(FfiObservedDiagnosticQueue));
                }
            }
        }

        private sealed class FfiSnapshotCollector
        {
            private const int FieldCount = 21;
            private const int LiveRecordCapacity = FfiMaxStreamRecords * FfiStreamCount;
            private readonly List<FfiStreamRecord>[] streams;
            private readonly uint[] streamFlags;
            private readonly long[] streamTotalRecordCounts;
            private readonly List<FfiSequenceRecord> sequence;
            private readonly Queue<FfiLiveStreamRecord> liveRecords;
            private long nextSequence;
            private long nextLiveSequence = 1;
            private bool terminatingFailure;
            private bool sequenceTruncated;

            public FfiSnapshotCollector()
            {
                streams = new List<FfiStreamRecord>[FfiStreamCount];
                streamFlags = new uint[FfiStreamCount];
                streamTotalRecordCounts = new long[FfiStreamCount];
                for (int index = 0; index < streams.Length; index++)
                {
                    streams[index] = new List<FfiStreamRecord>(FfiMaxStreamRecords);
                }

                sequence = new List<FfiSequenceRecord>(FfiMaxStreamRecords * FfiStreamCount);
                liveRecords = new Queue<FfiLiveStreamRecord>(LiveRecordCapacity);
            }

            public long ErrorCount => streamTotalRecordCounts[(int)FfiStreamKind.Error];

            public void MarkTerminatingFailure()
            {
                terminatingFailure = true;
            }

            public void AddOutput(PSObject value)
            {
                bool truncated = false;
                string[] fields = CreateFields();
                fields[0] = Bound(SafeDisplayText(value), ref truncated);
                fields[1] = Bound(CaptureTypeNames(value, out int typeNameCount, out int droppedTypeNameCount, ref truncated), ref truncated);
                FfiSnapshotValue scalarValue = TryGetBaseObject(value, out object baseObject) &&
                    TryProjectScalar(baseObject, FfiMaxSnapshotScalarPayloadLength, out FfiSnapshotValue projectedScalar)
                    ? projectedScalar
                    : null;
                FfiSnapshotValue propertyBagValue = TryProjectPropertyBag(
                    value,
                    out int propertyEntryCount,
                    out int droppedPropertyEntryCount,
                    out bool propertyBagTruncated);
                uint projectionFlags = 0;
                if (scalarValue != null)
                {
                    projectionFlags |= FfiRecordScalarValuePresent;
                }
                if (propertyBagValue != null)
                {
                    projectionFlags |= FfiRecordPropertyBagPresent;
                }
                if (propertyBagTruncated)
                {
                    projectionFlags |= FfiRecordPropertyBagTruncated;
                }
                if (droppedTypeNameCount != 0)
                {
                    projectionFlags |= FfiRecordTypeNamesTruncated;
                }

                Add(
                    FfiStreamKind.Output,
                    fields,
                    truncated,
                    projectionFlags,
                    scalarValue,
                    propertyBagValue,
                    null,
                    propertyEntryCount,
                    droppedPropertyEntryCount,
                    typeNameCount,
                    droppedTypeNameCount);
            }

            public void AddError(ErrorRecord value, Exception fallbackException = null)
            {
                bool truncated = false;
                Exception exception = SafeGet(() => value?.Exception) ?? fallbackException;
                InvocationInfo invocation = SafeGet(() => value?.InvocationInfo);
                ErrorCategoryInfo category = SafeGet(() => value?.CategoryInfo);
                ErrorDetails details = SafeGet(() => value?.ErrorDetails);
                object target = SafeGet(() => value?.TargetObject);
                string[] fields = CreateFields();
                fields[0] = Bound(SafeGet(() => value?.ToString()) ?? fallbackException?.Message ?? string.Empty, ref truncated);
                fields[2] = Bound(SafeGet(() => value?.FullyQualifiedErrorId) ?? string.Empty, ref truncated);
                fields[3] = Bound(SafeGet(() => category?.Category.ToString()) ?? string.Empty, ref truncated);
                fields[4] = Bound(exception?.GetType().FullName ?? string.Empty, ref truncated);
                fields[20] = (exception?.HResult ?? 0).ToString(CultureInfo.InvariantCulture);
                fields[5] = Bound(SafeGet(() => invocation?.InvocationName) ?? string.Empty, ref truncated);
                fields[6] = Bound(SafeGet(() => invocation?.PositionMessage) ?? string.Empty, ref truncated);
                fields[7] = Bound(SafeGet(() => value?.ScriptStackTrace) ?? string.Empty, ref truncated);
                fields[8] = Bound(SafeGet(() => category?.Reason) ?? string.Empty, ref truncated);
                fields[9] = Bound(SafeGet(() => category?.Activity) ?? string.Empty, ref truncated);
                fields[10] = Bound(SafeGet(() => category?.TargetName) ?? string.Empty, ref truncated);
                fields[11] = Bound(SafeGet(() => category?.TargetType) ?? string.Empty, ref truncated);
                fields[12] = Bound(SafeGet(() => invocation?.MyCommand?.Name) ?? string.Empty, ref truncated);
                fields[13] = Bound(SafeGet(() => invocation?.Line) ?? string.Empty, ref truncated);
                fields[14] = Bound(SafeGet(() => invocation?.OffsetInLine.ToString(CultureInfo.InvariantCulture)) ?? string.Empty, ref truncated);
                fields[15] = Bound(SafeGet(() => invocation?.PipelineLength.ToString(CultureInfo.InvariantCulture)) ?? string.Empty, ref truncated);
                fields[16] = Bound(SafeGet(() => invocation?.PipelinePosition.ToString(CultureInfo.InvariantCulture)) ?? string.Empty, ref truncated);
                fields[17] = Bound(SafeGet(() => details?.Message) ?? string.Empty, ref truncated);
                fields[18] = Bound(SafeGet(() => details?.RecommendedAction) ?? string.Empty, ref truncated);
                fields[19] = Bound(SafeDisplayText(target), ref truncated);
                FfiSnapshotValue targetValue = TryProjectScalar(target, FfiMaxSnapshotScalarPayloadLength, out FfiSnapshotValue projectedTarget)
                    ? projectedTarget
                    : null;
                Add(
                    FfiStreamKind.Error,
                    fields,
                    truncated,
                    targetValue == null ? 0 : FfiRecordErrorTargetValuePresent,
                    null,
                    null,
                    targetValue,
                    0,
                    0,
                    0,
                    0);
            }

            public void AddText(FfiStreamKind stream, object value)
            {
                bool truncated = false;
                string[] fields = CreateFields();
                fields[0] = Bound(SafeDisplayText(value), ref truncated);
                Add(stream, fields, truncated);
            }

            public FfiInvocationResultSnapshot Build()
            {
                var snapshotStreams = new FfiStreamSnapshot[FfiStreamCount];
                for (int index = 0; index < snapshotStreams.Length; index++)
                {
                    snapshotStreams[index] = new FfiStreamSnapshot(
                        streams[index].ToArray(),
                        streamFlags[index],
                        streamTotalRecordCounts[index]);
                }

                uint flags = 0;
                if (terminatingFailure)
                {
                    flags |= FfiResultTerminatingFailure;
                }

                if (sequenceTruncated)
                {
                    flags |= FfiResultSequenceTruncated;
                }

                return new FfiInvocationResultSnapshot(
                    snapshotStreams,
                    sequence.ToArray(),
                    flags,
                    Interlocked.Increment(ref FfiNextInvocationId));
            }

            public FfiLiveStreamBatch ReadLiveBatch(long afterSequence, int maximumRecords)
            {
                if (afterSequence < 0 || maximumRecords < 1 || maximumRecords > FfiMaxStreamRecords)
                {
                    throw new InvalidOperationException("Live invocation stream batch arguments are invalid.");
                }

                long lastSequence = nextLiveSequence - 1;
                if (afterSequence > lastSequence)
                {
                    throw new InvalidOperationException("Live invocation stream cursor is invalid.");
                }

                long firstSequence = liveRecords.Count == 0 ? 0 : liveRecords.Peek().Sequence;
                long lostRecordCount = firstSequence != 0 && afterSequence < firstSequence
                    ? firstSequence - afterSequence - 1
                    : 0;
                FfiLiveStreamRecord[] records = liveRecords
                    .Where(record => record.Sequence > afterSequence)
                    .Take(maximumRecords)
                    .ToArray();
                long next = records.Length == 0 ? afterSequence : records[^1].Sequence;
                return new FfiLiveStreamBatch(records, next, lastSequence, lostRecordCount);
            }

            private void Add(FfiStreamKind stream, string[] fields, bool fieldsTruncated)
            {
                Add(stream, fields, fieldsTruncated, 0, null, null, null, 0, 0, 0, 0);
            }

            private void Add(
                FfiStreamKind stream,
                string[] fields,
                bool fieldsTruncated,
                uint projectionFlags,
                FfiSnapshotValue scalarValue,
                FfiSnapshotValue propertyBagValue,
                FfiSnapshotValue errorTargetValue,
                int propertyEntryCount,
                int droppedPropertyEntryCount,
                int typeNameCount,
                int droppedTypeNameCount)
            {
                int streamIndex = (int)stream;
                if (streamTotalRecordCounts[streamIndex] != long.MaxValue)
                {
                    streamTotalRecordCounts[streamIndex]++;
                }

                long currentSequence = nextSequence == long.MaxValue ? long.MaxValue : nextSequence++;
                long currentLiveSequence = nextLiveSequence == long.MaxValue
                    ? long.MaxValue
                    : nextLiveSequence++;
                uint snapshotFlags = projectionFlags;
                if (fieldsTruncated)
                {
                    snapshotFlags |= FfiRecordFieldsTruncated;
                }
                if (liveRecords.Count == LiveRecordCapacity)
                {
                    liveRecords.Dequeue();
                }
                uint liveFlags = 0;
                string liveDisplayText = BoundLiveDisplayText(fields[0], ref liveFlags);
                liveRecords.Enqueue(new FfiLiveStreamRecord(streamIndex, currentLiveSequence, liveDisplayText, liveFlags));
                List<FfiStreamRecord> records = streams[streamIndex];
                if (records.Count == FfiMaxStreamRecords)
                {
                    streamFlags[streamIndex] |= FfiStreamTruncated;
                    sequenceTruncated = true;
                    return;
                }

                int recordIndex = records.Count;
                records.Add(new FfiStreamRecord(
                    currentSequence,
                    fields,
                    snapshotFlags,
                    scalarValue,
                    propertyBagValue,
                    errorTargetValue,
                    propertyEntryCount,
                    droppedPropertyEntryCount,
                    typeNameCount,
                    droppedTypeNameCount));
                sequence.Add(new FfiSequenceRecord(streamIndex, recordIndex, currentSequence));
            }

            private static string[] CreateFields()
            {
                string[] fields = new string[FieldCount];
                for (int index = 0; index < fields.Length; index++)
                {
                    fields[index] = string.Empty;
                }

                return fields;
            }

            private static string Bound(string value, ref bool truncated)
            {
                value ??= string.Empty;
                if (value.Length <= FfiMaxStreamFieldLength)
                {
                    return value;
                }

                truncated = true;
                return value.Substring(0, FfiMaxStreamFieldLength);
            }

            private static string BoundLiveDisplayText(string value, ref uint flags)
            {
                const int maximumUtf8Bytes = 4096;
                if (Encoding.UTF8.GetByteCount(value) <= maximumUtf8Bytes)
                {
                    return value;
                }

                int length = 0;
                int byteCount = 0;
                while (length < value.Length)
                {
                    int characterLength = char.IsHighSurrogate(value[length]) &&
                        length + 1 < value.Length &&
                        char.IsLowSurrogate(value[length + 1])
                        ? 2
                        : 1;
                    int characterByteCount = Encoding.UTF8.GetByteCount(
                        value.AsSpan(length, characterLength));
                    if (byteCount + characterByteCount > maximumUtf8Bytes)
                    {
                        break;
                    }

                    byteCount += characterByteCount;
                    length += characterLength;
                }

                flags |= FfiRecordFieldsTruncated;
                return value.Substring(0, length);
            }

            private static string SafeDisplayText(object value)
            {
                try
                {
                    return value?.ToString() ?? string.Empty;
                }
                catch (Exception exception)
                {
                    return $"<{exception.GetType().Name}: display text unavailable>";
                }
            }

            private static T SafeGet<T>(Func<T> getter)
            {
                try
                {
                    return getter();
                }
                catch
                {
                    return default;
                }
            }

            private static bool TryGetBaseObject(PSObject value, out object baseObject)
            {
                baseObject = null;
                try
                {
                    baseObject = value?.BaseObject;
                    return true;
                }
                catch
                {
                    return false;
                }
            }

            private static string CaptureTypeNames(
                PSObject value,
                out int retainedCount,
                out int droppedCount,
                ref bool truncated)
            {
                var names = new List<string>(8);
                droppedCount = 0;
                try
                {
                    if (value != null)
                    {
                        foreach (string name in value.TypeNames)
                        {
                            if (names.Count == 8)
                            {
                                IncrementSaturating(ref droppedCount);
                                continue;
                            }

                            string normalized = (name ?? string.Empty).Replace('\r', ' ').Replace('\n', ' ');
                            names.Add(Bound(normalized, ref truncated));
                        }
                    }
                }
                catch
                {
                    IncrementSaturating(ref droppedCount);
                }

                retainedCount = names.Count;
                if (droppedCount != 0)
                {
                    truncated = true;
                }

                return string.Join("\n", names);
            }

            private static FfiSnapshotValue TryProjectPropertyBag(
                PSObject value,
                out int retainedCount,
                out int droppedCount,
                out bool truncated)
            {
                retainedCount = 0;
                droppedCount = 0;
                truncated = false;
                if (value == null)
                {
                    return null;
                }

                var entries = new List<KeyValuePair<string, FfiSnapshotValue>>(FfiMaxSnapshotPropertyEntries);
                var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                int encodedLength = sizeof(uint);
                bool hasNoteProperties = false;
                try
                {
                    foreach (PSPropertyInfo property in value.Properties)
                    {
                        if (property is not PSNoteProperty)
                        {
                            continue;
                        }

                        hasNoteProperties = true;
                        string name = property.Name;
                        if (string.IsNullOrEmpty(name) ||
                            name.Length > FfiMaxSnapshotPropertyNameLength ||
                            name.IndexOf('\0') >= 0 ||
                            !names.Add(name))
                        {
                            IncrementSaturating(ref droppedCount);
                            continue;
                        }

                        object propertyValue;
                        try
                        {
                            propertyValue = property.Value;
                        }
                        catch
                        {
                            IncrementSaturating(ref droppedCount);
                            continue;
                        }

                        if (!TryProjectScalar(propertyValue, FfiMaxSnapshotScalarPayloadLength, out FfiSnapshotValue scalar))
                        {
                            IncrementSaturating(ref droppedCount);
                            continue;
                        }

                        byte[] nameBytes = new UTF8Encoding(false, true).GetBytes(name);
                        int entryLength = sizeof(uint) + nameBytes.Length + sizeof(uint) + sizeof(uint) + scalar.Payload.Length;
                        if (entries.Count == FfiMaxSnapshotPropertyEntries ||
                            entryLength > FfiMaxSnapshotPropertyBagPayloadLength - encodedLength)
                        {
                            IncrementSaturating(ref droppedCount);
                            continue;
                        }

                        encodedLength += entryLength;
                        entries.Add(new KeyValuePair<string, FfiSnapshotValue>(name, scalar));
                    }
                }
                catch
                {
                    IncrementSaturating(ref droppedCount);
                }

                if (!hasNoteProperties)
                {
                    return null;
                }

                entries.Sort((left, right) => StringComparer.Ordinal.Compare(left.Key, right.Key));
                var bytes = new List<byte>(encodedLength);
                WriteUInt32(bytes, checked((uint)entries.Count));
                foreach (KeyValuePair<string, FfiSnapshotValue> entry in entries)
                {
                    byte[] name = new UTF8Encoding(false, true).GetBytes(entry.Key);
                    WriteUInt32(bytes, checked((uint)name.Length));
                    bytes.AddRange(name);
                    WriteUInt32(bytes, entry.Value.Kind);
                    WriteUInt32(bytes, checked((uint)entry.Value.Payload.Length));
                    bytes.AddRange(entry.Value.Payload);
                }

                retainedCount = entries.Count;
                truncated = droppedCount != 0;
                return new FfiSnapshotValue((uint)FfiValueKind.PropertyBag, bytes.ToArray());
            }

            private static bool TryProjectScalar(object value, int maximumPayloadLength, out FfiSnapshotValue scalar)
            {
                scalar = null;
                try
                {
                    uint kind;
                    byte[] payload;
                    switch (value)
                    {
                        case null:
                            kind = (uint)FfiValueKind.Null;
                            payload = Array.Empty<byte>();
                            break;
                        case string text when text.IndexOf('\0') < 0:
                            kind = (uint)FfiValueKind.StringUtf8;
                            payload = new UTF8Encoding(false, true).GetBytes(text);
                            break;
                        case SwitchParameter switchParameter:
                            kind = (uint)FfiValueKind.Switch;
                            payload = [switchParameter.IsPresent ? (byte)1 : (byte)0];
                            break;
                        case bool boolean:
                            kind = (uint)FfiValueKind.Boolean;
                            payload = [boolean ? (byte)1 : (byte)0];
                            break;
                        case sbyte signed:
                            kind = (uint)FfiValueKind.SignedInteger;
                            payload = BitConverter.GetBytes((long)signed);
                            break;
                        case short signed:
                            kind = (uint)FfiValueKind.SignedInteger;
                            payload = BitConverter.GetBytes((long)signed);
                            break;
                        case int signed:
                            kind = (uint)FfiValueKind.SignedInteger;
                            payload = BitConverter.GetBytes((long)signed);
                            break;
                        case long signed:
                            kind = (uint)FfiValueKind.SignedInteger;
                            payload = BitConverter.GetBytes(signed);
                            break;
                        case byte unsigned:
                            kind = (uint)FfiValueKind.UnsignedInteger;
                            payload = BitConverter.GetBytes((ulong)unsigned);
                            break;
                        case ushort unsigned:
                            kind = (uint)FfiValueKind.UnsignedInteger;
                            payload = BitConverter.GetBytes((ulong)unsigned);
                            break;
                        case uint unsigned:
                            kind = (uint)FfiValueKind.UnsignedInteger;
                            payload = BitConverter.GetBytes((ulong)unsigned);
                            break;
                        case ulong unsigned:
                            kind = (uint)FfiValueKind.UnsignedInteger;
                            payload = BitConverter.GetBytes(unsigned);
                            break;
                        case float floating:
                            kind = (uint)FfiValueKind.Double;
                            payload = BitConverter.GetBytes(BitConverter.DoubleToInt64Bits(floating));
                            break;
                        case double floating:
                            kind = (uint)FfiValueKind.Double;
                            payload = BitConverter.GetBytes(BitConverter.DoubleToInt64Bits(floating));
                            break;
                        case decimal decimalValue:
                            kind = (uint)FfiValueKind.DecimalUtf8;
                            payload = new UTF8Encoding(false, true).GetBytes(decimalValue.ToString(CultureInfo.InvariantCulture));
                            break;
                        case byte[] bytes:
                            kind = (uint)FfiValueKind.Bytes;
                            payload = (byte[])bytes.Clone();
                            break;
                        case DateTime dateTime:
                            kind = (uint)FfiValueKind.DateTime;
                            payload = BitConverter.GetBytes(dateTime.ToBinary());
                            break;
                        case DateTimeOffset dateTimeOffset:
                            kind = (uint)FfiValueKind.DateTimeOffset;
                            payload = new byte[sizeof(long) + sizeof(short)];
                            BinaryPrimitives.WriteInt64LittleEndian(payload, dateTimeOffset.Ticks);
                            BinaryPrimitives.WriteInt16LittleEndian(payload.AsSpan(sizeof(long)), checked((short)dateTimeOffset.Offset.TotalMinutes));
                            break;
                        case TimeSpan timeSpan:
                            kind = (uint)FfiValueKind.TimeSpan;
                            payload = BitConverter.GetBytes(timeSpan.Ticks);
                            break;
                        case Guid guid:
                            kind = (uint)FfiValueKind.GuidUtf8;
                            payload = new UTF8Encoding(false, true).GetBytes(guid.ToString("D"));
                            break;
                        case Uri uri when uri.IsAbsoluteUri && uri.AbsoluteUri.IndexOf('\0') < 0:
                            kind = (uint)FfiValueKind.UriUtf8;
                            payload = new UTF8Encoding(false, true).GetBytes(uri.AbsoluteUri);
                            break;
                        default:
                            return false;
                    }

                    if (payload.Length > maximumPayloadLength)
                    {
                        return false;
                    }

                    scalar = new FfiSnapshotValue(kind, payload);
                    return true;
                }
                catch
                {
                    return false;
                }
            }

            internal static bool TryEncodeCopiedValue(object value, int depth, out FfiSnapshotValue copied)
            {
                return TryEncodeCopiedValue(
                    value,
                    depth,
                    new HashSet<object>(ReferenceEqualityComparer.Instance),
                    out copied);
            }

            private static bool TryEncodeCopiedValue(
                object value,
                int depth,
                HashSet<object> ancestors,
                out FfiSnapshotValue copied)
            {
                copied = null;
                if (depth > FfiMaxValueDepth)
                {
                    return false;
                }

                if (TryProjectScalar(value, FfiMaxValuePayloadLength, out copied))
                {
                    return true;
                }

                if (value is Array array)
                {
                    if (array.Rank != 1 ||
                        array.Length > FfiMaxValueContainerEntries ||
                        !ancestors.Add(array))
                    {
                        return false;
                    }

                    try
                    {
                        var bytes = new List<byte>(sizeof(uint));
                        WriteUInt32(bytes, checked((uint)array.Length));
                        for (int index = 0; index < array.Length; index++)
                        {
                            object item = array.GetValue(index);
                            if (!TryEncodeCopiedValue(item, depth + 1, ancestors, out FfiSnapshotValue nested) ||
                                !TryAppendNestedValue(bytes, nested))
                            {
                                return false;
                            }
                        }

                        copied = new FfiSnapshotValue((uint)FfiValueKind.Array, bytes.ToArray());
                        return true;
                    }
                    finally
                    {
                        ancestors.Remove(array);
                    }
                }

                if (value is not PSObject propertyBag || !ancestors.Add(propertyBag))
                {
                    return false;
                }

                try
                {
                    var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                    var entries = new List<KeyValuePair<string, FfiSnapshotValue>>();
                    foreach (PSPropertyInfo property in propertyBag.Properties)
                    {
                        if (property is not PSNoteProperty ||
                            string.IsNullOrEmpty(property.Name) ||
                            property.Name.IndexOf('\0') >= 0 ||
                            !names.Add(property.Name) ||
                            entries.Count == FfiMaxValueContainerEntries ||
                            !TryEncodeCopiedValue(property.Value, depth + 1, ancestors, out FfiSnapshotValue nested))
                        {
                            return false;
                        }

                        entries.Add(new KeyValuePair<string, FfiSnapshotValue>(property.Name, nested));
                    }

                    var payload = new List<byte>(sizeof(uint));
                    WriteUInt32(payload, checked((uint)entries.Count));
                    foreach (KeyValuePair<string, FfiSnapshotValue> entry in entries)
                    {
                        byte[] name = new UTF8Encoding(false, true).GetBytes(entry.Key);
                        if (name.Length > FfiMaxValuePayloadLength - payload.Count - sizeof(uint))
                        {
                            return false;
                        }

                        WriteUInt32(payload, checked((uint)name.Length));
                        if (!TryAppendBytes(payload, name) || !TryAppendNestedValue(payload, entry.Value))
                        {
                            return false;
                        }
                    }

                    copied = new FfiSnapshotValue((uint)FfiValueKind.PropertyBag, payload.ToArray());
                    return true;
                }
                catch
                {
                    return false;
                }
                finally
                {
                    ancestors.Remove(propertyBag);
                }
            }

            private static bool TryAppendNestedValue(List<byte> destination, FfiSnapshotValue value)
            {
                if (value.Payload.Length > FfiMaxValuePayloadLength - destination.Count - (sizeof(uint) * 2))
                {
                    return false;
                }

                WriteUInt32(destination, value.Kind);
                WriteUInt32(destination, checked((uint)value.Payload.Length));
                return TryAppendBytes(destination, value.Payload);
            }

            private static bool TryAppendBytes(List<byte> destination, byte[] value)
            {
                if (value.Length > FfiMaxValuePayloadLength - destination.Count)
                {
                    return false;
                }

                destination.AddRange(value);
                return true;
            }

            private static void WriteUInt32(List<byte> bytes, uint value)
            {
                bytes.Add((byte)value);
                bytes.Add((byte)(value >> 8));
                bytes.Add((byte)(value >> 16));
                bytes.Add((byte)(value >> 24));
            }

            private static void IncrementSaturating(ref int value)
            {
                if (value != int.MaxValue)
                {
                    value++;
                }
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        private unsafe struct FfiNativeUtf8Span
        {
            public byte* Data;
            public nuint Length;
        }

        [StructLayout(LayoutKind.Sequential)]
        private unsafe struct FfiNativeDataValue
        {
            public uint Size;
            public uint Kind;
            public uint Flags;
            public uint Reserved;
            public byte* Data;
            public nuint DataLength;
        }

        [StructLayout(LayoutKind.Sequential)]
        private unsafe struct FfiNativeCallResult
        {
            public uint Size;
            public int Status;
            public uint Flags;
            public uint Reserved;
            public byte* Diagnostic;
            public nuint DiagnosticCapacity;
            public nuint DiagnosticRequired;
            public nuint DiagnosticWritten;
        }

        private sealed unsafe class FfiCapabilityContext
        {
            private readonly ulong registrationHandle;
            private readonly ulong invocationId;
            private readonly IntPtr dispatcher;

            public FfiCapabilityContext(ulong registrationHandle, ulong invocationId, IntPtr dispatcher)
            {
                this.registrationHandle = registrationHandle;
                this.invocationId = invocationId;
                this.dispatcher = dispatcher;
            }

            public object Invoke(string capabilityName, object[] arguments)
            {
                if (string.IsNullOrEmpty(capabilityName) ||
                    capabilityName.Length > 64 ||
                    arguments.Length > FfiMaxValueContainerEntries ||
                    !FfiSnapshotCollector.TryEncodeCopiedValue(arguments, depth: 0, out FfiSnapshotValue encoded))
                {
                    throw new InvalidOperationException("Capability invocation arguments are invalid.");
                }

                byte[] name = new UTF8Encoding(false, true).GetBytes(capabilityName);
                if (name.Length == 0 || name.Length > 64 || encoded.Payload.Length > FfiMaxValuePayloadLength)
                {
                    throw new InvalidOperationException("Capability invocation arguments exceed their bound.");
                }

                byte[] response = new byte[FfiMaxValuePayloadLength];
                byte[] diagnostic = new byte[512];
                fixed (byte* namePointer = name)
                fixed (byte* inputPointer = encoded.Payload)
                fixed (byte* responsePointer = response)
                fixed (byte* diagnosticPointer = diagnostic)
                {
                    FfiNativeDataValue input = new()
                    {
                        Size = (uint)sizeof(FfiNativeDataValue),
                        Kind = encoded.Kind,
                        Data = inputPointer,
                        DataLength = (nuint)encoded.Payload.Length,
                    };
                    FfiNativeCallResult result = new()
                    {
                        Size = (uint)sizeof(FfiNativeCallResult),
                        Diagnostic = diagnosticPointer,
                        DiagnosticCapacity = (nuint)diagnostic.Length,
                    };
                    uint responseKind = 0;
                    nuint responseRequired = 0;
                    var dispatch = (delegate* unmanaged[Cdecl]<
                        ulong,
                        ulong,
                        FfiNativeUtf8Span,
                        FfiNativeDataValue*,
                        uint,
                        uint,
                        uint*,
                        byte*,
                        nuint,
                        nuint*,
                        FfiNativeCallResult*,
                        int>)dispatcher;
                    int status = dispatch(
                        registrationHandle,
                        invocationId,
                        new FfiNativeUtf8Span { Data = namePointer, Length = (nuint)name.Length },
                        &input,
                        (uint)arguments.Length,
                        30_000,
                        &responseKind,
                        responsePointer,
                        (nuint)response.Length,
                        &responseRequired,
                        &result);
                    if (status != FfiStatusSuccess || result.Status != FfiStatusSuccess ||
                        responseRequired > (nuint)response.Length)
                    {
                        throw new InvalidOperationException("The bounded capability invocation failed.");
                    }

                    return DecodeValue(
                        (FfiValueKind)responseKind,
                        new ReadOnlySpan<byte>(responsePointer, checked((int)responseRequired)),
                        depth: 0);
                }
            }
        }

        private sealed class FfiCapabilityBridge
        {
            private readonly FfiCapabilityContext context;

            public FfiCapabilityBridge(FfiCapabilityContext context)
            {
                this.context = context;
            }

            public object Invoke(string capabilityName, params object[] arguments)
            {
                return context.Invoke(capabilityName, arguments ?? Array.Empty<object>());
            }
        }

        private sealed unsafe class FfiBrokerContext
        {
            private readonly ulong channelHandle;
            private readonly ulong generation;
            private readonly IntPtr enqueue;
            private readonly IntPtr post;
            private readonly int maximumBodyBytes;

            public FfiBrokerContext(
                ulong channelHandle,
                ulong generation,
                IntPtr enqueue,
                IntPtr post,
                int maximumBodyBytes)
            {
                this.channelHandle = channelHandle;
                this.generation = generation;
                this.enqueue = enqueue;
                this.post = post;
                // The channel's configured bound, not an assumed 64 KiB: a smaller
                // channel must reject an oversized body here rather than at the
                // native boundary.
                this.maximumBodyBytes = maximumBodyBytes;
            }

            public byte[] Request(uint kind, byte[] body)
            {
                return Request(kind, body, maximumBodyBytes);
            }

            public byte[] Request(uint kind, byte[] body, int replyCapacity)
            {
                body ??= Array.Empty<byte>();
                if (body.Length > maximumBodyBytes || replyCapacity is < 1 or > 64 * 1024)
                {
                    throw new InvalidOperationException("The broker request bounds are invalid.");
                }

                byte[] reply = new byte[replyCapacity];
                byte[] diagnostic = new byte[512];
                fixed (byte* bodyPointer = body.Length == 0 ? new byte[1] : body)
                fixed (byte* replyPointer = reply)
                fixed (byte* diagnosticPointer = diagnostic)
                {
                    FfiNativeCallResult result = new()
                    {
                        Size = (uint)sizeof(FfiNativeCallResult),
                        Diagnostic = diagnosticPointer,
                        DiagnosticCapacity = (nuint)diagnostic.Length,
                    };
                    ulong correlation = 0;
                    uint replyLength = 0;
                    var invoke = (delegate* unmanaged[Cdecl]<
                        ulong, ulong, uint, uint, ulong, uint,
                        byte*, uint, ulong*, byte*, uint, uint*,
                        FfiNativeCallResult*, int>)enqueue;
                    int status = invoke(
                        channelHandle,
                        generation,
                        kind,
                        0,
                        0,
                        0,
                        body.Length == 0 ? null : bodyPointer,
                        (uint)body.Length,
                        &correlation,
                        replyPointer,
                        (uint)reply.Length,
                        &replyLength,
                        &result);
                    if (status != FfiStatusSuccess || result.Status != FfiStatusSuccess || replyLength > reply.Length)
                    {
                        throw new InvalidOperationException(DescribeBrokerFailure(result, diagnostic));
                    }

                    byte[] copied = new byte[replyLength];
                    Buffer.BlockCopy(reply, 0, copied, 0, (int)replyLength);
                    return copied;
                }
            }

            public void Post(uint kind, byte[] body)
            {
                body ??= Array.Empty<byte>();
                if (body.Length > maximumBodyBytes)
                {
                    throw new InvalidOperationException("The broker event body exceeds its bound.");
                }

                byte[] diagnostic = new byte[512];
                fixed (byte* bodyPointer = body.Length == 0 ? new byte[1] : body)
                fixed (byte* diagnosticPointer = diagnostic)
                {
                    FfiNativeCallResult result = new()
                    {
                        Size = (uint)sizeof(FfiNativeCallResult),
                        Diagnostic = diagnosticPointer,
                        DiagnosticCapacity = (nuint)diagnostic.Length,
                    };
                    var invoke = (delegate* unmanaged[Cdecl]<
                        ulong, ulong, uint, ulong, byte*, uint, FfiNativeCallResult*, int>)post;
                    int status = invoke(
                        channelHandle,
                        generation,
                        kind,
                        0,
                        body.Length == 0 ? null : bodyPointer,
                        (uint)body.Length,
                        &result);
                    if (status != FfiStatusSuccess || result.Status != FfiStatusSuccess)
                    {
                        throw new InvalidOperationException(DescribeBrokerFailure(result, diagnostic));
                    }
                }
            }

            private static string DescribeBrokerFailure(FfiNativeCallResult result, byte[] diagnostic)
            {
                int written = (int)Math.Min(result.DiagnosticWritten, (nuint)diagnostic.Length);
                string detail = written > 0
                    ? new UTF8Encoding(false, false).GetString(diagnostic, 0, written)
                    : string.Empty;
                return detail.Length > 0
                    ? "The bounded broker request failed: " + detail
                    : "The bounded broker request failed.";
            }
        }

        private sealed class FfiBrokerBridge
        {
            private readonly FfiBrokerContext context;

            public FfiBrokerBridge(FfiBrokerContext context)
            {
                this.context = context;
            }

            public byte[] Request(uint kind, byte[] body)
            {
                return context.Request(kind, body);
            }

            public void Post(uint kind, byte[] body)
            {
                context.Post(kind, body);
            }
        }

        private sealed class FfiBridgeContext : IDisposable
        {
            private readonly string variableName;
            private readonly FfiLiveObjectLease lease;
            private Runspace runspace;
            private bool hadPreviousVariable;
            private object previousValue;
            private bool bound;
            private bool disposed;

            public FfiBridgeContext(
                string variableName,
                PowerShellLiveObjectContract contract,
                ulong bindingId,
                uint maximumRequestBytes,
                uint maximumReplyBytes,
                FfiBrokerContext broker)
            {
                this.variableName = variableName ?? throw new ArgumentNullException(nameof(variableName));
                ArgumentNullException.ThrowIfNull(broker);
                var sink = new FfiBridgeBrokerSink(
                    contract,
                    bindingId,
                    maximumRequestBytes,
                    maximumReplyBytes,
                    (body, capacity) => broker.Request(
                        PowerShellBridgeBrokerWire.RequestKind,
                        body,
                        capacity),
                    body => broker.Post(PowerShellBridgeBrokerWire.EventKind, body));
                try
                {
                    lease = FfiLiveObjectContracts.CreateBridgeBrokerLease(contract, sink);
                    sink = null!;
                }
                finally
                {
                    sink?.Dispose();
                }
            }

            public void Begin(Runspace target)
            {
                if (disposed)
                {
                    throw new ObjectDisposedException(nameof(FfiBridgeContext));
                }

                if (bound)
                {
                    throw new InvalidOperationException("The bridge context is already bound.");
                }

                runspace = target ?? throw new ArgumentNullException(nameof(target));
                object value = lease.BeginInvocationBinding();
                try
                {
                    PSVariable existing = runspace.SessionStateProxy.PSVariable.Get(variableName);
                    hadPreviousVariable = existing is not null;
                    previousValue = existing?.Value;
                    runspace.SessionStateProxy.SetVariable(variableName, value);
                    bound = true;
                }
                catch
                {
                    lease.EndInvocationBinding();
                    throw;
                }
            }

            public void End()
            {
                if (!bound)
                {
                    return;
                }

                bound = false;
                Exception failure = null;
                try
                {
                    lease.EndInvocationBinding();
                }
                catch (Exception exception)
                {
                    failure = exception;
                }
                finally
                {
                    RestoreVariable(runspace, variableName, hadPreviousVariable, previousValue);
                    runspace = null!;
                    previousValue = null;
                }

                if (failure is not null)
                {
                    throw failure;
                }
            }

            public void Dispose()
            {
                if (disposed)
                {
                    return;
                }

                disposed = true;
                Exception failure = null;
                try
                {
                    End();
                }
                catch (Exception exception)
                {
                    failure = exception;
                }
                finally
                {
                    lease.Dispose();
                }

                if (failure is not null)
                {
                    throw failure;
                }
            }

            private static void RestoreVariable(
                Runspace target,
                string name,
                bool hadPrevious,
                object previous)
            {
                if (target is null)
                {
                    return;
                }

                if (hadPrevious)
                {
                    target.SessionStateProxy.SetVariable(name, previous);
                }
                else
                {
                    target.SessionStateProxy.PSVariable.Remove(name);
                }
            }
        }

        private sealed class FfiLiveInvocation : IDisposable
        {
            private readonly object gate = new object();
            private readonly PowerShell powerShell;
            private readonly object[] input;
            private readonly FfiPowerShellSession session;
            private readonly FfiCapabilityContext capabilityContext;
            private readonly FfiSnapshotCollector collector = new FfiSnapshotCollector();
            private readonly FfiTypedResultQueue typedResults;
            private readonly FfiObservedDiagnosticQueue observedDiagnostics;
            private PSDataCollection<PSObject> output;
            private PSDataCollection<object> inputCollection;
            private IAsyncResult asyncResult;
            private EventHandler<DataAddedEventArgs> outputAdded;
            private EventHandler<DataAddedEventArgs> errorAdded;
            private EventHandler<DataAddedEventArgs> warningAdded;
            private EventHandler<DataAddedEventArgs> verboseAdded;
            private EventHandler<DataAddedEventArgs> debugAdded;
            private EventHandler<DataAddedEventArgs> informationAdded;
            private EventHandler<DataAddedEventArgs> progressAdded;
            private readonly FfiBrokerContext brokerContext;
            private readonly FfiBridgeContext bridgeContext;
            private Runspace capabilityRunspace;
            private bool hadPreviousCapabilityVariable;
            private object previousCapabilityValue;
            private Runspace brokerRunspace;
            private bool hadPreviousBrokerVariable;
            private object previousBrokerValue;
            private Exception terminatingException;
            private ErrorRecord terminatingError;
            private FfiInvocationResultSnapshot snapshot;
            private bool cleanedUp;
            private bool disposed;
            private bool sessionInvocationStarted;
            private bool observedError;

            public FfiLiveInvocation(
                PowerShell powerShell,
                object[] input,
                FfiPowerShellSession session,
                FfiCapabilityContext capabilityContext,
                FfiBrokerContext brokerContext = null,
                FfiBridgeContext bridgeContext = null,
                FfiTypedResultQueue typedResults = null,
                FfiObservedDiagnosticQueue observedDiagnostics = null)
            {
                this.powerShell = powerShell ?? throw new ArgumentNullException(nameof(powerShell));
                this.input = input;
                this.session = session;
                this.capabilityContext = capabilityContext;
                this.brokerContext = brokerContext;
                this.bridgeContext = bridgeContext;
                this.typedResults = typedResults;
                this.observedDiagnostics = observedDiagnostics;
            }

            public bool IsCompleted
            {
                get
                {
                    lock (gate)
                    {
                        return asyncResult?.IsCompleted ?? false;
                    }
                }
            }

            public void Start()
            {
                lock (gate)
                {
                    if (asyncResult != null)
                    {
                        throw new InvalidOperationException("Live invocation has already started.");
                    }

                    output = new PSDataCollection<PSObject> { DataAddedCount = 1 };
                    outputAdded = (_, args) => AddOutput(args.Index);
                    errorAdded = (_, args) => AddError(args.Index);
                    warningAdded = (_, args) => AddText(FfiStreamKind.Warning, powerShell.Streams.Warning, args.Index);
                    verboseAdded = (_, args) => AddText(FfiStreamKind.Verbose, powerShell.Streams.Verbose, args.Index);
                    debugAdded = (_, args) => AddText(FfiStreamKind.Debug, powerShell.Streams.Debug, args.Index);
                    informationAdded = (_, args) => AddText(FfiStreamKind.Information, powerShell.Streams.Information, args.Index);
                    progressAdded = (_, args) => AddText(FfiStreamKind.Progress, powerShell.Streams.Progress, args.Index);

                    ClearStreamBuffers(powerShell);
                    output.DataAdded += outputAdded;
                    powerShell.Streams.Error.DataAdded += errorAdded;
                    powerShell.Streams.Warning.DataAdded += warningAdded;
                    powerShell.Streams.Verbose.DataAdded += verboseAdded;
                    powerShell.Streams.Debug.DataAdded += debugAdded;
                    powerShell.Streams.Information.DataAdded += informationAdded;
                    powerShell.Streams.Progress.DataAdded += progressAdded;
                    if (session != null)
                    {
                        session.BeginInvocation();
                        sessionInvocationStarted = true;
                    }
                    PSInvocationSettings invocationSettings = session?.CreateInvocationSettings();
                    if (capabilityContext != null)
                    {
                        capabilityRunspace = powerShell.Runspace ?? throw new InvalidOperationException(
                            "Bounded capability RPC requires a PowerShell pipeline with an explicit local runspace.");
                        // SetVariable mutates the existing PSVariable in place, so the
                        // value must be snapshotted before it is replaced.
                        PSVariable existingCapabilityVariable =
                            capabilityRunspace.SessionStateProxy.PSVariable.Get("DpsCapabilities");
                        hadPreviousCapabilityVariable = existingCapabilityVariable != null;
                        previousCapabilityValue = existingCapabilityVariable?.Value;
                        capabilityRunspace.SessionStateProxy.SetVariable(
                            "DpsCapabilities",
                            new FfiCapabilityBridge(capabilityContext));
                    }
                    if (brokerContext != null && bridgeContext is null)
                    {
                        brokerRunspace = powerShell.Runspace ?? throw new InvalidOperationException(
                            "The duplex broker channel requires a PowerShell pipeline with an explicit local runspace.");
                        PSVariable existingBrokerVariable =
                            brokerRunspace.SessionStateProxy.PSVariable.Get("DpsBroker");
                        hadPreviousBrokerVariable = existingBrokerVariable != null;
                        previousBrokerValue = existingBrokerVariable?.Value;
                        brokerRunspace.SessionStateProxy.SetVariable(
                            "DpsBroker",
                            new FfiBrokerBridge(brokerContext));
                    }
                    if (bridgeContext != null)
                    {
                        Runspace bridgeRunspace = powerShell.Runspace ?? throw new InvalidOperationException(
                            "The generated bridge requires a PowerShell pipeline with an explicit local runspace.");
                        bridgeContext.Begin(bridgeRunspace);
                    }

                    if (input == null)
                    {
                        asyncResult = powerShell.BeginInvoke<PSObject, PSObject>(
                            null,
                            output,
                            invocationSettings,
                            null,
                            null);
                    }
                    else
                    {
                        inputCollection = new PSDataCollection<object>();
                        foreach (object value in input)
                        {
                            inputCollection.Add(value);
                        }

                        inputCollection.Complete();
                        asyncResult = powerShell.BeginInvoke<object, PSObject>(
                            inputCollection,
                            output,
                            invocationSettings,
                            null,
                            null);
                    }
                }
            }

            public FfiLiveStreamBatch ReadBatch(long afterSequence, int maximumRecords)
            {
                lock (gate)
                {
                    EnsureStarted();
                    return collector.ReadLiveBatch(afterSequence, maximumRecords);
                }
            }

            public FfiTypedResultPage ReadTypedResultPage(long acknowledgedThrough, int maximumRecords)
            {
                FfiTypedResultQueue queue = typedResults ?? throw new InvalidOperationException(
                    "Live invocation does not have a typed result queue.");
                return queue.Read(acknowledgedThrough, maximumRecords);
            }

            public FfiTypedResultPage ReadObservedResultPage(long acknowledgedThrough, int maximumRecords)
            {
                FfiTypedResultQueue resultQueue = typedResults ?? throw new InvalidOperationException(
                    "Live invocation does not have an observed result queue.");
                FfiObservedDiagnosticQueue diagnosticQueue = observedDiagnostics ?? throw new InvalidOperationException(
                    "Live invocation does not have an observed diagnostic queue.");
                FfiTypedResultPage page = resultQueue.Read(acknowledgedThrough, maximumRecords);
                return page.WithComplete(
                    resultQueue.IsSuccessfullyAcknowledged &&
                    diagnosticQueue.IsSuccessfullyAcknowledged);
            }

            public FfiObservedDiagnosticPage ReadObservedDiagnosticPage(long acknowledgedThrough, int maximumRecords)
            {
                FfiTypedResultQueue resultQueue = typedResults ?? throw new InvalidOperationException(
                    "Live invocation does not have an observed result queue.");
                FfiObservedDiagnosticQueue diagnosticQueue = observedDiagnostics ?? throw new InvalidOperationException(
                    "Live invocation does not have an observed diagnostic queue.");
                FfiObservedDiagnosticPage page = diagnosticQueue.Read(acknowledgedThrough, maximumRecords);
                return page.WithComplete(
                    resultQueue.IsSuccessfullyAcknowledged &&
                    diagnosticQueue.IsSuccessfullyAcknowledged);
            }

            public void Stop()
            {
                typedResults?.Cancel();
                observedDiagnostics?.Cancel();
                powerShell.Stop();
            }

            public FfiInvocationResultSnapshot Complete()
            {
                lock (gate)
                {
                    EnsureStarted();
                    if (snapshot != null)
                    {
                        return snapshot;
                    }

                    if (!asyncResult.IsCompleted)
                    {
                        throw new InvalidOperationException("Live invocation has not reached a terminal state.");
                    }

                    try
                    {
                        powerShell.EndInvoke(asyncResult);
                        foreach (PSObject value in output)
                        {
                            collector.AddOutput(value);
                        }
                    }
                    catch (RuntimeException exception)
                    {
                        collector.MarkTerminatingFailure();
                        terminatingException = exception;
                        terminatingError = exception.ErrorRecord;
                    }
                    catch (Exception exception)
                    {
                        collector.MarkTerminatingFailure();
                        terminatingException = exception;
                    }
                    finally
                    {
                        Cleanup();
                    }

                    snapshot = collector.Build();
                    int terminalStatus = (snapshot.Flags & FfiResultTerminatingFailure) != 0 ||
                        (observedDiagnostics is not null && observedError)
                        ? FfiStatusManagedFailure
                        : FfiStatusSuccess;
                    typedResults?.Complete(terminalStatus);
                    observedDiagnostics?.Complete(terminalStatus);
                    return snapshot;
                }
            }

            public void CompleteTypedResults()
            {
                if (typedResults is null)
                {
                    throw new InvalidOperationException("Live invocation does not have a typed result queue.");
                }

                _ = Complete();
            }

            public void CompleteObservedInvocation()
            {
                if (typedResults is null || observedDiagnostics is null)
                {
                    throw new InvalidOperationException("Live invocation does not have observed invocation queues.");
                }

                _ = Complete();
            }

            public void Dispose()
            {
                IAsyncResult invocation;
                lock (gate)
                {
                    if (disposed)
                    {
                        return;
                    }

                    disposed = true;
                    try
                    {
                        typedResults?.Dispose();
                        observedDiagnostics?.Dispose();
                    }
                    finally
                    {
                        // Same reason: the guard is set, so skipping Cleanup here
                        // would permanently strand the session variables and the
                        // invocation accounting.
                        if (asyncResult is null)
                        {
                            Cleanup();
                        }
                    }

                    if (asyncResult is null)
                    {
                        return;
                    }

                    invocation = asyncResult;
                }

                if (!invocation.IsCompleted)
                {
                    Stop();
                }

                invocation.AsyncWaitHandle.WaitOne();
                _ = Complete();
            }

            private void AddError(int index)
            {
                ErrorRecord record;
                lock (gate)
                {
                    record = powerShell.Streams.Error[index];
                    collector.AddError(record);
                    observedError = true;
                    powerShell.Streams.Error.Clear();
                }

                AddObservedDiagnostic(FfiStreamKind.Error, record);
            }

            private void AddOutput(int index)
            {
                FfiSnapshotValue typedValue = null;
                bool typedValueSupported = typedResults is null;
                PSObject diagnosticValue;
                lock (gate)
                {
                    diagnosticValue = output[index];
                    collector.AddOutput(diagnosticValue);
                    if (typedResults is not null)
                    {
                        typedValueSupported = TryEncodeTypedResultValue(diagnosticValue, out typedValue);
                    }
                    output.Clear();
                }

                AddObservedDiagnostic(FfiStreamKind.Output, diagnosticValue);
                if (typedResults is null)
                {
                    return;
                }

                if (!typedValueSupported)
                {
                    typedResults.Fail(FfiStatusUnsupportedValue);
                    return;
                }

                _ = typedResults.Write(typedValue);
            }

            private void AddText<T>(FfiStreamKind stream, PSDataCollection<T> records, int index)
            {
                T record;
                lock (gate)
                {
                    record = records[index];
                    collector.AddText(stream, record);
                    records.Clear();
                }

                AddObservedDiagnostic(stream, record);
            }

            private void AddObservedDiagnostic(FfiStreamKind stream, object record)
            {
                if (observedDiagnostics is null)
                {
                    return;
                }

                try
                {
                    string text = record?.ToString() ?? string.Empty;
                    FfiSnapshotValue progress = null;
                    if (stream == FfiStreamKind.Progress &&
                        !TryEncodeObservedProgress(record as ProgressRecord, out progress))
                    {
                        observedDiagnostics.Fail(FfiStatusUnsupportedValue);
                        return;
                    }

                    _ = observedDiagnostics.Write((int)stream, text, progress);
                }
                catch
                {
                    observedDiagnostics.Fail(FfiStatusUnsupportedValue);
                }
            }

            private static bool TryEncodeObservedProgress(ProgressRecord progress, out FfiSnapshotValue value)
            {
                value = null;
                if (progress is null ||
                    progress.ActivityId < 0 ||
                    progress.ParentActivityId < -1 ||
                    progress.PercentComplete is < -1 or > 100 ||
                    progress.SecondsRemaining < -1 ||
                    !IsObservedProgressText(progress.Activity, 512) ||
                    !IsObservedProgressText(progress.StatusDescription, 1024) ||
                    !IsObservedProgressText(progress.CurrentOperation, 1024))
                {
                    return false;
                }

                var propertyBag = new PSObject();
                propertyBag.Properties.Add(new PSNoteProperty("ActivityId", (long)progress.ActivityId));
                propertyBag.Properties.Add(new PSNoteProperty("ParentActivityId", (long)progress.ParentActivityId));
                propertyBag.Properties.Add(new PSNoteProperty("Activity", progress.Activity ?? string.Empty));
                if (progress.StatusDescription is not null)
                {
                    propertyBag.Properties.Add(new PSNoteProperty("StatusDescription", progress.StatusDescription));
                }

                if (progress.CurrentOperation is not null)
                {
                    propertyBag.Properties.Add(new PSNoteProperty("CurrentOperation", progress.CurrentOperation));
                }

                propertyBag.Properties.Add(new PSNoteProperty("PercentComplete", (long)progress.PercentComplete));
                propertyBag.Properties.Add(new PSNoteProperty("SecondsRemaining", (long)progress.SecondsRemaining));
                propertyBag.Properties.Add(new PSNoteProperty(
                    "IsCompleted",
                    progress.RecordType == ProgressRecordType.Completed));
                return FfiSnapshotCollector.TryEncodeCopiedValue(propertyBag, depth: 0, out value) &&
                    value.Kind == (uint)FfiValueKind.PropertyBag &&
                    value.Payload.Length <= FfiMaxValuePayloadLength;
            }

            private static bool IsObservedProgressText(string value, int maximumLength)
            {
                return value is null || value.Length <= maximumLength;
            }

            private static bool TryEncodeTypedResultValue(PSObject value, out FfiSnapshotValue typedValue)
            {
                typedValue = null;
                try
                {
                    return FfiSnapshotCollector.TryEncodeCopiedValue(value?.BaseObject, depth: 0, out typedValue) ||
                        FfiSnapshotCollector.TryEncodeCopiedValue(value, depth: 0, out typedValue);
                }
                catch
                {
                    typedValue = null;
                    return false;
                }
            }

            private void EnsureStarted()
            {
                if (asyncResult == null)
                {
                    throw new InvalidOperationException("Live invocation has not started.");
                }
            }

            private void Cleanup()
            {
                if (cleanedUp)
                {
                    return;
                }

                cleanedUp = true;
                try
                {
                    if (output != null)
                    {
                        output.DataAdded -= outputAdded;
                    }
                    powerShell.Streams.Error.DataAdded -= errorAdded;
                    powerShell.Streams.Warning.DataAdded -= warningAdded;
                    powerShell.Streams.Verbose.DataAdded -= verboseAdded;
                    powerShell.Streams.Debug.DataAdded -= debugAdded;
                    powerShell.Streams.Information.DataAdded -= informationAdded;
                    powerShell.Streams.Progress.DataAdded -= progressAdded;

                    if (terminatingException != null && collector.ErrorCount == 0)
                    {
                        collector.AddError(terminatingError, terminatingException);
                        observedError = true;
                        AddObservedDiagnostic(FfiStreamKind.Error, terminatingError ?? (object)terminatingException);
                    }

                    output?.Clear();
                    inputCollection?.Clear();
                    ClearStreamBuffers(powerShell);
                }
                finally
                {
                    // The guard above is already set, so any step skipped here is
                    // skipped permanently. Each restore is independent: one failing
                    // must not strand another variable or the session's invocation
                    // accounting.
                    try
                    {
                        RestoreSessionVariable(
                            capabilityRunspace,
                            "DpsCapabilities",
                            hadPreviousCapabilityVariable,
                            previousCapabilityValue);
                    }
                    finally
                    {
                        try
                        {
                            RestoreSessionVariable(
                                brokerRunspace,
                                "DpsBroker",
                                hadPreviousBrokerVariable,
                                previousBrokerValue);
                        }
                        finally
                        {
                            try
                            {
                                bridgeContext?.Dispose();
                            }
                            finally
                            {
                                if (sessionInvocationStarted)
                                {
                                    session.EndInvocation(terminatingException != null);
                                    sessionInvocationStarted = false;
                                }
                            }
                        }
                    }
                }
            }

            private static void RestoreSessionVariable(
                Runspace runspace,
                string name,
                bool hadPrevious,
                object previousValue)
            {
                if (runspace == null)
                {
                    return;
                }

                if (!hadPrevious)
                {
                    runspace.SessionStateProxy.PSVariable.Remove(name);
                }
                else
                {
                    runspace.SessionStateProxy.SetVariable(name, previousValue);
                }
            }
        }

        private static FfiInvocationResultSnapshot InvokeAndCaptureStreamSnapshot(
            PowerShell ps,
            object[] input = null,
            FfiPowerShellSession session = null,
            FfiCapabilityContext capabilityContext = null)
        {
            var collector = new FfiSnapshotCollector();
            Exception terminatingException = null;
            ErrorRecord terminatingError = null;
            var output = new PSDataCollection<PSObject> { DataAddedCount = 1 };
            session?.BeginInvocation();
            PSInvocationSettings invocationSettings = session?.CreateInvocationSettings();
            Runspace capabilityRunspace = null;
            bool hadPreviousCapabilityVariable = false;
            object previousCapabilityValue = null;

            EventHandler<DataAddedEventArgs> outputAdded = (_, args) =>
            {
                collector.AddOutput(output[args.Index]);
                output.Clear();
            };
            EventHandler<DataAddedEventArgs> errorAdded = (_, args) =>
            {
                collector.AddError(ps.Streams.Error[args.Index]);
                ps.Streams.Error.Clear();
            };
            EventHandler<DataAddedEventArgs> warningAdded = (_, args) =>
            {
                collector.AddText(FfiStreamKind.Warning, ps.Streams.Warning[args.Index]);
                ps.Streams.Warning.Clear();
            };
            EventHandler<DataAddedEventArgs> verboseAdded = (_, args) =>
            {
                collector.AddText(FfiStreamKind.Verbose, ps.Streams.Verbose[args.Index]);
                ps.Streams.Verbose.Clear();
            };
            EventHandler<DataAddedEventArgs> debugAdded = (_, args) =>
            {
                collector.AddText(FfiStreamKind.Debug, ps.Streams.Debug[args.Index]);
                ps.Streams.Debug.Clear();
            };
            EventHandler<DataAddedEventArgs> informationAdded = (_, args) =>
            {
                collector.AddText(FfiStreamKind.Information, ps.Streams.Information[args.Index]);
                ps.Streams.Information.Clear();
            };
            EventHandler<DataAddedEventArgs> progressAdded = (_, args) =>
            {
                collector.AddText(FfiStreamKind.Progress, ps.Streams.Progress[args.Index]);
                ps.Streams.Progress.Clear();
            };

            ClearStreamBuffers(ps);
            output.DataAdded += outputAdded;
            ps.Streams.Error.DataAdded += errorAdded;
            ps.Streams.Warning.DataAdded += warningAdded;
            ps.Streams.Verbose.DataAdded += verboseAdded;
            ps.Streams.Debug.DataAdded += debugAdded;
            ps.Streams.Information.DataAdded += informationAdded;
            ps.Streams.Progress.DataAdded += progressAdded;
            try
            {
                if (capabilityContext != null)
                {
                    capabilityRunspace = ps.Runspace ?? throw new InvalidOperationException(
                        "Bounded capability RPC requires a PowerShell pipeline with an explicit local runspace.");
                    // SetVariable mutates the existing PSVariable in place, so the
                    // value must be snapshotted before it is replaced.
                    PSVariable existingCapabilityVariable =
                        capabilityRunspace.SessionStateProxy.PSVariable.Get("DpsCapabilities");
                    hadPreviousCapabilityVariable = existingCapabilityVariable != null;
                    previousCapabilityValue = existingCapabilityVariable?.Value;
                    capabilityRunspace.SessionStateProxy.SetVariable(
                        "DpsCapabilities",
                        new FfiCapabilityBridge(capabilityContext));
                }
                if (input == null)
                {
                    ps.Invoke<PSObject, PSObject>(null, output, invocationSettings);
                }
                else
                {
                    var inputCollection = new PSDataCollection<object>();
                    foreach (object value in input)
                    {
                        inputCollection.Add(value);
                    }

                    inputCollection.Complete();
                    ps.Invoke<object, PSObject>(inputCollection, output, invocationSettings);
                    inputCollection.Clear();
                }
                foreach (PSObject value in output)
                {
                    collector.AddOutput(value);
                }
            }
            catch (RuntimeException exception)
            {
                collector.MarkTerminatingFailure();
                terminatingException = exception;
                terminatingError = exception.ErrorRecord;
            }
            catch (Exception exception)
            {
                collector.MarkTerminatingFailure();
                terminatingException = exception;
            }
            finally
            {
                output.DataAdded -= outputAdded;
                ps.Streams.Error.DataAdded -= errorAdded;
                ps.Streams.Warning.DataAdded -= warningAdded;
                ps.Streams.Verbose.DataAdded -= verboseAdded;
                ps.Streams.Debug.DataAdded -= debugAdded;
                ps.Streams.Information.DataAdded -= informationAdded;
                ps.Streams.Progress.DataAdded -= progressAdded;

                if (terminatingException != null && collector.ErrorCount == 0)
                {
                    collector.AddError(terminatingError, terminatingException);
                }

                output.Clear();
                ClearStreamBuffers(ps);
                if (capabilityRunspace != null)
                {
                    if (!hadPreviousCapabilityVariable)
                    {
                        capabilityRunspace.SessionStateProxy.PSVariable.Remove("DpsCapabilities");
                    }
                    else
                    {
                        capabilityRunspace.SessionStateProxy.SetVariable(
                            "DpsCapabilities",
                            previousCapabilityValue);
                    }
                }
                session?.EndInvocation(terminatingException != null);
            }

            return collector.Build();
        }

        private static InvocationResult InvokeAndCaptureLegacy(
            PowerShell ps,
            object[] input = null,
            FfiPowerShellSession session = null)
        {
            FfiInvocationResultSnapshot snapshot = InvokeAndCaptureStreamSnapshot(ps, input, session);
            var output = new StringBuilder();
            foreach (FfiStreamRecord record in snapshot.GetStream((int)FfiStreamKind.Output).Records)
            {
                output.AppendLine(record.GetField(0));
            }

            FfiStreamRecord[] capturedErrors = snapshot.GetStream((int)FfiStreamKind.Error).Records;
            var errors = new InvocationError[capturedErrors.Length];
            for (int index = 0; index < capturedErrors.Length; index++)
            {
                FfiStreamRecord error = capturedErrors[index];
                errors[index] = new InvocationError(
                    error.GetField(0),
                    error.GetField(2),
                    error.GetField(3),
                    error.GetField(4));
            }

            return new InvocationResult(
                output.ToString(),
                errors,
                (snapshot.Flags & FfiResultTerminatingFailure) != 0 ? FfiStatusManagedFailure : FfiStatusSuccess);
        }

        private static void ClearStreamBuffers(PowerShell ps)
        {
            ps.Streams.Error.Clear();
            ps.Streams.Warning.Clear();
            ps.Streams.Verbose.Clear();
            ps.Streams.Debug.Clear();
            ps.Streams.Information.Clear();
            ps.Streams.Progress.Clear();
        }

        private static FfiInvocationResultSnapshot GetInvocationResultSnapshot(IntPtr ptrResultHandle)
        {
            if (ptrResultHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("Invocation result handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrResultHandle);
            if (!handle.IsAllocated || handle.Target is not FfiInvocationResultSnapshot snapshot)
            {
                throw new InvalidOperationException("Invocation result handle is invalid.");
            }

            return snapshot;
        }

        private static FfiLiveInvocation GetLiveInvocation(IntPtr ptrLiveInvocationHandle)
        {
            if (ptrLiveInvocationHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("Live invocation handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrLiveInvocationHandle);
            if (!handle.IsAllocated || handle.Target is not FfiLiveInvocation invocation)
            {
                throw new InvalidOperationException("Live invocation handle is invalid.");
            }

            return invocation;
        }

        private static FfiLiveStreamBatch GetLiveStreamBatch(IntPtr ptrBatchHandle)
        {
            if (ptrBatchHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("Live invocation stream batch handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrBatchHandle);
            if (!handle.IsAllocated || handle.Target is not FfiLiveStreamBatch batch)
            {
                throw new InvalidOperationException("Live invocation stream batch handle is invalid.");
            }

            return batch;
        }

        private static FfiTypedResultPage GetTypedResultPage(IntPtr ptrPageHandle)
        {
            if (ptrPageHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("Typed result page handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrPageHandle);
            if (!handle.IsAllocated || handle.Target is not FfiTypedResultPage page)
            {
                throw new InvalidOperationException("Typed result page handle is invalid.");
            }

            return page;
        }

        private static FfiObservedDiagnosticPage GetObservedDiagnosticPage(IntPtr ptrPageHandle)
        {
            if (ptrPageHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("Observed diagnostic page handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrPageHandle);
            if (!handle.IsAllocated || handle.Target is not FfiObservedDiagnosticPage page)
            {
                throw new InvalidOperationException("Observed diagnostic page handle is invalid.");
            }

            return page;
        }

        private static unsafe int ReleaseLiveHandle<T>(IntPtr ptrHandle, FfiCallResult* result, string invalidMessage)
            where T : class
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                GCHandle handle = GCHandle.FromIntPtr(ptrHandle);
                if (!handle.IsAllocated || handle.Target is not T)
                {
                    return WriteFailure(result, FfiStatusInvalidHandle, invalidMessage);
                }

                handle.Free();
                return WriteSuccess(result);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidHandle, exception);
            }
        }

        private sealed class BufferTooSmallException : Exception
        {
        }

        private static FfiPowerShellPipeline GetPowerShellPipeline(IntPtr ptrHandle)
        {
            if (ptrHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("PowerShell handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrHandle);
            if (!handle.IsAllocated || handle.Target is not FfiPowerShellPipeline pipeline)
            {
                throw new InvalidOperationException("PowerShell handle is invalid.");
            }

            return pipeline;
        }

        private static PowerShell GetPowerShell(IntPtr ptrHandle)
        {
            return GetPowerShellPipeline(ptrHandle).PowerShell;
        }

        private static FfiPowerShellSession GetPowerShellSession(IntPtr ptrHandle)
        {
            if (ptrHandle == IntPtr.Zero)
            {
                throw new InvalidOperationException("PowerShell session handle is invalid.");
            }

            GCHandle handle = GCHandle.FromIntPtr(ptrHandle);
            if (!handle.IsAllocated || handle.Target is not FfiPowerShellSession session)
            {
                throw new InvalidOperationException("PowerShell session handle is invalid.");
            }

            return session;
        }

        private static unsafe int ReadUtf8(byte* value, int length, FfiCallResult* result, out string text)
        {
            text = string.Empty;
            if (length < 0 || (length > 0 && value == null))
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "UTF-8 input span is invalid.");
            }

            try
            {
                ReadOnlySpan<byte> bytes = new ReadOnlySpan<byte>(value, length);
                if (bytes.IndexOf((byte)0) >= 0)
                {
                    return WriteFailure(result, FfiStatusInvalidArgument, "UTF-8 input cannot contain NUL.");
                }

                text = length == 0 ? string.Empty : Encoding.UTF8.GetString(bytes);
                return FfiStatusSuccess;
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, exception);
            }
        }

        private static unsafe int Execute(FfiCallResult* result, Action operation, bool bufferTooSmallIsSuccess = false)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                operation();
                return WriteSuccess(result);
            }
            catch (BufferTooSmallException) when (bufferTooSmallIsSuccess)
            {
                result->Status = 1;
                return result->Status;
            }
            catch (FfiInputNotCompletedException exception)
            {
                return WriteFailure(result, FfiStatusInputNotCompleted, exception);
            }
            catch (FfiInputBackpressureException exception)
            {
                return WriteFailure(result, FfiStatusBackpressure, exception);
            }
            catch (FfiUnsupportedValueException exception)
            {
                return WriteFailure(result, FfiStatusUnsupportedValue, exception);
            }
            catch (Exception exception)
            {
                return WriteFailure(result, FfiStatusManagedFailure, exception);
            }
        }

        private static unsafe int ExecuteSecret(FfiCallResult* result, Action operation)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            try
            {
                operation();
                return WriteSuccess(result);
            }
            catch
            {
                return WriteFailure(result, FfiStatusManagedFailure, "Secret-bound PowerShell operation failed.");
            }
        }

        private static unsafe bool TryInitializeResult(FfiCallResult* result)
        {
            if (result == null || result->Size < (uint)sizeof(FfiCallResult))
            {
                return false;
            }

            result->Status = FfiStatusSuccess;
            result->Flags = 0;
            result->DiagnosticRequiredLength = 0;
            result->DiagnosticWrittenLength = 0;
            return true;
        }

        private static string TryGetPowerShellFileVersion()
        {
            try
            {
                string location = typeof(PowerShell).Assembly.Location;
                if (string.IsNullOrWhiteSpace(location))
                {
                    return string.Empty;
                }

                string version = FileVersionInfo.GetVersionInfo(location).FileVersion;
                return string.IsNullOrWhiteSpace(version) ||
                       version.IndexOf('\0') >= 0 ||
                       Encoding.UTF8.GetByteCount(version) > 128
                    ? string.Empty
                    : version;
            }
            catch
            {
                return string.Empty;
            }
        }

        private static unsafe int WriteSuccess(FfiCallResult* result)
        {
            result->Status = FfiStatusSuccess;
            return FfiStatusSuccess;
        }

        private static unsafe int WriteFailure(FfiCallResult* result, int status, Exception exception)
        {
            return WriteFailure(result, status, exception.Message);
        }

        private static unsafe int WriteFailure(FfiCallResult* result, int status, string diagnostic)
        {
            if (!TryInitializeResult(result))
            {
                return FfiStatusInvalidArgument;
            }

            diagnostic ??= string.Empty;
            result->Status = status;
            result->DiagnosticRequiredLength = Encoding.UTF8.GetByteCount(diagnostic);
            if (result->DiagnosticCapacity <= 0 || result->Diagnostic == null)
            {
                if (result->DiagnosticRequiredLength > 0)
                {
                    result->Flags |= FfiCallResultTruncatedDiagnostic;
                }

                return status;
            }

            Encoder encoder = Encoding.UTF8.GetEncoder();
            encoder.Convert(
                diagnostic.AsSpan(),
                new Span<byte>(result->Diagnostic, result->DiagnosticCapacity),
                true,
                out _,
                out int written,
                out bool completed);

            result->DiagnosticWrittenLength = written;
            if (!completed)
            {
                result->Flags |= FfiCallResultTruncatedDiagnostic;
            }

            return status;
        }
    }
}
