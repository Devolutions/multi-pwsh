using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Devolutions.PowerShell.Ffi.LiveObjects;

namespace Devolutions.PowerShell.Ffi;

[StructLayout(LayoutKind.Sequential)]
internal struct NativeAbiInfo
{
    internal uint Size;
    internal uint AbiVersion;
    internal ulong FeatureFlags;
    internal uint MinimumCompatibleAbiVersion;
    internal uint Reserved;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeUtf8Span
{
    internal byte* Data;
    internal nuint Length;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeDataValue
{
    internal uint Size;
    internal uint Kind;
    internal uint Flags;
    internal uint Reserved;
    internal byte* Data;
    internal nuint DataLength;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeCredentialResult
{
    internal uint Size;
    internal uint IsCancelled;
    internal byte* Username;
    internal int UsernameCapacity;
    internal int UsernameLength;
    internal byte* Domain;
    internal int DomainCapacity;
    internal int DomainLength;
    internal char* Password;
    internal int PasswordCapacity;
    internal int PasswordLength;
    internal byte* OutputMessages;
    internal int OutputMessagesCapacity;
    internal int OutputMessagesLength;
    internal byte* ErrorMessages;
    internal int ErrorMessagesCapacity;
    internal int ErrorMessagesLength;
    internal byte* LogMessage;
    internal int LogMessageCapacity;
    internal int LogMessageLength;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeBrokerChannelOptions
{
    internal uint Size;
    internal uint AbiVersion;
    internal uint MaximumInflightFrames;
    internal uint MaximumBodyBytes;
    internal uint DefaultDeadlineMilliseconds;
    internal uint Flags;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeBrokerFrameInfo
{
    internal uint Size;
    internal uint AbiVersion;
    internal ulong CorrelationId;
    internal ulong OrderingKey;
    internal ulong DeadlineEpochMilliseconds;
    internal uint RemainingMilliseconds;
    internal uint Kind;
    internal uint Flags;
    internal uint BodyLength;
    internal uint State;
    internal uint DroppedBefore;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeBrokerTerminalInfo
{
    internal uint Size;
    internal uint AbiVersion;
    internal uint State;
    internal int TerminalStatus;
    internal ulong TerminalEpochMilliseconds;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeCapabilityRegistration
{
    internal uint Size;
    internal uint Flags;
    internal NativeDataValue* Definitions;
    internal nint DispatchCallback;
    internal nint CancelCallback;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeLiveObjectContractPack
{
    internal uint Size;
    internal uint Flags;
    internal NativeUtf8Span PayloadAdapterAssemblyPath;
    internal NativeUtf8Span PayloadAdapterTypeName;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeCallResult
{
    internal uint Size;
    internal int Status;
    internal uint Flags;
    internal uint Reserved;
    internal byte* Diagnostic;
    internal nuint DiagnosticCapacity;
    internal nuint DiagnosticRequired;
    internal nuint DiagnosticWritten;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeSessionOptions
{
    internal uint Size;
    internal uint RunspaceMode;
    internal uint InitialConfiguration;
    internal uint HistoryMode;
    internal uint ErrorPreference;
    internal uint WarningPreference;
    internal uint VerbosePreference;
    internal uint DebugPreference;
    internal uint InformationPreference;
    internal uint Flags;
    internal uint Reserved;
    internal NativeUtf8Span AllowedModulePath;
    internal uint ExecutionPolicy;
    internal uint ConfigurationFlags;
    internal NativeDataValue InitialVariables;
    internal NativeDataValue ModuleImports;
    internal NativeDataValue AllowedModulePaths;
    internal NativeUtf8Span WorkingDirectory;
    internal NativeDataValue Environment;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeSessionSnapshot
{
    internal uint Size;
    internal uint State;
    internal uint RunspaceState;
    internal uint Flags;
    internal uint ActivePipelineCount;
    internal uint EventCount;
    internal ulong InvocationCount;
    internal ulong HistoryCount;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeSessionPoolOptions
{
    internal uint Size;
    internal uint MinimumSessions;
    internal uint MaximumSessions;
    internal uint Flags;
    internal uint Reserved;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeOperationStreamBatchInfo
{
    internal uint Size;
    internal uint OperationState;
    internal int TerminalStatus;
    internal uint Flags;
    internal ulong NextSequence;
    internal ulong TotalRecordCount;
    internal ulong DroppedRecordCount;
    internal ulong SourceDroppedRecordCount;
    internal ulong LostRecordCount;
    internal uint RecordCount;
    internal uint Reserved;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeTypedResultPageInfo
{
    internal uint Size;
    internal uint Flags;
    internal int TerminalStatus;
    internal uint Reserved;
    internal ulong AcknowledgedSequence;
    internal ulong NextSequence;
    internal ulong TotalRecordCount;
    internal ulong DroppedRecordCount;
    internal uint RecordCount;
    internal uint Reserved2;
}

[StructLayout(LayoutKind.Sequential)]
internal struct NativeRuntimeDiagnosticsInfo
{
    internal uint Size;
    internal uint BindingsAbiVersion;
    internal nuint PayloadTableSize;
    internal uint PayloadTableSlotCount;
    internal uint PayloadTableShape;
    internal uint PowerShellFileVersionAvailable;
    internal uint ContractPackCount;
    internal uint Reserved;
    internal uint RemoteProviderStatus;
    internal uint RemoteProviderUnavailableReason;
    internal uint Reserved2;
    internal ulong RemoteProviderCapabilities;
}

internal static unsafe partial class NativeMethods
{
    internal const string LibraryName = "multi-pwsh-sdk";

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_get_abi_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetAbiInfo(NativeAbiInfo* info);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_initialize_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Initialize(NativeUtf8Span payloadPath, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_initialize_from_path")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InitializeFromPath(NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_initialize_from_path_with_contract_packs")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InitializeFromPathWithContractPacks(
        NativeLiveObjectContractPack* packs,
        nuint packCount,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_initialize_with_contract_packs_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InitializeWithContractPacks(
        NativeUtf8Span payloadPath,
        NativeLiveObjectContractPack* packs,
        nuint packCount,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_get_payload_path_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetPayloadPath(
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_get_runtime_diagnostics_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetRuntimeDiagnosticsInfo(
        NativeRuntimeDiagnosticsInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_copy_runtime_diagnostics_power_shell_file_version_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyRuntimeDiagnosticsPowerShellFileVersion(
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_copy_runtime_diagnostics_contract_pack_identity_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyRuntimeDiagnosticsContractPackIdentity(
        uint index,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_create")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Create(ulong* handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_live_object_probe_create")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CreateLiveObjectProbe(long initialCount, nint* comObject, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_live_object_probe_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseLiveObjectProbe(nint comObject, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_live_object_probe_unregister")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int UnregisterLiveObjectProbe(nint comObject, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Release(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_command_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddCommand(ulong handle, NativeUtf8Span command, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_script_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddScript(ulong handle, NativeUtf8Span script, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_argument_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddArgument(ulong handle, NativeUtf8Span argument, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_parameter_string_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddParameterString(
        ulong handle,
        NativeUtf8Span name,
        NativeUtf8Span value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_parameter_i64")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddParameterInt64(
        ulong handle,
        NativeUtf8Span name,
        long value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_command_utf8_local")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddCommandWithLocalScope(
        ulong handle,
        NativeUtf8Span command,
        uint useLocalScope,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_script_utf8_local")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddScriptWithLocalScope(
        ulong handle,
        NativeUtf8Span script,
        uint useLocalScope,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_argument_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddArgumentValue(
        ulong handle,
        NativeDataValue* value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_argument_live_object")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddArgumentLiveObject(
        ulong handle,
        nint comObject,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_parameter_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddParameterValue(
        ulong handle,
        NativeUtf8Span name,
        NativeDataValue* value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_invoke_secret_result")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InvokeSecretResult(
        ulong handle,
        uint expectedKind,
        byte* userNameBuffer,
        nuint userNameCapacity,
        nuint* userNameLength,
        char* secretBuffer,
        nuint secretCapacity,
        nuint* secretLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_invoke_credential_result")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InvokeCredentialResult(
        ulong handle,
        NativeCredentialResult* credentialResult,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_parameter_switch")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddParameterSwitch(
        ulong handle,
        NativeUtf8Span name,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_input_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddInputValue(
        ulong handle,
        NativeDataValue* value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_complete_input")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CompleteInput(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_reset_input")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ResetInput(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_add_statement")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int AddStatement(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_clear")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Clear(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_invoke_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Invoke(
        ulong handle,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_invoke")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InvokeToResult(
        ulong handle,
        ulong* resultHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseInvocationResult(ulong resultHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultInfo(
        ulong resultHandle,
        uint* flags,
        uint* sequenceCount,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_metadata")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultMetadata(
        ulong resultHandle,
        uint* state,
        ulong* invocationId,
        uint* hadErrors,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_stream_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultStreamInfo(
        ulong resultHandle,
        uint stream,
        uint* recordCount,
        uint* flags,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_stream_record_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultStreamRecordInfo(
        ulong resultHandle,
        uint stream,
        uint recordIndex,
        ulong* sequence,
        uint* flags,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_copy_stream_record_field_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyInvocationResultStreamRecordField(
        ulong resultHandle,
        uint stream,
        uint recordIndex,
        uint field,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_stream_totals")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultStreamTotals(
        ulong resultHandle,
        uint stream,
        ulong* totalRecordCount,
        ulong* droppedRecordCount,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_stream_record_projection_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultStreamRecordProjectionInfo(
        ulong resultHandle,
        uint stream,
        uint recordIndex,
        uint* propertyEntryCount,
        uint* droppedPropertyEntryCount,
        uint* typeNameCount,
        uint* droppedTypeNameCount,
        uint* projectionFlags,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_copy_stream_record_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyInvocationResultStreamRecordValue(
        ulong resultHandle,
        uint stream,
        uint recordIndex,
        uint valueSlot,
        uint* kind,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_result_get_sequence_record")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationResultSequenceRecord(
        ulong resultHandle,
        uint sequenceIndex,
        uint* stream,
        uint* recordIndex,
        ulong* sequence,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_get_invocation_error_count")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetInvocationErrorCount(
        ulong handle,
        uint* errorCount,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_copy_invocation_error_field_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyInvocationErrorField(
        ulong handle,
        uint errorIndex,
        uint field,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_stop")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int Stop(ulong handle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_invoke_async")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int InvokeAsync(
        ulong handle,
        ulong* operationHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseOperation(ulong operationHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_stop")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int StopOperation(ulong operationHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_poll")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int PollOperation(
        ulong operationHandle,
        uint* operationState,
        int* terminalStatus,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_wait")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int WaitOperation(
        ulong operationHandle,
        uint timeoutMilliseconds,
        uint* operationState,
        int* terminalStatus,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_get_result")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetOperationResult(
        ulong operationHandle,
        ulong* resultHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_read_stream_batch")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReadOperationStreamBatch(
        ulong operationHandle,
        ulong afterSequence,
        uint maximumRecords,
        ulong* batchHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_stream_batch_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetOperationStreamBatchInfo(
        ulong batchHandle,
        NativeOperationStreamBatchInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_stream_batch_get_record_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetOperationStreamBatchRecordInfo(
        ulong batchHandle,
        uint recordIndex,
        uint* stream,
        ulong* sequence,
        uint* flags,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_stream_batch_copy_record_text_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyOperationStreamBatchRecordText(
        ulong batchHandle,
        uint recordIndex,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_operation_stream_batch_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseOperationStreamBatch(ulong batchHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_begin_typed_result_invocation")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BeginTypedResultInvocation(
        ulong handle,
        uint maximumBufferedRecords,
        uint maximumPageRecords,
        ulong* typedResultHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_invocation_read_page")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReadTypedResultPage(
        ulong typedResultHandle,
        ulong acknowledgedThrough,
        uint maximumRecords,
        ulong* pageHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_invocation_stop")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int StopTypedResultInvocation(ulong typedResultHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_invocation_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseTypedResultInvocation(ulong typedResultHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_page_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetTypedResultPageInfo(
        ulong pageHandle,
        NativeTypedResultPageInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_page_get_record_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetTypedResultPageRecordInfo(
        ulong pageHandle,
        uint recordIndex,
        ulong* sequence,
        uint* kind,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_page_copy_record_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyTypedResultPageRecordValue(
        ulong pageHandle,
        uint recordIndex,
        uint* kind,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_typed_result_page_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseTypedResultPage(ulong pageHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_begin_observed_invocation")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BeginObservedInvocation(
        ulong handle,
        uint maximumBufferedResultRecords,
        uint maximumResultPageRecords,
        uint maximumBufferedDiagnosticRecords,
        uint maximumDiagnosticPageRecords,
        ulong* observedHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_invocation_read_result_page")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReadObservedResultPage(
        ulong observedHandle,
        ulong acknowledgedThrough,
        uint maximumRecords,
        ulong* pageHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_invocation_read_diagnostic_page")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReadObservedDiagnosticPage(
        ulong observedHandle,
        ulong acknowledgedThrough,
        uint maximumRecords,
        ulong* pageHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_invocation_stop")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int StopObservedInvocation(ulong observedHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_invocation_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseObservedInvocation(ulong observedHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_diagnostic_page_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetObservedDiagnosticPageInfo(
        ulong pageHandle,
        NativeTypedResultPageInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_diagnostic_page_get_record_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetObservedDiagnosticPageRecordInfo(
        ulong pageHandle,
        uint recordIndex,
        uint* stream,
        ulong* sequence,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_diagnostic_page_copy_record_text_utf8")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyObservedDiagnosticPageRecordText(
        ulong pageHandle,
        uint recordIndex,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_diagnostic_page_copy_record_value")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CopyObservedDiagnosticPageRecordValue(
        ulong pageHandle,
        uint recordIndex,
        uint* kind,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_observed_diagnostic_page_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseObservedDiagnosticPage(ulong pageHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_create")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CreateSession(
        NativeSessionOptions* options,
        ulong* sessionHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_preflight")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int PreflightSession(
        NativeSessionOptions* options,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseSession(ulong sessionHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_create_builder")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CreateSessionBuilder(
        ulong sessionHandle,
        ulong* builderHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_get_snapshot")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetSessionSnapshot(
        ulong sessionHandle,
        NativeSessionSnapshot* snapshot,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_get_event_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetSessionEventInfo(
        ulong sessionHandle,
        uint eventIndex,
        ulong* sequence,
        uint* state,
        uint* flags,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_set_variable")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetSessionVariable(
        ulong sessionHandle,
        NativeUtf8Span name,
        NativeDataValue* value,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_set_live_object_variable")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetSessionLiveObjectVariable(
        ulong sessionHandle,
        NativeUtf8Span name,
        nint comObject,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_set_live_object_contract_variable")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetSessionLiveObjectContractVariable(
        ulong sessionHandle,
        NativeUtf8Span name,
        NativeLiveObjectContractDescriptor* contract,
        nint comObject,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_remove_variable")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int RemoveSessionVariable(
        ulong sessionHandle,
        NativeUtf8Span name,
        uint* removed,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_get_variable_snapshot")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int GetSessionVariableSnapshot(
        ulong sessionHandle,
        NativeUtf8Span name,
        uint* found,
        uint* kind,
        byte* buffer,
        nuint bufferLength,
        nuint* requiredLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_capability_register")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int RegisterCapabilities(
        NativeCapabilityRegistration* registration,
        ulong* capabilityHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_capability_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int ReleaseCapabilities(ulong capabilityHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_set_capabilities")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetCapabilities(
        ulong builderHandle,
        ulong capabilityHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_session_pool_create")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int CreateSessionPool(
        NativeSessionPoolOptions* options,
        ulong* poolHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_open")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerOpen(
        NativeBrokerChannelOptions* options,
        ulong* channelHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_close")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerClose(ulong channelHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_wait")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerWait(
        ulong channelHandle,
        uint timeoutMilliseconds,
        ulong* frameHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_frame_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerFrameGetInfo(
        ulong frameHandle,
        NativeBrokerFrameInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_frame_read")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerFrameRead(
        ulong frameHandle,
        byte* buffer,
        uint capacity,
        uint* required,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_frame_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerFrameRelease(ulong frameHandle, NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_observe")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerObserve(
        ulong channelHandle,
        ulong correlationId,
        ulong* observationHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_observation_get_info")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerObservationGetInfo(
        ulong observationHandle,
        NativeBrokerTerminalInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_observation_wait")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerObservationWait(
        ulong observationHandle,
        uint timeoutMilliseconds,
        NativeBrokerTerminalInfo* info,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_observation_release")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerObservationRelease(
        ulong observationHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_reply")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerReply(
        ulong channelHandle,
        ulong correlationId,
        byte* body,
        uint bodyLength,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_reply_error")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerReplyError(
        ulong channelHandle,
        ulong correlationId,
        int code,
        NativeUtf8Span message,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_broker_cancel")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int BrokerCancel(
        ulong channelHandle,
        ulong correlationId,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_set_broker")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetBroker(
        ulong builderHandle,
        ulong channelHandle,
        NativeCallResult* result);

    [LibraryImport(LibraryName, EntryPoint = "multi_pwsh_set_bridge")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial int SetBridge(
        ulong builderHandle,
        ulong channelHandle,
        ulong bindingId,
        ulong contractIdLow,
        ulong contractIdHigh,
        ushort contractMajorVersion,
        ushort contractMinorVersion,
        uint maximumRequestBytes,
        uint maximumReplyBytes,
        NativeUtf8Span variableName,
        NativeCallResult* result);
}
