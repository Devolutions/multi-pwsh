use std::convert::TryFrom;
use std::fmt;
use std::mem;
use std::sync::Arc;

use crate::delegate_loader::{AssemblyDelegateLoader, MethodWithUnknownSignature};
use crate::error::Error;
use crate::loader::HostedRuntime;
use crate::pdcstr;
use crate::pdcstring::{PdCStr, PdCString};

use super::bindings_generated::PowerShellHandle;

const FFI_BINDINGS_ABI_VERSION: u32 = 1;
const FFI_CALL_DIAGNOSTIC_CAPACITY: usize = 4096;
const FFI_FEATURE_ASYNC_OPERATION_PRIMITIVES: u64 = 1 << 8;
const FFI_FEATURE_SESSION_PRIMITIVES: u64 = 1 << 10;
const FFI_FEATURE_SESSION_POLLING: u64 = 1 << 11;
const FFI_FEATURE_SNAPSHOT_PROJECTIONS: u64 = 1 << 13;
const FFI_FEATURE_SESSION_CONFIGURATION: u64 = 1 << 14;
const FFI_FEATURE_SESSION_VARIABLES: u64 = 1 << 15;
const FFI_FEATURE_CAPABILITY_RPC: u64 = 1 << 16;
const FFI_FEATURE_LIVE_OBJECT_PROBE: u64 = 1 << 17;
const FFI_FEATURE_LIVE_SESSION_OBJECT_PROBE: u64 = 1 << 18;
const FFI_FEATURE_LIVE_OBJECT_CONTRACTS: u64 = 1 << 19;
const FFI_FEATURE_LIVE_STREAM_POLLING: u64 = 1 << 20;
const FFI_FEATURE_TYPED_RESULT_PAGING: u64 = 1 << 21;
const FFI_FEATURE_OBSERVED_INVOCATION: u64 = 1 << 22;
const FFI_FEATURE_SESSION_PREFLIGHT: u64 = 1 << 23;
const FFI_FEATURE_RUNTIME_DIAGNOSTICS: u64 = 1 << 24;
const FFI_FEATURE_DUPLEX_BROKER_CHANNEL: u64 = 1 << 25;
const FFI_FEATURE_GENERATED_BRIDGE_ATTACHMENT: u64 = 1 << 26;
const FFI_FEATURE_RELIABLE_BRIDGE_EVENTS: u64 = 1 << 28;
const FFI_FEATURE_OBSERVED_PRESENTATION: u64 = 1 << 29;
const FFI_FEATURE_SECRET_ADAPTERS: u64 = 1 << 30;
const FFI_FEATURE_CREDENTIAL_RESULT: u64 = 1 << 31;
const FFI_FEATURE_REMOTE_PROVIDER_DIAGNOSTICS: u64 = 1 << 32;
const FFI_REQUIRED_FEATURES: u64 = FFI_FEATURE_ASYNC_OPERATION_PRIMITIVES
    | FFI_FEATURE_SESSION_PRIMITIVES
    | FFI_FEATURE_SESSION_POLLING
    | FFI_FEATURE_SNAPSHOT_PROJECTIONS
    | FFI_FEATURE_SESSION_CONFIGURATION
    | FFI_FEATURE_SESSION_VARIABLES
    | FFI_FEATURE_CAPABILITY_RPC
    | FFI_FEATURE_LIVE_OBJECT_PROBE
    | FFI_FEATURE_LIVE_SESSION_OBJECT_PROBE
    | FFI_FEATURE_LIVE_OBJECT_CONTRACTS
    | FFI_FEATURE_LIVE_STREAM_POLLING
    | FFI_FEATURE_TYPED_RESULT_PAGING
    | FFI_FEATURE_OBSERVED_INVOCATION
    | FFI_FEATURE_SESSION_PREFLIGHT
    | FFI_FEATURE_RUNTIME_DIAGNOSTICS
    | FFI_FEATURE_DUPLEX_BROKER_CHANNEL
    | FFI_FEATURE_GENERATED_BRIDGE_ATTACHMENT
    | FFI_FEATURE_RELIABLE_BRIDGE_EVENTS
    | FFI_FEATURE_OBSERVED_PRESENTATION
    | FFI_FEATURE_SECRET_ADAPTERS
    | FFI_FEATURE_CREDENTIAL_RESULT
    | FFI_FEATURE_REMOTE_PROVIDER_DIAGNOSTICS;
const STATUS_SUCCESS: i32 = 0;
const STATUS_BUFFER_TOO_SMALL: i32 = 1;
const VALUE_KIND_PROPERTY_BAG: u32 = 14;

#[repr(C)]
struct FfiCallResult {
    size: u32,
    status: i32,
    flags: u32,
    diagnostic: *mut u8,
    diagnostic_capacity: i32,
    diagnostic_required_length: i32,
    diagnostic_written_length: i32,
}

#[repr(C)]
pub struct FfiCredentialResult {
    pub size: u32,
    pub is_cancelled: u32,
    pub username: *mut u8,
    pub username_capacity: i32,
    pub username_length: i32,
    pub domain: *mut u8,
    pub domain_capacity: i32,
    pub domain_length: i32,
    pub password: *mut u16,
    pub password_capacity: i32,
    pub password_length: i32,
    pub output_messages: *mut u8,
    pub output_messages_capacity: i32,
    pub output_messages_length: i32,
    pub error_messages: *mut u8,
    pub error_messages_capacity: i32,
    pub error_messages_length: i32,
    pub log_message: *mut u8,
    pub log_message_capacity: i32,
    pub log_message_length: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiLiveObjectContractDescriptor {
    pub size: u32,
    pub directions: u32,
    pub interface_id_low: u64,
    pub interface_id_high: u64,
    pub major_version: u16,
    pub minor_version: u16,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FfiApiV1Header {
    size: usize,
    abi_version: u32,
    feature_flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FfiApiV1 {
    size: usize,
    abi_version: u32,
    feature_flags: u64,
    create_fn: *const libc::c_void,
    release_fn: *const libc::c_void,
    add_argument_utf8_fn: *const libc::c_void,
    add_parameter_string_utf8_fn: *const libc::c_void,
    add_parameter_int64_fn: *const libc::c_void,
    add_command_utf8_fn: *const libc::c_void,
    add_script_utf8_fn: *const libc::c_void,
    add_statement_fn: *const libc::c_void,
    invoke_to_utf8_fn: *const libc::c_void,
    get_invocation_error_count_fn: *const libc::c_void,
    copy_invocation_error_field_to_utf8_fn: *const libc::c_void,
    clear_fn: *const libc::c_void,
    stop_fn: *const libc::c_void,
    invoke_to_result_fn: *const libc::c_void,
    invocation_result_release_fn: *const libc::c_void,
    invocation_result_get_info_fn: *const libc::c_void,
    invocation_result_get_stream_info_fn: *const libc::c_void,
    invocation_result_get_stream_record_info_fn: *const libc::c_void,
    invocation_result_copy_stream_record_field_to_utf8_fn: *const libc::c_void,
    invocation_result_get_sequence_record_fn: *const libc::c_void,
    add_command_utf8_local_fn: *const libc::c_void,
    add_script_utf8_local_fn: *const libc::c_void,
    add_argument_value_fn: *const libc::c_void,
    add_parameter_value_fn: *const libc::c_void,
    add_parameter_switch_fn: *const libc::c_void,
    add_input_value_fn: *const libc::c_void,
    complete_input_fn: *const libc::c_void,
    reset_input_fn: *const libc::c_void,
    invocation_result_get_metadata_fn: *const libc::c_void,
    session_create_fn: *const libc::c_void,
    session_release_fn: *const libc::c_void,
    session_create_builder_fn: *const libc::c_void,
    session_get_snapshot_fn: *const libc::c_void,
    session_get_event_info_fn: *const libc::c_void,
    invocation_result_get_stream_totals_fn: *const libc::c_void,
    invocation_result_get_stream_record_projection_info_fn: *const libc::c_void,
    invocation_result_copy_stream_record_value_fn: *const libc::c_void,
    session_create_configured_fn: *const libc::c_void,
    session_set_variable_fn: *const libc::c_void,
    session_remove_variable_fn: *const libc::c_void,
    session_get_variable_snapshot_fn: *const libc::c_void,
    power_shell_set_capability_context_fn: *const libc::c_void,
    live_object_probe_create_fn: *const libc::c_void,
    live_object_probe_release_fn: *const libc::c_void,
    live_object_probe_unregister_fn: *const libc::c_void,
    power_shell_add_argument_live_object_fn: *const libc::c_void,
    power_shell_session_set_live_object_variable_fn: *const libc::c_void,
    live_object_contract_pack_register_fn: *const libc::c_void,
    power_shell_session_set_live_object_contract_variable_fn: *const libc::c_void,
    live_object_contract_pack_register_many_fn: *const libc::c_void,
    power_shell_begin_live_invocation_fn: *const libc::c_void,
    live_invocation_poll_fn: *const libc::c_void,
    live_invocation_read_batch_fn: *const libc::c_void,
    live_invocation_batch_get_info_fn: *const libc::c_void,
    live_invocation_batch_get_record_info_fn: *const libc::c_void,
    live_invocation_batch_copy_record_text_to_utf8_fn: *const libc::c_void,
    live_invocation_batch_release_fn: *const libc::c_void,
    live_invocation_complete_fn: *const libc::c_void,
    live_invocation_stop_fn: *const libc::c_void,
    live_invocation_release_fn: *const libc::c_void,
    power_shell_begin_typed_result_invocation_fn: *const libc::c_void,
    typed_result_invocation_poll_fn: *const libc::c_void,
    typed_result_invocation_read_page_fn: *const libc::c_void,
    typed_result_invocation_complete_fn: *const libc::c_void,
    typed_result_invocation_stop_fn: *const libc::c_void,
    typed_result_invocation_release_fn: *const libc::c_void,
    typed_result_page_get_info_fn: *const libc::c_void,
    typed_result_page_get_record_info_fn: *const libc::c_void,
    typed_result_page_copy_record_value_fn: *const libc::c_void,
    typed_result_page_release_fn: *const libc::c_void,
    power_shell_begin_observed_invocation_fn: *const libc::c_void,
    observed_invocation_poll_fn: *const libc::c_void,
    observed_invocation_read_result_page_fn: *const libc::c_void,
    observed_invocation_read_diagnostic_page_fn: *const libc::c_void,
    observed_invocation_complete_fn: *const libc::c_void,
    observed_invocation_stop_fn: *const libc::c_void,
    observed_invocation_release_fn: *const libc::c_void,
    observed_diagnostic_page_get_info_fn: *const libc::c_void,
    observed_diagnostic_page_get_record_info_fn: *const libc::c_void,
    observed_diagnostic_page_copy_record_text_to_utf8_fn: *const libc::c_void,
    observed_diagnostic_page_release_fn: *const libc::c_void,
    session_preflight_configured_fn: *const libc::c_void,
    runtime_diagnostics_copy_power_shell_file_version_utf8_fn: *const libc::c_void,
    power_shell_set_broker_context_fn: *const libc::c_void,
    power_shell_set_bridge_context_fn: *const libc::c_void,
    observed_diagnostic_page_copy_record_value_fn: *const libc::c_void,
    power_shell_invoke_secret_result_fn: *const libc::c_void,
    power_shell_invoke_credential_result_fn: *const libc::c_void,
    runtime_diagnostics_get_devolutions_wsman_provider_info_fn: *const libc::c_void,
}

type FnBindingsGetFfiApiV1 = unsafe extern "system" fn() -> *const FfiApiV1;
type FnFfiPowerShellCreate = unsafe extern "system" fn(*mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellRelease = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddUtf8 = unsafe extern "system" fn(PowerShellHandle, *const u8, i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddParameterStringUtf8 =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, *const u8, i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddParameterInt64 =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, i64, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddStatement = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellInvokeToUtf8 =
    unsafe extern "system" fn(PowerShellHandle, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellGetInvocationErrorCount =
    unsafe extern "system" fn(PowerShellHandle, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellCopyInvocationErrorFieldToUtf8 =
    unsafe extern "system" fn(PowerShellHandle, i32, i32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellClear = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellStop = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellInvokeToResult =
    unsafe extern "system" fn(PowerShellHandle, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellInvokeSecretResult = unsafe extern "system" fn(
    PowerShellHandle,
    u32,
    *mut u8,
    i32,
    *mut i32,
    *mut u16,
    i32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellInvokeCredentialResult =
    unsafe extern "system" fn(PowerShellHandle, *mut FfiCredentialResult, *mut FfiCallResult) -> i32;
type FnFfiPowerShellBeginLiveInvocation =
    unsafe extern "system" fn(PowerShellHandle, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationPoll = unsafe extern "system" fn(PowerShellHandle, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationReadBatch =
    unsafe extern "system" fn(PowerShellHandle, i64, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationBatchGetInfo =
    unsafe extern "system" fn(PowerShellHandle, *mut i64, *mut i64, *mut i64, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationBatchGetRecordInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i32, *mut i64, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationBatchCopyRecordTextToUtf8 =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationRelease = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiLiveInvocationComplete =
    unsafe extern "system" fn(PowerShellHandle, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellBeginTypedResultInvocation =
    unsafe extern "system" fn(PowerShellHandle, i32, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiTypedResultInvocationPoll = unsafe extern "system" fn(PowerShellHandle, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiTypedResultInvocationReadPage =
    unsafe extern "system" fn(PowerShellHandle, i64, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiTypedResultInvocationComplete = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiTypedResultPageGetInfo = unsafe extern "system" fn(
    PowerShellHandle,
    *mut i64,
    *mut i64,
    *mut i64,
    *mut i64,
    *mut i32,
    *mut u32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiTypedResultPageGetRecordInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i64, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiTypedResultPageCopyRecordValue =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut u32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellBeginObservedInvocation =
    unsafe extern "system" fn(PowerShellHandle, i32, i32, i32, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiObservedInvocationPoll = unsafe extern "system" fn(PowerShellHandle, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiObservedInvocationReadResultPage =
    unsafe extern "system" fn(PowerShellHandle, i64, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiObservedInvocationReadDiagnosticPage =
    unsafe extern "system" fn(PowerShellHandle, i64, i32, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiObservedInvocationComplete = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiObservedDiagnosticPageGetInfo = unsafe extern "system" fn(
    PowerShellHandle,
    *mut i64,
    *mut i64,
    *mut i64,
    *mut i64,
    *mut i32,
    *mut u32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiObservedDiagnosticPageGetRecordInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i32, *mut i64, *mut FfiCallResult) -> i32;
type FnFfiObservedDiagnosticPageCopyRecordTextToUtf8 =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiObservedDiagnosticPageCopyRecordValue =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut u32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiRuntimeDiagnosticsCopyPowerShellFileVersionUtf8 =
    unsafe extern "system" fn(*mut u8, i32, *mut i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiRuntimeDiagnosticsGetDevolutionsWsManProviderInfo =
    unsafe extern "system" fn(*mut u32, *mut u32, *mut u64, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultRelease = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetInfo =
    unsafe extern "system" fn(PowerShellHandle, *mut u32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetStreamInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i32, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetStreamRecordInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, i32, *mut i64, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultCopyStreamRecordFieldToUtf8 =
    unsafe extern "system" fn(PowerShellHandle, i32, i32, i32, *mut u8, i32, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetSequenceRecord =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i32, *mut i32, *mut i64, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddScopedUtf8 =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddValue =
    unsafe extern "system" fn(PowerShellHandle, u32, *const u8, i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddParameterValue =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, u32, *const u8, i32, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetMetadata =
    unsafe extern "system" fn(PowerShellHandle, *mut u32, *mut i64, *mut i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionCreate = unsafe extern "system" fn(
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    *const u8,
    i32,
    *mut PowerShellHandle,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSessionCreateConfigured = unsafe extern "system" fn(
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *mut PowerShellHandle,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSessionPreflightConfigured = unsafe extern "system" fn(
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *const u8,
    i32,
    *mut u8,
    i32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSessionRelease = unsafe extern "system" fn(PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionCreateBuilder =
    unsafe extern "system" fn(PowerShellHandle, *mut PowerShellHandle, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionGetSnapshot = unsafe extern "system" fn(
    PowerShellHandle,
    *mut u32,
    *mut u32,
    *mut u32,
    *mut u32,
    *mut u32,
    *mut i64,
    *mut i64,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSessionGetEventInfo =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i64, *mut u32, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionSetVariable =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, u32, *const u8, i32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionSetLiveObjectVariable =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, *mut libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiLiveObjectContractPackRegister = unsafe extern "system" fn(*mut libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiLiveObjectContractPackRegisterMany =
    unsafe extern "system" fn(*const *mut libc::c_void, u32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionSetLiveObjectContractVariable = unsafe extern "system" fn(
    PowerShellHandle,
    *const u8,
    i32,
    *const FfiLiveObjectContractDescriptor,
    *mut libc::c_void,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSessionRemoveVariable =
    unsafe extern "system" fn(PowerShellHandle, *const u8, i32, *mut u32, *mut FfiCallResult) -> i32;
type FnFfiPowerShellSessionGetVariableSnapshot = unsafe extern "system" fn(
    PowerShellHandle,
    *const u8,
    i32,
    *mut u32,
    *mut u32,
    *mut u8,
    i32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSetBrokerContext = unsafe extern "system" fn(
    PowerShellHandle,
    u64,
    u64,
    *const libc::c_void,
    *const libc::c_void,
    u32,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSetBridgeContext = unsafe extern "system" fn(
    PowerShellHandle,
    u64,
    u64,
    u64,
    u16,
    u16,
    u32,
    u32,
    *const u8,
    i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiPowerShellSetCapabilityContext =
    unsafe extern "system" fn(PowerShellHandle, u64, u64, *const libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetStreamTotals =
    unsafe extern "system" fn(PowerShellHandle, i32, *mut i64, *mut i64, *mut FfiCallResult) -> i32;
type FnFfiInvocationResultGetStreamRecordProjectionInfo = unsafe extern "system" fn(
    PowerShellHandle,
    i32,
    i32,
    *mut i32,
    *mut i32,
    *mut i32,
    *mut i32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiInvocationResultCopyStreamRecordValue = unsafe extern "system" fn(
    PowerShellHandle,
    i32,
    i32,
    i32,
    *mut u32,
    *mut u8,
    i32,
    *mut i32,
    *mut FfiCallResult,
) -> i32;
type FnFfiLiveObjectProbeCreate = unsafe extern "system" fn(i64, *mut *mut libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiLiveObjectProbeRelease = unsafe extern "system" fn(*mut libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiLiveObjectProbeUnregister = unsafe extern "system" fn(*mut libc::c_void, *mut FfiCallResult) -> i32;
type FnFfiPowerShellAddLiveObject =
    unsafe extern "system" fn(PowerShellHandle, *mut libc::c_void, *mut FfiCallResult) -> i32;

#[derive(Clone, Copy)]
pub(crate) struct FfiBindings {
    abi_version: u32,
    payload_table_size: usize,
    create_fn: FnFfiPowerShellCreate,
    release_fn: FnFfiPowerShellRelease,
    add_argument_utf8_fn: FnFfiPowerShellAddUtf8,
    add_parameter_string_utf8_fn: FnFfiPowerShellAddParameterStringUtf8,
    add_parameter_int64_fn: FnFfiPowerShellAddParameterInt64,
    add_command_utf8_fn: FnFfiPowerShellAddUtf8,
    add_script_utf8_fn: FnFfiPowerShellAddUtf8,
    add_statement_fn: FnFfiPowerShellAddStatement,
    invoke_to_utf8_fn: FnFfiPowerShellInvokeToUtf8,
    get_invocation_error_count_fn: FnFfiPowerShellGetInvocationErrorCount,
    copy_invocation_error_field_to_utf8_fn: FnFfiPowerShellCopyInvocationErrorFieldToUtf8,
    clear_fn: FnFfiPowerShellClear,
    stop_fn: FnFfiPowerShellStop,
    invoke_to_result_fn: FnFfiPowerShellInvokeToResult,
    invocation_result_release_fn: FnFfiInvocationResultRelease,
    invocation_result_get_info_fn: FnFfiInvocationResultGetInfo,
    invocation_result_get_stream_info_fn: FnFfiInvocationResultGetStreamInfo,
    invocation_result_get_stream_record_info_fn: FnFfiInvocationResultGetStreamRecordInfo,
    invocation_result_copy_stream_record_field_to_utf8_fn: FnFfiInvocationResultCopyStreamRecordFieldToUtf8,
    invocation_result_get_sequence_record_fn: FnFfiInvocationResultGetSequenceRecord,
    add_command_utf8_local_fn: FnFfiPowerShellAddScopedUtf8,
    add_script_utf8_local_fn: FnFfiPowerShellAddScopedUtf8,
    add_argument_value_fn: FnFfiPowerShellAddValue,
    add_parameter_value_fn: FnFfiPowerShellAddParameterValue,
    add_parameter_switch_fn: FnFfiPowerShellAddUtf8,
    add_input_value_fn: FnFfiPowerShellAddValue,
    complete_input_fn: FnFfiPowerShellAddStatement,
    reset_input_fn: FnFfiPowerShellAddStatement,
    invocation_result_get_metadata_fn: FnFfiInvocationResultGetMetadata,
    session_create_fn: FnFfiPowerShellSessionCreate,
    session_release_fn: FnFfiPowerShellSessionRelease,
    session_create_builder_fn: FnFfiPowerShellSessionCreateBuilder,
    session_get_snapshot_fn: FnFfiPowerShellSessionGetSnapshot,
    session_get_event_info_fn: FnFfiPowerShellSessionGetEventInfo,
    invocation_result_get_stream_totals_fn: FnFfiInvocationResultGetStreamTotals,
    invocation_result_get_stream_record_projection_info_fn: FnFfiInvocationResultGetStreamRecordProjectionInfo,
    invocation_result_copy_stream_record_value_fn: FnFfiInvocationResultCopyStreamRecordValue,
    session_create_configured_fn: FnFfiPowerShellSessionCreateConfigured,
    session_set_variable_fn: FnFfiPowerShellSessionSetVariable,
    session_remove_variable_fn: FnFfiPowerShellSessionRemoveVariable,
    session_get_variable_snapshot_fn: FnFfiPowerShellSessionGetVariableSnapshot,
    power_shell_set_capability_context_fn: FnFfiPowerShellSetCapabilityContext,
    live_object_probe_create_fn: FnFfiLiveObjectProbeCreate,
    live_object_probe_release_fn: FnFfiLiveObjectProbeRelease,
    live_object_probe_unregister_fn: FnFfiLiveObjectProbeUnregister,
    power_shell_add_argument_live_object_fn: FnFfiPowerShellAddLiveObject,
    power_shell_session_set_live_object_variable_fn: FnFfiPowerShellSessionSetLiveObjectVariable,
    live_object_contract_pack_register_fn: FnFfiLiveObjectContractPackRegister,
    power_shell_session_set_live_object_contract_variable_fn: FnFfiPowerShellSessionSetLiveObjectContractVariable,
    live_object_contract_pack_register_many_fn: FnFfiLiveObjectContractPackRegisterMany,
    live_stream: FfiLiveStreamBindings,
    typed_result_paging: FfiTypedResultPagingBindings,
    observed_invocation: FfiObservedInvocationBindings,
    session_preflight_configured_fn: FnFfiPowerShellSessionPreflightConfigured,
    runtime_diagnostics_copy_power_shell_file_version_utf8_fn: FnFfiRuntimeDiagnosticsCopyPowerShellFileVersionUtf8,
    power_shell_set_broker_context_fn: FnFfiPowerShellSetBrokerContext,
    power_shell_set_bridge_context_fn: FnFfiPowerShellSetBridgeContext,
    power_shell_invoke_secret_result_fn: FnFfiPowerShellInvokeSecretResult,
    power_shell_invoke_credential_result_fn: FnFfiPowerShellInvokeCredentialResult,
    runtime_diagnostics_get_devolutions_wsman_provider_info_fn: FnFfiRuntimeDiagnosticsGetDevolutionsWsManProviderInfo,
}

pub struct FfiPayloadRuntimeDiagnostics {
    pub bindings_abi_version: u32,
    pub payload_table_size: usize,
    pub payload_table_slot_count: u32,
    pub power_shell_file_version: Option<String>,
    pub devolutions_wsman_provider_status: u32,
    pub devolutions_wsman_provider_unavailable_reason: u32,
    pub devolutions_wsman_provider_capabilities: u64,
}

#[derive(Clone, Copy)]
struct FfiLiveStreamBindings {
    power_shell_begin_live_invocation_fn: FnFfiPowerShellBeginLiveInvocation,
    live_invocation_poll_fn: FnFfiLiveInvocationPoll,
    live_invocation_read_batch_fn: FnFfiLiveInvocationReadBatch,
    live_invocation_batch_get_info_fn: FnFfiLiveInvocationBatchGetInfo,
    live_invocation_batch_get_record_info_fn: FnFfiLiveInvocationBatchGetRecordInfo,
    live_invocation_batch_copy_record_text_to_utf8_fn: FnFfiLiveInvocationBatchCopyRecordTextToUtf8,
    live_invocation_batch_release_fn: FnFfiLiveInvocationRelease,
    live_invocation_complete_fn: FnFfiLiveInvocationComplete,
    live_invocation_stop_fn: FnFfiLiveInvocationRelease,
    live_invocation_release_fn: FnFfiLiveInvocationRelease,
}

#[derive(Clone, Copy)]
struct FfiTypedResultPagingBindings {
    power_shell_begin_typed_result_invocation_fn: FnFfiPowerShellBeginTypedResultInvocation,
    typed_result_invocation_poll_fn: FnFfiTypedResultInvocationPoll,
    typed_result_invocation_read_page_fn: FnFfiTypedResultInvocationReadPage,
    typed_result_invocation_complete_fn: FnFfiTypedResultInvocationComplete,
    typed_result_invocation_stop_fn: FnFfiTypedResultInvocationComplete,
    typed_result_invocation_release_fn: FnFfiTypedResultInvocationComplete,
    typed_result_page_get_info_fn: FnFfiTypedResultPageGetInfo,
    typed_result_page_get_record_info_fn: FnFfiTypedResultPageGetRecordInfo,
    typed_result_page_copy_record_value_fn: FnFfiTypedResultPageCopyRecordValue,
    typed_result_page_release_fn: FnFfiTypedResultInvocationComplete,
}

#[derive(Clone, Copy)]
struct FfiObservedInvocationBindings {
    power_shell_begin_observed_invocation_fn: FnFfiPowerShellBeginObservedInvocation,
    observed_invocation_poll_fn: FnFfiObservedInvocationPoll,
    observed_invocation_read_result_page_fn: FnFfiObservedInvocationReadResultPage,
    observed_invocation_read_diagnostic_page_fn: FnFfiObservedInvocationReadDiagnosticPage,
    observed_invocation_complete_fn: FnFfiObservedInvocationComplete,
    observed_invocation_stop_fn: FnFfiObservedInvocationComplete,
    observed_invocation_release_fn: FnFfiObservedInvocationComplete,
    observed_diagnostic_page_get_info_fn: FnFfiObservedDiagnosticPageGetInfo,
    observed_diagnostic_page_get_record_info_fn: FnFfiObservedDiagnosticPageGetRecordInfo,
    observed_diagnostic_page_copy_record_text_to_utf8_fn: FnFfiObservedDiagnosticPageCopyRecordTextToUtf8,
    observed_diagnostic_page_release_fn: FnFfiObservedInvocationComplete,
    observed_diagnostic_page_copy_record_value_fn: FnFfiObservedDiagnosticPageCopyRecordValue,
}

#[derive(Debug)]
pub struct FfiBindingError {
    status: i32,
    diagnostic: String,
}

impl FfiBindingError {
    fn from_status(status: i32, diagnostic: String) -> Self {
        Self { status, diagnostic }
    }

    pub fn status(&self) -> i32 {
        self.status
    }
}

impl fmt::Display for FfiBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "managed PowerShell binding failed with status {}: {}",
            self.status, self.diagnostic
        )
    }
}

impl std::error::Error for FfiBindingError {}

fn incompatible_ffi_api_error() -> Error {
    Error::IO(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "managed FFI bindings report an incompatible ABI version, table size, or required feature set",
    ))
}

fn validate_ffi_api_header(header: FfiApiV1Header) -> Result<(), Error> {
    if header.abi_version != FFI_BINDINGS_ABI_VERSION
        || header.size < mem::size_of::<FfiApiV1>()
        || header.feature_flags & FFI_REQUIRED_FEATURES != FFI_REQUIRED_FEATURES
    {
        return Err(incompatible_ffi_api_error());
    }

    Ok(())
}

unsafe fn load_ffi_api_v1(get_api_fn: FnBindingsGetFfiApiV1) -> Result<FfiApiV1, Error> {
    let api_ptr = get_api_fn();
    if api_ptr.is_null() {
        return Err(Error::IO(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "managed FFI binding table is null",
        )));
    }

    let header = (api_ptr as *const FfiApiV1Header).read();
    validate_ffi_api_header(header)?;
    Ok(api_ptr.read())
}

impl FfiBindings {
    pub(crate) fn new_with_loader(fn_loader: &AssemblyDelegateLoader<PdCString>) -> Result<Self, Error> {
        fn get_function_pointer(
            fn_loader: &AssemblyDelegateLoader<PdCString>,
            type_name: impl AsRef<PdCStr>,
            method_name: impl AsRef<PdCStr>,
        ) -> Result<MethodWithUnknownSignature, Error> {
            fn_loader.get_function_pointer_for_unmanaged_callers_only_method(type_name, method_name)
        }

        let get_api_fn: FnBindingsGetFfiApiV1 = {
            let fn_ptr = get_function_pointer(
                fn_loader,
                pdcstr!("NativeHost.Bindings, Devolutions.PowerShell.SDK.Bindings"),
                pdcstr!("Bindings_GetFfiApiV1"),
            )?;
            unsafe { mem::transmute(fn_ptr) }
        };

        let api = unsafe { load_ffi_api_v1(get_api_fn)? };
        let fields = [
            api.create_fn,
            api.release_fn,
            api.add_argument_utf8_fn,
            api.add_parameter_string_utf8_fn,
            api.add_parameter_int64_fn,
            api.add_command_utf8_fn,
            api.add_script_utf8_fn,
            api.add_statement_fn,
            api.invoke_to_utf8_fn,
            api.get_invocation_error_count_fn,
            api.copy_invocation_error_field_to_utf8_fn,
            api.clear_fn,
            api.stop_fn,
            api.invoke_to_result_fn,
            api.invocation_result_release_fn,
            api.invocation_result_get_info_fn,
            api.invocation_result_get_stream_info_fn,
            api.invocation_result_get_stream_record_info_fn,
            api.invocation_result_copy_stream_record_field_to_utf8_fn,
            api.invocation_result_get_sequence_record_fn,
            api.add_command_utf8_local_fn,
            api.add_script_utf8_local_fn,
            api.add_argument_value_fn,
            api.add_parameter_value_fn,
            api.add_parameter_switch_fn,
            api.add_input_value_fn,
            api.complete_input_fn,
            api.reset_input_fn,
            api.invocation_result_get_metadata_fn,
            api.session_create_fn,
            api.session_release_fn,
            api.session_create_builder_fn,
            api.session_get_snapshot_fn,
            api.session_get_event_info_fn,
            api.invocation_result_get_stream_totals_fn,
            api.invocation_result_get_stream_record_projection_info_fn,
            api.invocation_result_copy_stream_record_value_fn,
            api.session_create_configured_fn,
            api.session_set_variable_fn,
            api.session_remove_variable_fn,
            api.session_get_variable_snapshot_fn,
            api.power_shell_set_capability_context_fn,
            api.live_object_probe_create_fn,
            api.live_object_probe_release_fn,
            api.live_object_probe_unregister_fn,
            api.power_shell_add_argument_live_object_fn,
            api.power_shell_session_set_live_object_variable_fn,
            api.live_object_contract_pack_register_fn,
            api.power_shell_session_set_live_object_contract_variable_fn,
            api.live_object_contract_pack_register_many_fn,
            api.power_shell_begin_live_invocation_fn,
            api.live_invocation_poll_fn,
            api.live_invocation_read_batch_fn,
            api.live_invocation_batch_get_info_fn,
            api.live_invocation_batch_get_record_info_fn,
            api.live_invocation_batch_copy_record_text_to_utf8_fn,
            api.live_invocation_batch_release_fn,
            api.live_invocation_complete_fn,
            api.live_invocation_stop_fn,
            api.live_invocation_release_fn,
            api.power_shell_begin_typed_result_invocation_fn,
            api.typed_result_invocation_poll_fn,
            api.typed_result_invocation_read_page_fn,
            api.typed_result_invocation_complete_fn,
            api.typed_result_invocation_stop_fn,
            api.typed_result_invocation_release_fn,
            api.typed_result_page_get_info_fn,
            api.typed_result_page_get_record_info_fn,
            api.typed_result_page_copy_record_value_fn,
            api.typed_result_page_release_fn,
            api.power_shell_begin_observed_invocation_fn,
            api.observed_invocation_poll_fn,
            api.observed_invocation_read_result_page_fn,
            api.observed_invocation_read_diagnostic_page_fn,
            api.observed_invocation_complete_fn,
            api.observed_invocation_stop_fn,
            api.observed_invocation_release_fn,
            api.observed_diagnostic_page_get_info_fn,
            api.observed_diagnostic_page_get_record_info_fn,
            api.observed_diagnostic_page_copy_record_text_to_utf8_fn,
            api.observed_diagnostic_page_release_fn,
            api.session_preflight_configured_fn,
            api.runtime_diagnostics_copy_power_shell_file_version_utf8_fn,
            api.power_shell_set_broker_context_fn,
            api.power_shell_set_bridge_context_fn,
            api.observed_diagnostic_page_copy_record_value_fn,
            api.power_shell_invoke_secret_result_fn,
            api.power_shell_invoke_credential_result_fn,
            api.runtime_diagnostics_get_devolutions_wsman_provider_info_fn,
        ];
        if fields.iter().any(|field| field.is_null()) {
            return Err(Error::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "managed FFI binding table contains a null function pointer",
            )));
        }

        let live_stream = FfiLiveStreamBindings {
            power_shell_begin_live_invocation_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellBeginLiveInvocation>(
                    api.power_shell_begin_live_invocation_fn,
                )
            },
            live_invocation_poll_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationPoll>(api.live_invocation_poll_fn)
            },
            live_invocation_read_batch_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationReadBatch>(api.live_invocation_read_batch_fn)
            },
            live_invocation_batch_get_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationBatchGetInfo>(
                    api.live_invocation_batch_get_info_fn,
                )
            },
            live_invocation_batch_get_record_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationBatchGetRecordInfo>(
                    api.live_invocation_batch_get_record_info_fn,
                )
            },
            live_invocation_batch_copy_record_text_to_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationBatchCopyRecordTextToUtf8>(
                    api.live_invocation_batch_copy_record_text_to_utf8_fn,
                )
            },
            live_invocation_batch_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationRelease>(api.live_invocation_batch_release_fn)
            },
            live_invocation_complete_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationComplete>(api.live_invocation_complete_fn)
            },
            live_invocation_stop_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationRelease>(api.live_invocation_stop_fn)
            },
            live_invocation_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveInvocationRelease>(api.live_invocation_release_fn)
            },
        };
        let typed_result_paging = FfiTypedResultPagingBindings {
            power_shell_begin_typed_result_invocation_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellBeginTypedResultInvocation>(
                    api.power_shell_begin_typed_result_invocation_fn,
                )
            },
            typed_result_invocation_poll_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationPoll>(
                    api.typed_result_invocation_poll_fn,
                )
            },
            typed_result_invocation_read_page_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationReadPage>(
                    api.typed_result_invocation_read_page_fn,
                )
            },
            typed_result_invocation_complete_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationComplete>(
                    api.typed_result_invocation_complete_fn,
                )
            },
            typed_result_invocation_stop_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationComplete>(
                    api.typed_result_invocation_stop_fn,
                )
            },
            typed_result_invocation_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationComplete>(
                    api.typed_result_invocation_release_fn,
                )
            },
            typed_result_page_get_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultPageGetInfo>(api.typed_result_page_get_info_fn)
            },
            typed_result_page_get_record_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultPageGetRecordInfo>(
                    api.typed_result_page_get_record_info_fn,
                )
            },
            typed_result_page_copy_record_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultPageCopyRecordValue>(
                    api.typed_result_page_copy_record_value_fn,
                )
            },
            typed_result_page_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiTypedResultInvocationComplete>(
                    api.typed_result_page_release_fn,
                )
            },
        };
        let observed_invocation = FfiObservedInvocationBindings {
            power_shell_begin_observed_invocation_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellBeginObservedInvocation>(
                    api.power_shell_begin_observed_invocation_fn,
                )
            },
            observed_invocation_poll_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationPoll>(api.observed_invocation_poll_fn)
            },
            observed_invocation_read_result_page_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationReadResultPage>(
                    api.observed_invocation_read_result_page_fn,
                )
            },
            observed_invocation_read_diagnostic_page_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationReadDiagnosticPage>(
                    api.observed_invocation_read_diagnostic_page_fn,
                )
            },
            observed_invocation_complete_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationComplete>(
                    api.observed_invocation_complete_fn,
                )
            },
            observed_invocation_stop_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationComplete>(api.observed_invocation_stop_fn)
            },
            observed_invocation_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationComplete>(
                    api.observed_invocation_release_fn,
                )
            },
            observed_diagnostic_page_get_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedDiagnosticPageGetInfo>(
                    api.observed_diagnostic_page_get_info_fn,
                )
            },
            observed_diagnostic_page_get_record_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedDiagnosticPageGetRecordInfo>(
                    api.observed_diagnostic_page_get_record_info_fn,
                )
            },
            observed_diagnostic_page_copy_record_text_to_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedDiagnosticPageCopyRecordTextToUtf8>(
                    api.observed_diagnostic_page_copy_record_text_to_utf8_fn,
                )
            },
            observed_diagnostic_page_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedInvocationComplete>(
                    api.observed_diagnostic_page_release_fn,
                )
            },
            observed_diagnostic_page_copy_record_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiObservedDiagnosticPageCopyRecordValue>(
                    api.observed_diagnostic_page_copy_record_value_fn,
                )
            },
        };

        Ok(Self {
            abi_version: api.abi_version,
            payload_table_size: api.size,
            create_fn: unsafe { mem::transmute::<*const libc::c_void, FnFfiPowerShellCreate>(api.create_fn) },
            release_fn: unsafe { mem::transmute::<*const libc::c_void, FnFfiPowerShellRelease>(api.release_fn) },
            add_argument_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddUtf8>(api.add_argument_utf8_fn)
            },
            add_parameter_string_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddParameterStringUtf8>(
                    api.add_parameter_string_utf8_fn,
                )
            },
            add_parameter_int64_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddParameterInt64>(api.add_parameter_int64_fn)
            },
            add_command_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddUtf8>(api.add_command_utf8_fn)
            },
            add_script_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddUtf8>(api.add_script_utf8_fn)
            },
            add_statement_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddStatement>(api.add_statement_fn)
            },
            invoke_to_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellInvokeToUtf8>(api.invoke_to_utf8_fn)
            },
            get_invocation_error_count_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellGetInvocationErrorCount>(
                    api.get_invocation_error_count_fn,
                )
            },
            copy_invocation_error_field_to_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellCopyInvocationErrorFieldToUtf8>(
                    api.copy_invocation_error_field_to_utf8_fn,
                )
            },
            clear_fn: unsafe { mem::transmute::<*const libc::c_void, FnFfiPowerShellClear>(api.clear_fn) },
            stop_fn: unsafe { mem::transmute::<*const libc::c_void, FnFfiPowerShellStop>(api.stop_fn) },
            invoke_to_result_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellInvokeToResult>(api.invoke_to_result_fn)
            },
            invocation_result_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultRelease>(api.invocation_result_release_fn)
            },
            invocation_result_get_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetInfo>(api.invocation_result_get_info_fn)
            },
            invocation_result_get_stream_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetStreamInfo>(
                    api.invocation_result_get_stream_info_fn,
                )
            },
            invocation_result_get_stream_record_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetStreamRecordInfo>(
                    api.invocation_result_get_stream_record_info_fn,
                )
            },
            invocation_result_copy_stream_record_field_to_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultCopyStreamRecordFieldToUtf8>(
                    api.invocation_result_copy_stream_record_field_to_utf8_fn,
                )
            },
            invocation_result_get_sequence_record_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetSequenceRecord>(
                    api.invocation_result_get_sequence_record_fn,
                )
            },
            add_command_utf8_local_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddScopedUtf8>(api.add_command_utf8_local_fn)
            },
            add_script_utf8_local_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddScopedUtf8>(api.add_script_utf8_local_fn)
            },
            add_argument_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddValue>(api.add_argument_value_fn)
            },
            add_parameter_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddParameterValue>(api.add_parameter_value_fn)
            },
            add_parameter_switch_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddUtf8>(api.add_parameter_switch_fn)
            },
            add_input_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddValue>(api.add_input_value_fn)
            },
            complete_input_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddStatement>(api.complete_input_fn)
            },
            reset_input_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddStatement>(api.reset_input_fn)
            },
            invocation_result_get_metadata_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetMetadata>(
                    api.invocation_result_get_metadata_fn,
                )
            },
            session_create_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionCreate>(api.session_create_fn)
            },
            session_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionRelease>(api.session_release_fn)
            },
            session_create_builder_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionCreateBuilder>(
                    api.session_create_builder_fn,
                )
            },
            session_get_snapshot_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionGetSnapshot>(api.session_get_snapshot_fn)
            },
            session_get_event_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionGetEventInfo>(api.session_get_event_info_fn)
            },
            invocation_result_get_stream_totals_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetStreamTotals>(
                    api.invocation_result_get_stream_totals_fn,
                )
            },
            invocation_result_get_stream_record_projection_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultGetStreamRecordProjectionInfo>(
                    api.invocation_result_get_stream_record_projection_info_fn,
                )
            },
            invocation_result_copy_stream_record_value_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiInvocationResultCopyStreamRecordValue>(
                    api.invocation_result_copy_stream_record_value_fn,
                )
            },
            session_create_configured_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionCreateConfigured>(
                    api.session_create_configured_fn,
                )
            },
            session_preflight_configured_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionPreflightConfigured>(
                    api.session_preflight_configured_fn,
                )
            },
            runtime_diagnostics_copy_power_shell_file_version_utf8_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiRuntimeDiagnosticsCopyPowerShellFileVersionUtf8>(
                    api.runtime_diagnostics_copy_power_shell_file_version_utf8_fn,
                )
            },
            runtime_diagnostics_get_devolutions_wsman_provider_info_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiRuntimeDiagnosticsGetDevolutionsWsManProviderInfo>(
                    api.runtime_diagnostics_get_devolutions_wsman_provider_info_fn,
                )
            },
            session_set_variable_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionSetVariable>(api.session_set_variable_fn)
            },
            session_remove_variable_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionRemoveVariable>(
                    api.session_remove_variable_fn,
                )
            },
            session_get_variable_snapshot_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionGetVariableSnapshot>(
                    api.session_get_variable_snapshot_fn,
                )
            },
            power_shell_set_broker_context_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSetBrokerContext>(
                    api.power_shell_set_broker_context_fn,
                )
            },
            power_shell_set_bridge_context_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSetBridgeContext>(
                    api.power_shell_set_bridge_context_fn,
                )
            },
            power_shell_invoke_secret_result_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellInvokeSecretResult>(
                    api.power_shell_invoke_secret_result_fn,
                )
            },
            power_shell_invoke_credential_result_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellInvokeCredentialResult>(
                    api.power_shell_invoke_credential_result_fn,
                )
            },
            power_shell_set_capability_context_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSetCapabilityContext>(
                    api.power_shell_set_capability_context_fn,
                )
            },
            live_object_probe_create_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveObjectProbeCreate>(api.live_object_probe_create_fn)
            },
            live_object_probe_release_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveObjectProbeRelease>(api.live_object_probe_release_fn)
            },
            live_object_probe_unregister_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveObjectProbeUnregister>(
                    api.live_object_probe_unregister_fn,
                )
            },
            power_shell_add_argument_live_object_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellAddLiveObject>(
                    api.power_shell_add_argument_live_object_fn,
                )
            },
            power_shell_session_set_live_object_variable_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionSetLiveObjectVariable>(
                    api.power_shell_session_set_live_object_variable_fn,
                )
            },
            live_object_contract_pack_register_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveObjectContractPackRegister>(
                    api.live_object_contract_pack_register_fn,
                )
            },
            power_shell_session_set_live_object_contract_variable_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiPowerShellSessionSetLiveObjectContractVariable>(
                    api.power_shell_session_set_live_object_contract_variable_fn,
                )
            },
            live_object_contract_pack_register_many_fn: unsafe {
                mem::transmute::<*const libc::c_void, FnFfiLiveObjectContractPackRegisterMany>(
                    api.live_object_contract_pack_register_many_fn,
                )
            },
            live_stream,
            typed_result_paging,
            observed_invocation,
        })
    }

    pub(crate) fn runtime_diagnostics(&self) -> Result<FfiPayloadRuntimeDiagnostics, FfiBindingError> {
        let power_shell_file_version = self.copy_power_shell_file_version()?;
        let (
            devolutions_wsman_provider_status,
            devolutions_wsman_provider_unavailable_reason,
            devolutions_wsman_provider_capabilities,
        ) = self.devolutions_wsman_provider_info()?;
        Ok(FfiPayloadRuntimeDiagnostics {
            bindings_abi_version: self.abi_version,
            payload_table_size: self.payload_table_size,
            payload_table_slot_count: ((mem::size_of::<FfiApiV1>() - mem::size_of::<FfiApiV1Header>())
                / mem::size_of::<*const libc::c_void>()) as u32,
            power_shell_file_version,
            devolutions_wsman_provider_status,
            devolutions_wsman_provider_unavailable_reason,
            devolutions_wsman_provider_capabilities,
        })
    }

    fn devolutions_wsman_provider_info(&self) -> Result<(u32, u32, u64), FfiBindingError> {
        let mut status = 0_u32;
        let mut unavailable_reason = 0_u32;
        let mut capabilities = 0_u64;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let call_status = unsafe {
            (self.runtime_diagnostics_get_devolutions_wsman_provider_info_fn)(
                &mut status,
                &mut unavailable_reason,
                &mut capabilities,
                &mut call_result,
            )
        };
        check_status(call_status, &call_result, &diagnostic)?;

        let valid = match status {
            0 => (1..=4).contains(&unavailable_reason) && capabilities == 0,
            1 => unavailable_reason == 0 && capabilities & !0x7f == 0 && capabilities & 0b11 == 0b11,
            _ => false,
        };
        if !valid {
            return Err(FfiBindingError::from_status(
                -6,
                "managed runtime diagnostics returned inconsistent Devolutions WSMan provider metadata".to_owned(),
            ));
        }

        Ok((status, unavailable_reason, capabilities))
    }

    fn copy_power_shell_file_version(&self) -> Result<Option<String>, FfiBindingError> {
        let mut available = 0_i32;
        let mut required_length = 0_i32;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.runtime_diagnostics_copy_power_shell_file_version_utf8_fn)(
                std::ptr::null_mut(),
                0,
                &mut required_length,
                &mut available,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        if available == 0 {
            if required_length != 0 {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed runtime diagnostics reported an unavailable file version with a payload".to_owned(),
                ));
            }

            return Ok(None);
        }

        if available != 1 || required_length <= 0 || required_length > 128 {
            return Err(FfiBindingError::from_status(
                -6,
                "managed runtime diagnostics returned invalid PowerShell file version metadata".to_owned(),
            ));
        }

        let mut version = vec![0_u8; required_length as usize];
        call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.runtime_diagnostics_copy_power_shell_file_version_utf8_fn)(
                version.as_mut_ptr(),
                required_length,
                &mut required_length,
                &mut available,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)?;
        if available != 1 || required_length as usize != version.len() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed runtime diagnostics changed the PowerShell file version during copy".to_owned(),
            ));
        }

        let version = String::from_utf8(version).map_err(|_| {
            FfiBindingError::from_status(
                -6,
                "managed runtime diagnostics returned a non-UTF-8 PowerShell file version".to_owned(),
            )
        })?;
        if version.is_empty() || version.contains('\0') {
            return Err(FfiBindingError::from_status(
                -6,
                "managed runtime diagnostics returned an invalid PowerShell file version".to_owned(),
            ));
        }

        Ok(Some(version))
    }

    pub(crate) fn create_live_object_probe(&self, initial_count: i64) -> Result<*mut libc::c_void, FfiBindingError> {
        let mut com_object = std::ptr::null_mut();
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe { (self.live_object_probe_create_fn)(initial_count, &mut com_object, &mut call_result) };
        check_status(status, &call_result, &diagnostic)?;
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed live object probe creation returned a null pointer".to_owned(),
            ));
        }

        Ok(com_object)
    }

    pub(crate) fn release_live_object_probe(&self, com_object: *mut libc::c_void) -> Result<(), FfiBindingError> {
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live object probe pointer is null".to_owned(),
            ));
        }

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe { (self.live_object_probe_release_fn)(com_object, &mut call_result) };
        check_status(status, &call_result, &diagnostic)
    }

    pub(crate) fn unregister_live_object_probe(&self, com_object: *mut libc::c_void) -> Result<(), FfiBindingError> {
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live object probe pointer is null".to_owned(),
            ));
        }

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe { (self.live_object_probe_unregister_fn)(com_object, &mut call_result) };
        check_status(status, &call_result, &diagnostic)
    }

    /// # Safety
    ///
    /// `pack_api` must be a valid `NativeLiveObjectContractPackApi` pointer
    /// produced by an assembly loaded into the active payload runtime.
    pub(crate) unsafe fn register_live_object_contract_pack(
        &self,
        pack_api: *mut libc::c_void,
    ) -> Result<(), FfiBindingError> {
        if pack_api.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live object contract pack API pointer is null".to_owned(),
            ));
        }

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe { (self.live_object_contract_pack_register_fn)(pack_api, &mut call_result) };
        check_status(status, &call_result, &diagnostic)
    }

    /// # Safety
    ///
    /// Every pointer in `pack_apis` must identify a valid
    /// `NativeLiveObjectContractPackApi` from the active payload runtime.
    pub(crate) unsafe fn register_live_object_contract_packs(
        &self,
        pack_apis: &[*mut libc::c_void],
    ) -> Result<(), FfiBindingError> {
        if pack_apis.is_empty() || pack_apis.len() > 16 || pack_apis.iter().any(|api| api.is_null()) {
            return Err(FfiBindingError::from_status(
                -1,
                "live object contract pack inputs are invalid".to_owned(),
            ));
        }

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.live_object_contract_pack_register_many_fn)(
                pack_apis.as_ptr(),
                pack_apis.len() as u32,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)
    }
}

pub struct FfiBridgeContext<'a> {
    pub binding_id: u64,
    pub contract_id_low: u64,
    pub contract_id_high: u64,
    pub contract_major_version: u16,
    pub contract_minor_version: u16,
    pub maximum_request_bytes: u32,
    pub maximum_reply_bytes: u32,
    pub variable_name: &'a str,
}

pub struct FfiPowerShell {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

impl FfiPowerShell {
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new_for_runtime(runtime: Arc<HostedRuntime>) -> Result<Self, Box<dyn std::error::Error>> {
        let bindings = runtime.ffi_bindings();
        let mut handle = std::ptr::null_mut();
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe { (bindings.create_fn)(&mut handle, &mut call_result) };
        check_status(status, &call_result, &diagnostic)?;
        if handle.is_null() {
            return Err(Box::new(FfiBindingError::from_status(
                -6,
                "managed PowerShell creation returned a null handle".to_owned(),
            )));
        }

        Ok(Self {
            _runtime: runtime,
            bindings,
            handle: Some(handle),
        })
    }

    pub fn add_argument_string(&self, argument: &str) -> Result<(), FfiBindingError> {
        self.with_utf8(argument, |handle, bytes, length, result| unsafe {
            (self.bindings.add_argument_utf8_fn)(handle, bytes, length, result)
        })
    }

    pub fn add_parameter_string(&self, name: &str, value: &str) -> Result<(), FfiBindingError> {
        let name_length = checked_utf8_length(name)?;
        let value_length = checked_utf8_length(value)?;
        self.call(|handle, result| unsafe {
            (self.bindings.add_parameter_string_utf8_fn)(
                handle,
                name.as_ptr(),
                name_length,
                value.as_ptr(),
                value_length,
                result,
            )
        })
    }

    pub fn add_parameter_long(&self, name: &str, value: i64) -> Result<(), FfiBindingError> {
        self.with_utf8(name, |handle, bytes, length, result| unsafe {
            (self.bindings.add_parameter_int64_fn)(handle, bytes, length, value, result)
        })
    }

    pub fn add_command(&self, command: &str) -> Result<(), FfiBindingError> {
        self.with_utf8(command, |handle, bytes, length, result| unsafe {
            (self.bindings.add_command_utf8_fn)(handle, bytes, length, result)
        })
    }

    pub fn add_command_scoped(&self, command: &str, use_local_scope: bool) -> Result<(), FfiBindingError> {
        self.with_utf8(command, |handle, bytes, length, result| unsafe {
            (self.bindings.add_command_utf8_local_fn)(handle, bytes, length, i32::from(use_local_scope), result)
        })
    }

    pub fn add_script(&self, script: &str) -> Result<(), FfiBindingError> {
        self.with_utf8(script, |handle, bytes, length, result| unsafe {
            (self.bindings.add_script_utf8_fn)(handle, bytes, length, result)
        })
    }

    pub fn add_script_scoped(&self, script: &str, use_local_scope: bool) -> Result<(), FfiBindingError> {
        self.with_utf8(script, |handle, bytes, length, result| unsafe {
            (self.bindings.add_script_utf8_local_fn)(handle, bytes, length, i32::from(use_local_scope), result)
        })
    }

    pub fn add_argument_value(&self, kind: u32, payload: &[u8]) -> Result<(), FfiBindingError> {
        self.with_value(kind, payload, |handle, kind, bytes, length, result| unsafe {
            (self.bindings.add_argument_value_fn)(handle, kind, bytes, length, result)
        })
    }

    /// # Safety
    ///
    /// `com_object` must be a valid IUnknown pointer for the built-in probe
    /// contract and must remain valid for the managed projection call.
    pub unsafe fn add_argument_live_object(&self, com_object: *mut libc::c_void) -> Result<(), FfiBindingError> {
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live object probe pointer is null".to_owned(),
            ));
        }

        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_add_argument_live_object_fn)(handle, com_object, result)
        })
    }

    pub fn add_parameter_value(&self, name: &str, kind: u32, payload: &[u8]) -> Result<(), FfiBindingError> {
        let name_length = checked_utf8_length(name)?;
        let payload_length = checked_value_length(payload)?;
        self.call(|handle, result| unsafe {
            (self.bindings.add_parameter_value_fn)(
                handle,
                name.as_ptr(),
                name_length,
                kind,
                payload.as_ptr(),
                payload_length,
                result,
            )
        })
    }

    pub fn add_parameter_switch(&self, name: &str) -> Result<(), FfiBindingError> {
        self.with_utf8(name, |handle, bytes, length, result| unsafe {
            (self.bindings.add_parameter_switch_fn)(handle, bytes, length, result)
        })
    }

    pub fn add_input_value(&self, kind: u32, payload: &[u8]) -> Result<(), FfiBindingError> {
        self.with_value(kind, payload, |handle, kind, bytes, length, result| unsafe {
            (self.bindings.add_input_value_fn)(handle, kind, bytes, length, result)
        })
    }

    pub fn complete_input(&self) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe { (self.bindings.complete_input_fn)(handle, result) })
    }

    pub fn reset_input(&self) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe { (self.bindings.reset_input_fn)(handle, result) })
    }

    pub fn add_statement(&self) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe { (self.bindings.add_statement_fn)(handle, result) })
    }

    pub fn clear(&self) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe { (self.bindings.clear_fn)(handle, result) })
    }

    pub fn stop(&self) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe { (self.bindings.stop_fn)(handle, result) })
    }

    pub fn invoke_to_string(&self) -> Result<String, FfiBindingError> {
        let mut required_length = 0;
        let status = self.call_status(|handle, result| unsafe {
            (self.bindings.invoke_to_utf8_fn)(handle, std::ptr::null_mut(), 0, &mut required_length, result)
        })?;
        if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
            return Err(FfiBindingError::from_status(
                status,
                "managed PowerShell invocation failed".to_owned(),
            ));
        }

        let mut output = vec![
            0_u8;
            usize::try_from(required_length).map_err(|_| {
                FfiBindingError::from_status(-1, "managed output length is invalid".to_owned())
            })?
        ];
        let status = self.call_status(|handle, result| unsafe {
            (self.bindings.invoke_to_utf8_fn)(
                handle,
                output.as_mut_ptr(),
                required_length,
                &mut required_length,
                result,
            )
        })?;
        if status != STATUS_SUCCESS {
            return Err(FfiBindingError::from_status(
                status,
                "managed PowerShell output copy failed".to_owned(),
            ));
        }

        String::from_utf8(output)
            .map_err(|_| FfiBindingError::from_status(-6, "managed PowerShell output is not UTF-8".to_owned()))
    }

    pub fn invoke_secret_result(
        &self,
        expected_kind: u32,
        user_name: &mut [u8],
        secret: &mut [u16],
    ) -> Result<(usize, usize), FfiBindingError> {
        let mut user_name_length = 0_i32;
        let mut secret_length = 0_i32;
        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_invoke_secret_result_fn)(
                handle,
                expected_kind,
                user_name.as_mut_ptr(),
                i32::try_from(user_name.len()).unwrap_or(i32::MAX),
                &mut user_name_length,
                secret.as_mut_ptr(),
                i32::try_from(secret.len()).unwrap_or(i32::MAX),
                &mut secret_length,
                result,
            )
        })?;
        let user_name_length = usize::try_from(user_name_length)
            .ok()
            .filter(|length| *length <= user_name.len())
            .ok_or_else(|| {
                FfiBindingError::from_status(-6, "managed credential user name length is invalid".to_owned())
            })?;
        let secret_length = usize::try_from(secret_length)
            .ok()
            .filter(|length| *length <= secret.len())
            .ok_or_else(|| FfiBindingError::from_status(-6, "managed secret length is invalid".to_owned()))?;
        Ok((user_name_length, secret_length))
    }

    /// # Safety
    ///
    /// Every non-null buffer in `credential_result` must be writable for its
    /// declared capacity and remain valid for the duration of the call.
    pub unsafe fn invoke_credential_result(
        &self,
        credential_result: &mut FfiCredentialResult,
    ) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_invoke_credential_result_fn)(handle, credential_result, result)
        })?;
        Ok(())
    }

    pub fn invoke_to_result(&self) -> Result<FfiInvocationResult, FfiBindingError> {
        let mut result_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe { (self.bindings.invoke_to_result_fn)(handle, &mut result_handle, result) })?;
        if result_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed PowerShell invocation returned a null result handle".to_owned(),
            ));
        }

        Ok(FfiInvocationResult {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(result_handle),
        })
    }

    pub fn supports_live_stream_polling(&self) -> bool {
        true
    }

    pub fn supports_typed_result_paging(&self) -> bool {
        true
    }

    pub fn supports_observed_invocation(&self) -> bool {
        true
    }

    pub fn begin_typed_result_invocation(
        &self,
        maximum_buffered_records: u32,
        maximum_page_records: u32,
    ) -> Result<FfiTypedResultInvocation, FfiBindingError> {
        if maximum_buffered_records == 0
            || maximum_buffered_records > 64
            || maximum_page_records == 0
            || maximum_page_records > maximum_buffered_records
        {
            return Err(FfiBindingError::from_status(
                -1,
                "typed result invocation bounds are invalid".to_owned(),
            ));
        }
        let typed_result_paging = self.bindings.typed_result_paging;
        let mut typed_result_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (typed_result_paging.power_shell_begin_typed_result_invocation_fn)(
                handle,
                maximum_buffered_records as i32,
                maximum_page_records as i32,
                &mut typed_result_handle,
                result,
            )
        })?;
        if typed_result_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed typed result invocation returned a null handle".to_owned(),
            ));
        }

        Ok(FfiTypedResultInvocation {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(typed_result_handle),
        })
    }

    pub fn begin_observed_invocation(
        &self,
        maximum_buffered_result_records: u32,
        maximum_result_page_records: u32,
        maximum_buffered_diagnostic_records: u32,
        maximum_diagnostic_page_records: u32,
    ) -> Result<FfiObservedInvocation, FfiBindingError> {
        if maximum_buffered_result_records == 0
            || maximum_buffered_result_records > 64
            || maximum_result_page_records == 0
            || maximum_result_page_records > maximum_buffered_result_records
            || maximum_buffered_diagnostic_records == 0
            || maximum_buffered_diagnostic_records > 64
            || maximum_diagnostic_page_records == 0
            || maximum_diagnostic_page_records > maximum_buffered_diagnostic_records
        {
            return Err(FfiBindingError::from_status(
                -1,
                "observed invocation bounds are invalid".to_owned(),
            ));
        }
        let observed_invocation = self.bindings.observed_invocation;
        let mut observed_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (observed_invocation.power_shell_begin_observed_invocation_fn)(
                handle,
                maximum_buffered_result_records as i32,
                maximum_result_page_records as i32,
                maximum_buffered_diagnostic_records as i32,
                maximum_diagnostic_page_records as i32,
                &mut observed_handle,
                result,
            )
        })?;
        if observed_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed observed invocation returned a null handle".to_owned(),
            ));
        }

        Ok(FfiObservedInvocation {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(observed_handle),
        })
    }

    pub fn begin_live_invocation(&self) -> Result<FfiLiveInvocation, FfiBindingError> {
        let live_stream = self.bindings.live_stream;
        let mut live_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (live_stream.power_shell_begin_live_invocation_fn)(handle, &mut live_handle, result)
        })?;
        if live_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed live invocation returned a null handle".to_owned(),
            ));
        }

        Ok(FfiLiveInvocation {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(live_handle),
        })
    }

    /// # Safety
    ///
    /// `dispatcher` must remain valid for the lifetime of the configured invocation.
    pub unsafe fn set_capability_context(
        &self,
        registration_handle: u64,
        invocation_id: u64,
        dispatcher: *const libc::c_void,
    ) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_set_capability_context_fn)(
                handle,
                registration_handle,
                invocation_id,
                dispatcher,
                result,
            )
        })
    }

    /// # Safety
    ///
    /// `enqueue` and `post` must remain valid for the lifetime of the configured invocation.
    pub unsafe fn set_broker_context(
        &self,
        channel_handle: u64,
        generation: u64,
        enqueue: *const libc::c_void,
        post: *const libc::c_void,
        maximum_body_bytes: u32,
    ) -> Result<(), FfiBindingError> {
        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_set_broker_context_fn)(
                handle,
                channel_handle,
                generation,
                enqueue,
                post,
                maximum_body_bytes,
                result,
            )
        })
    }

    pub fn set_bridge_context(&self, context: &FfiBridgeContext<'_>) -> Result<(), FfiBindingError> {
        let variable_name_length = i32::try_from(context.variable_name.len())
            .map_err(|_| FfiBindingError::from_status(-6, "bridge variable name is too long".to_owned()))?;
        self.call(|handle, result| unsafe {
            (self.bindings.power_shell_set_bridge_context_fn)(
                handle,
                context.binding_id,
                context.contract_id_low,
                context.contract_id_high,
                context.contract_major_version,
                context.contract_minor_version,
                context.maximum_request_bytes,
                context.maximum_reply_bytes,
                context.variable_name.as_ptr(),
                variable_name_length,
                result,
            )
        })
    }

    pub fn invocation_error_count(&self) -> Result<usize, FfiBindingError> {
        let mut count = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.get_invocation_error_count_fn)(handle, &mut count, result)
        })?;
        usize::try_from(count)
            .map_err(|_| FfiBindingError::from_status(-6, "managed error count is invalid".to_owned()))
    }

    pub fn invocation_error_field(&self, error_index: i32, field: i32) -> Result<String, FfiBindingError> {
        let mut required_length = 0;
        let status = self.call_status(|handle, result| unsafe {
            (self.bindings.copy_invocation_error_field_to_utf8_fn)(
                handle,
                error_index,
                field,
                std::ptr::null_mut(),
                0,
                &mut required_length,
                result,
            )
        })?;
        if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
            return Err(FfiBindingError::from_status(
                status,
                "managed invocation error field is unavailable".to_owned(),
            ));
        }

        let mut value = vec![
            0_u8;
            usize::try_from(required_length).map_err(|_| {
                FfiBindingError::from_status(-1, "managed error field length is invalid".to_owned())
            })?
        ];
        self.call(|handle, result| unsafe {
            (self.bindings.copy_invocation_error_field_to_utf8_fn)(
                handle,
                error_index,
                field,
                value.as_mut_ptr(),
                required_length,
                &mut required_length,
                result,
            )
        })?;
        String::from_utf8(value)
            .map_err(|_| FfiBindingError::from_status(-6, "managed invocation error field is not UTF-8".to_owned()))
    }

    fn with_utf8<F>(&self, value: &str, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *const u8, i32, *mut FfiCallResult) -> i32,
    {
        let length = checked_utf8_length(value)?;
        self.call(|handle, result| operation(handle, value.as_ptr(), length, result))
    }

    fn with_value<F>(&self, kind: u32, payload: &[u8], operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, u32, *const u8, i32, *mut FfiCallResult) -> i32,
    {
        let length = checked_value_length(payload)?;
        self.call(|handle, result| operation(handle, kind, payload.as_ptr(), length, result))
    }

    fn call<F>(&self, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let status = self.call_status(operation)?;
        if status == STATUS_SUCCESS {
            Ok(())
        } else {
            Err(FfiBindingError::from_status(
                status,
                "managed PowerShell binding failed".to_owned(),
            ))
        }
    }

    fn call_status<F>(&self, operation: F) -> Result<i32, FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell handle has been released".to_owned()))?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = operation(handle, &mut call_result);
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        Ok(status)
    }
}

impl Drop for FfiPowerShell {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (self.bindings.release_fn)(handle, &mut call_result);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfiLiveStreamRecord {
    pub stream: u32,
    pub sequence: u64,
    pub text: String,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct FfiLiveStreamBatch {
    pub next_sequence: u64,
    pub total_record_count: u64,
    pub lost_record_count: u64,
    pub records: Vec<FfiLiveStreamRecord>,
}

pub struct FfiLiveInvocation {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

impl FfiLiveInvocation {
    pub fn poll(&self) -> Result<bool, FfiBindingError> {
        let live_stream = self.bindings.live_stream;
        let mut completed = 0_i32;
        self.call(|handle, result| unsafe { (live_stream.live_invocation_poll_fn)(handle, &mut completed, result) })?;
        match completed {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FfiBindingError::from_status(
                -6,
                "managed live invocation returned invalid completion metadata".to_owned(),
            )),
        }
    }

    pub fn read_stream_batch(
        &self,
        after_sequence: u64,
        maximum_records: u32,
    ) -> Result<FfiLiveStreamBatch, FfiBindingError> {
        if maximum_records == 0 || maximum_records > 32 || after_sequence > i64::MAX as u64 {
            return Err(FfiBindingError::from_status(
                -1,
                "live stream batch cursor or limit is invalid".to_owned(),
            ));
        }

        let live_stream = self.bindings.live_stream;
        let mut batch_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (live_stream.live_invocation_read_batch_fn)(
                handle,
                after_sequence as i64,
                maximum_records as i32,
                &mut batch_handle,
                result,
            )
        })?;
        if batch_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed live invocation returned a null stream batch handle".to_owned(),
            ));
        }

        let batch = (|| {
            let mut next_sequence = 0_i64;
            let mut total_record_count = 0_i64;
            let mut lost_record_count = 0_i64;
            let mut record_count = 0_i32;
            let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
            let mut call_result = new_call_result(&mut diagnostic);
            let status = unsafe {
                (live_stream.live_invocation_batch_get_info_fn)(
                    batch_handle,
                    &mut next_sequence,
                    &mut total_record_count,
                    &mut lost_record_count,
                    &mut record_count,
                    &mut call_result,
                )
            };
            check_status(status, &call_result, &diagnostic)?;
            let next_sequence = u64::try_from(next_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed live stream next sequence is invalid".to_owned())
            })?;
            let total_record_count = u64::try_from(total_record_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed live stream total is invalid".to_owned()))?;
            let lost_record_count = u64::try_from(lost_record_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed live stream loss count is invalid".to_owned())
            })?;
            if record_count < 0 || record_count > maximum_records as i32 {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed live stream record count is invalid".to_owned(),
                ));
            }

            let mut records = Vec::with_capacity(record_count as usize);
            let mut previous_sequence = after_sequence;
            for index in 0..record_count {
                let mut stream = 0_i32;
                let mut sequence = 0_i64;
                let mut flags = 0_u32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (live_stream.live_invocation_batch_get_record_info_fn)(
                        batch_handle,
                        index,
                        &mut stream,
                        &mut sequence,
                        &mut flags,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                let stream = u32::try_from(stream)
                    .map_err(|_| FfiBindingError::from_status(-6, "managed live stream kind is invalid".to_owned()))?;
                let sequence = u64::try_from(sequence).map_err(|_| {
                    FfiBindingError::from_status(-6, "managed live stream sequence is invalid".to_owned())
                })?;
                if stream >= 7 || sequence == 0 || sequence <= previous_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed live stream records are not ordered".to_owned(),
                    ));
                }
                previous_sequence = sequence;

                let mut required_length = 0_i32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (live_stream.live_invocation_batch_copy_record_text_to_utf8_fn)(
                        batch_handle,
                        index,
                        std::ptr::null_mut(),
                        0,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
                    check_status(status, &call_result, &diagnostic)?;
                }
                if required_length < 0 || required_length as usize > 4096 {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed live stream record text exceeds its bound".to_owned(),
                    ));
                }
                let mut text = vec![0_u8; required_length as usize];
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (live_stream.live_invocation_batch_copy_record_text_to_utf8_fn)(
                        batch_handle,
                        index,
                        text.as_mut_ptr(),
                        required_length,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                if required_length as usize != text.len() {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed live stream record text changed during copy".to_owned(),
                    ));
                }
                records.push(FfiLiveStreamRecord {
                    stream,
                    sequence,
                    text: String::from_utf8(text).map_err(|_| {
                        FfiBindingError::from_status(-6, "managed live stream text is not UTF-8".to_owned())
                    })?,
                    flags,
                });
            }

            if next_sequence < after_sequence
                || next_sequence < previous_sequence
                || lost_record_count > total_record_count
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed live stream batch metadata is invalid".to_owned(),
                ));
            }
            Ok(FfiLiveStreamBatch {
                next_sequence,
                total_record_count,
                lost_record_count,
                records,
            })
        })();

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let release_status = unsafe { (live_stream.live_invocation_batch_release_fn)(batch_handle, &mut call_result) };
        match (batch, check_status(release_status, &call_result, &diagnostic)) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn stop(&self) -> Result<(), FfiBindingError> {
        let live_stream = self.bindings.live_stream;
        self.call(|handle, result| unsafe { (live_stream.live_invocation_stop_fn)(handle, result) })
    }

    pub fn complete(&self) -> Result<FfiInvocationResult, FfiBindingError> {
        let live_stream = self.bindings.live_stream;
        let mut result_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (live_stream.live_invocation_complete_fn)(handle, &mut result_handle, result)
        })?;
        if result_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed live invocation returned a null result handle".to_owned(),
            ));
        }

        Ok(FfiInvocationResult {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(result_handle),
        })
    }

    fn call<F>(&self, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "Live invocation handle has been released".to_owned()))?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = operation(handle, &mut call_result);
        check_status(status, &call_result, &diagnostic)
    }
}

impl Drop for FfiLiveInvocation {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let live_stream = self.bindings.live_stream;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (live_stream.live_invocation_release_fn)(handle, &mut call_result);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfiTypedResultRecord {
    pub sequence: u64,
    pub kind: u32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct FfiTypedResultPage {
    pub acknowledged_sequence: u64,
    pub next_sequence: u64,
    pub total_record_count: u64,
    pub dropped_record_count: u64,
    pub terminal_status: i32,
    pub is_terminal: bool,
    pub is_truncated: bool,
    pub is_complete: bool,
    pub records: Vec<FfiTypedResultRecord>,
}

pub struct FfiTypedResultInvocation {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

impl FfiTypedResultInvocation {
    pub fn poll(&self) -> Result<bool, FfiBindingError> {
        let typed_result_paging = self.bindings.typed_result_paging;
        let mut completed = 0_i32;
        self.call(|handle, result| unsafe {
            (typed_result_paging.typed_result_invocation_poll_fn)(handle, &mut completed, result)
        })?;
        match completed {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FfiBindingError::from_status(
                -6,
                "managed typed result invocation returned invalid completion metadata".to_owned(),
            )),
        }
    }

    pub fn read_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiTypedResultPage, FfiBindingError> {
        if acknowledged_through > i64::MAX as u64 || maximum_records == 0 || maximum_records > 64 {
            return Err(FfiBindingError::from_status(
                -1,
                "typed result page cursor or limit is invalid".to_owned(),
            ));
        }

        let typed_result_paging = self.bindings.typed_result_paging;
        let mut page_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (typed_result_paging.typed_result_invocation_read_page_fn)(
                handle,
                acknowledged_through as i64,
                maximum_records as i32,
                &mut page_handle,
                result,
            )
        })?;
        if page_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed typed result invocation returned a null page handle".to_owned(),
            ));
        }

        let page = (|| {
            let mut acknowledged_sequence = 0_i64;
            let mut next_sequence = 0_i64;
            let mut total_record_count = 0_i64;
            let mut dropped_record_count = 0_i64;
            let mut terminal_status = 0_i32;
            let mut flags = 0_u32;
            let mut record_count = 0_i32;
            let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
            let mut call_result = new_call_result(&mut diagnostic);
            let status = unsafe {
                (typed_result_paging.typed_result_page_get_info_fn)(
                    page_handle,
                    &mut acknowledged_sequence,
                    &mut next_sequence,
                    &mut total_record_count,
                    &mut dropped_record_count,
                    &mut terminal_status,
                    &mut flags,
                    &mut record_count,
                    &mut call_result,
                )
            };
            check_status(status, &call_result, &diagnostic)?;
            let acknowledged_sequence = u64::try_from(acknowledged_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed typed result acknowledgement is invalid".to_owned())
            })?;
            let next_sequence = u64::try_from(next_sequence)
                .map_err(|_| FfiBindingError::from_status(-6, "managed typed result cursor is invalid".to_owned()))?;
            let total_record_count = u64::try_from(total_record_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed typed result total is invalid".to_owned()))?;
            let dropped_record_count = u64::try_from(dropped_record_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed typed result drop count is invalid".to_owned())
            })?;
            if acknowledged_sequence != acknowledged_through
                || next_sequence < acknowledged_sequence
                || next_sequence > total_record_count
                || dropped_record_count > total_record_count
                || record_count < 0
                || record_count > maximum_records as i32
                || flags & !0x7 != 0
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed typed result page metadata is invalid".to_owned(),
                ));
            }

            let is_terminal = flags & 1 != 0;
            let is_truncated = flags & (1 << 1) != 0;
            let is_complete = flags & (1 << 2) != 0;
            if (!is_terminal && terminal_status != 0)
                || (is_complete && (!is_terminal || terminal_status != 0 || is_truncated))
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed typed result terminal metadata is inconsistent".to_owned(),
                ));
            }

            let mut records = Vec::with_capacity(record_count as usize);
            let mut previous_sequence = acknowledged_sequence;
            for index in 0..record_count {
                let mut sequence = 0_i64;
                let mut kind = 0_u32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_get_record_info_fn)(
                        page_handle,
                        index,
                        &mut sequence,
                        &mut kind,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                let sequence = u64::try_from(sequence).map_err(|_| {
                    FfiBindingError::from_status(-6, "managed typed result record sequence is invalid".to_owned())
                })?;
                if kind > 14 || sequence <= previous_sequence || sequence > next_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed typed result records are unordered or unsupported".to_owned(),
                    ));
                }
                previous_sequence = sequence;

                let mut required_length = 0_i32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_copy_record_value_fn)(
                        page_handle,
                        index,
                        &mut kind,
                        std::ptr::null_mut(),
                        0,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
                    check_status(status, &call_result, &diagnostic)?;
                }
                if kind > 14 || required_length < 0 || required_length as usize > 64 * 1024 {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed typed result value exceeds its fixed bound".to_owned(),
                    ));
                }
                let mut payload = vec![0_u8; required_length as usize];
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_copy_record_value_fn)(
                        page_handle,
                        index,
                        &mut kind,
                        payload.as_mut_ptr(),
                        required_length,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                if kind > 14 || required_length as usize != payload.len() {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed typed result value changed during copy".to_owned(),
                    ));
                }
                records.push(FfiTypedResultRecord {
                    sequence,
                    kind,
                    payload,
                });
            }

            if records.is_empty() {
                if next_sequence != acknowledged_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed typed result page cursor is inconsistent".to_owned(),
                    ));
                }
            } else if next_sequence != previous_sequence {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed typed result page cursor is inconsistent".to_owned(),
                ));
            }

            Ok(FfiTypedResultPage {
                acknowledged_sequence,
                next_sequence,
                total_record_count,
                dropped_record_count,
                terminal_status,
                is_terminal,
                is_truncated,
                is_complete,
                records,
            })
        })();

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let release_status =
            unsafe { (typed_result_paging.typed_result_page_release_fn)(page_handle, &mut call_result) };
        match (page, check_status(release_status, &call_result, &diagnostic)) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn complete(&self) -> Result<(), FfiBindingError> {
        let typed_result_paging = self.bindings.typed_result_paging;
        self.call(|handle, result| unsafe { (typed_result_paging.typed_result_invocation_complete_fn)(handle, result) })
    }

    pub fn stop(&self) -> Result<(), FfiBindingError> {
        let typed_result_paging = self.bindings.typed_result_paging;
        self.call(|handle, result| unsafe { (typed_result_paging.typed_result_invocation_stop_fn)(handle, result) })
    }

    fn call<F>(&self, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let handle = self.handle.ok_or_else(|| {
            FfiBindingError::from_status(-4, "Typed result invocation handle has been released".to_owned())
        })?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = operation(handle, &mut call_result);
        check_status(status, &call_result, &diagnostic)
    }
}

impl Drop for FfiTypedResultInvocation {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let typed_result_paging = self.bindings.typed_result_paging;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (typed_result_paging.typed_result_invocation_release_fn)(handle, &mut call_result);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfiObservedDiagnosticRecord {
    pub stream: u32,
    pub sequence: u64,
    pub text: String,
    pub value: Option<FfiTypedResultRecord>,
}

#[derive(Clone, Debug)]
pub struct FfiObservedDiagnosticPage {
    pub acknowledged_sequence: u64,
    pub next_sequence: u64,
    pub total_record_count: u64,
    pub dropped_record_count: u64,
    pub terminal_status: i32,
    pub is_terminal: bool,
    pub is_truncated: bool,
    pub is_complete: bool,
    pub records: Vec<FfiObservedDiagnosticRecord>,
}

pub struct FfiObservedInvocation {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

impl FfiObservedInvocation {
    pub fn poll(&self) -> Result<bool, FfiBindingError> {
        let observed_invocation = self.bindings.observed_invocation;
        let mut completed = 0_i32;
        self.call(|handle, result| unsafe {
            (observed_invocation.observed_invocation_poll_fn)(handle, &mut completed, result)
        })?;
        match completed {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FfiBindingError::from_status(
                -6,
                "managed observed invocation returned invalid completion metadata".to_owned(),
            )),
        }
    }

    pub fn read_result_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiTypedResultPage, FfiBindingError> {
        if acknowledged_through > i64::MAX as u64 || maximum_records == 0 || maximum_records > 64 {
            return Err(FfiBindingError::from_status(
                -1,
                "observed result page cursor or limit is invalid".to_owned(),
            ));
        }

        let observed_invocation = self.bindings.observed_invocation;
        let typed_result_paging = self.bindings.typed_result_paging;
        let mut page_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (observed_invocation.observed_invocation_read_result_page_fn)(
                handle,
                acknowledged_through as i64,
                maximum_records as i32,
                &mut page_handle,
                result,
            )
        })?;
        if page_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed observed invocation returned a null result page handle".to_owned(),
            ));
        }

        let page = (|| {
            let mut acknowledged_sequence = 0_i64;
            let mut next_sequence = 0_i64;
            let mut total_record_count = 0_i64;
            let mut dropped_record_count = 0_i64;
            let mut terminal_status = 0_i32;
            let mut flags = 0_u32;
            let mut record_count = 0_i32;
            let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
            let mut call_result = new_call_result(&mut diagnostic);
            let status = unsafe {
                (typed_result_paging.typed_result_page_get_info_fn)(
                    page_handle,
                    &mut acknowledged_sequence,
                    &mut next_sequence,
                    &mut total_record_count,
                    &mut dropped_record_count,
                    &mut terminal_status,
                    &mut flags,
                    &mut record_count,
                    &mut call_result,
                )
            };
            check_status(status, &call_result, &diagnostic)?;
            let acknowledged_sequence = u64::try_from(acknowledged_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed result acknowledgement is invalid".to_owned())
            })?;
            let next_sequence = u64::try_from(next_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed result cursor is invalid".to_owned())
            })?;
            let total_record_count = u64::try_from(total_record_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed observed result total is invalid".to_owned()))?;
            let dropped_record_count = u64::try_from(dropped_record_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed result drop count is invalid".to_owned())
            })?;
            if acknowledged_sequence != acknowledged_through
                || next_sequence < acknowledged_sequence
                || next_sequence > total_record_count
                || dropped_record_count > total_record_count
                || record_count < 0
                || record_count > maximum_records as i32
                || flags & !0x7 != 0
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed result page metadata is invalid".to_owned(),
                ));
            }

            let is_terminal = flags & 1 != 0;
            let is_truncated = flags & (1 << 1) != 0;
            let is_complete = flags & (1 << 2) != 0;
            if (!is_terminal && terminal_status != 0)
                || (is_complete && (!is_terminal || terminal_status != 0 || is_truncated))
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed result terminal metadata is inconsistent".to_owned(),
                ));
            }

            let mut records = Vec::with_capacity(record_count as usize);
            let mut previous_sequence = acknowledged_sequence;
            for index in 0..record_count {
                let mut sequence = 0_i64;
                let mut kind = 0_u32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_get_record_info_fn)(
                        page_handle,
                        index,
                        &mut sequence,
                        &mut kind,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                let sequence = u64::try_from(sequence).map_err(|_| {
                    FfiBindingError::from_status(-6, "managed observed result record sequence is invalid".to_owned())
                })?;
                if kind > 14 || sequence <= previous_sequence || sequence > next_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed result records are unordered or unsupported".to_owned(),
                    ));
                }
                previous_sequence = sequence;

                let mut required_length = 0_i32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_copy_record_value_fn)(
                        page_handle,
                        index,
                        &mut kind,
                        std::ptr::null_mut(),
                        0,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
                    check_status(status, &call_result, &diagnostic)?;
                }
                if kind > 14 || required_length < 0 || required_length as usize > 64 * 1024 {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed result value exceeds its fixed bound".to_owned(),
                    ));
                }
                let mut payload = vec![0_u8; required_length as usize];
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (typed_result_paging.typed_result_page_copy_record_value_fn)(
                        page_handle,
                        index,
                        &mut kind,
                        payload.as_mut_ptr(),
                        required_length,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                if kind > 14 || required_length as usize != payload.len() {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed result value changed during copy".to_owned(),
                    ));
                }
                records.push(FfiTypedResultRecord {
                    sequence,
                    kind,
                    payload,
                });
            }

            if records.is_empty() {
                if next_sequence != acknowledged_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed result page cursor is inconsistent".to_owned(),
                    ));
                }
            } else if next_sequence != previous_sequence {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed result page cursor is inconsistent".to_owned(),
                ));
            }

            Ok(FfiTypedResultPage {
                acknowledged_sequence,
                next_sequence,
                total_record_count,
                dropped_record_count,
                terminal_status,
                is_terminal,
                is_truncated,
                is_complete,
                records,
            })
        })();

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let release_status =
            unsafe { (typed_result_paging.typed_result_page_release_fn)(page_handle, &mut call_result) };
        match (page, check_status(release_status, &call_result, &diagnostic)) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn read_diagnostic_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiObservedDiagnosticPage, FfiBindingError> {
        if acknowledged_through > i64::MAX as u64 || maximum_records == 0 || maximum_records > 64 {
            return Err(FfiBindingError::from_status(
                -1,
                "observed diagnostic page cursor or limit is invalid".to_owned(),
            ));
        }

        let observed_invocation = self.bindings.observed_invocation;
        let mut page_handle = std::ptr::null_mut();
        self.call(|handle, result| unsafe {
            (observed_invocation.observed_invocation_read_diagnostic_page_fn)(
                handle,
                acknowledged_through as i64,
                maximum_records as i32,
                &mut page_handle,
                result,
            )
        })?;
        if page_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed observed invocation returned a null diagnostic page handle".to_owned(),
            ));
        }

        let page = (|| {
            let mut acknowledged_sequence = 0_i64;
            let mut next_sequence = 0_i64;
            let mut total_record_count = 0_i64;
            let mut dropped_record_count = 0_i64;
            let mut terminal_status = 0_i32;
            let mut flags = 0_u32;
            let mut record_count = 0_i32;
            let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
            let mut call_result = new_call_result(&mut diagnostic);
            let status = unsafe {
                (observed_invocation.observed_diagnostic_page_get_info_fn)(
                    page_handle,
                    &mut acknowledged_sequence,
                    &mut next_sequence,
                    &mut total_record_count,
                    &mut dropped_record_count,
                    &mut terminal_status,
                    &mut flags,
                    &mut record_count,
                    &mut call_result,
                )
            };
            check_status(status, &call_result, &diagnostic)?;
            let acknowledged_sequence = u64::try_from(acknowledged_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed diagnostic acknowledgement is invalid".to_owned())
            })?;
            let next_sequence = u64::try_from(next_sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed diagnostic cursor is invalid".to_owned())
            })?;
            let total_record_count = u64::try_from(total_record_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed diagnostic total is invalid".to_owned())
            })?;
            let dropped_record_count = u64::try_from(dropped_record_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed observed diagnostic drop count is invalid".to_owned())
            })?;
            if acknowledged_sequence != acknowledged_through
                || next_sequence < acknowledged_sequence
                || next_sequence > total_record_count
                || dropped_record_count > total_record_count
                || record_count < 0
                || record_count > maximum_records as i32
                || flags & !0x7 != 0
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed diagnostic page metadata is invalid".to_owned(),
                ));
            }

            let is_terminal = flags & 1 != 0;
            let is_truncated = flags & (1 << 1) != 0;
            let is_complete = flags & (1 << 2) != 0;
            if (!is_terminal && terminal_status != 0)
                || is_truncated
                || (is_complete && (!is_terminal || terminal_status != 0))
            {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed diagnostic terminal metadata is inconsistent".to_owned(),
                ));
            }

            let mut records = Vec::with_capacity(record_count as usize);
            let mut previous_sequence = acknowledged_sequence;
            for index in 0..record_count {
                let mut stream = 0_i32;
                let mut sequence = 0_i64;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (observed_invocation.observed_diagnostic_page_get_record_info_fn)(
                        page_handle,
                        index,
                        &mut stream,
                        &mut sequence,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                let stream = u32::try_from(stream).map_err(|_| {
                    FfiBindingError::from_status(-6, "managed observed diagnostic stream is invalid".to_owned())
                })?;
                let sequence = u64::try_from(sequence).map_err(|_| {
                    FfiBindingError::from_status(
                        -6,
                        "managed observed diagnostic record sequence is invalid".to_owned(),
                    )
                })?;
                if stream >= 7 || sequence <= previous_sequence || sequence > next_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed diagnostic records are unordered or invalid".to_owned(),
                    ));
                }
                previous_sequence = sequence;

                let mut required_length = 0_i32;
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (observed_invocation.observed_diagnostic_page_copy_record_text_to_utf8_fn)(
                        page_handle,
                        index,
                        std::ptr::null_mut(),
                        0,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
                    check_status(status, &call_result, &diagnostic)?;
                }
                if required_length < 0 || required_length as usize > 64 * 1024 {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed diagnostic text exceeds its fixed bound".to_owned(),
                    ));
                }
                let mut text = vec![0_u8; required_length as usize];
                call_result = new_call_result(&mut diagnostic);
                let status = unsafe {
                    (observed_invocation.observed_diagnostic_page_copy_record_text_to_utf8_fn)(
                        page_handle,
                        index,
                        text.as_mut_ptr(),
                        required_length,
                        &mut required_length,
                        &mut call_result,
                    )
                };
                check_status(status, &call_result, &diagnostic)?;
                if required_length as usize != text.len() {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed diagnostic text changed during copy".to_owned(),
                    ));
                }
                let text = String::from_utf8(text).map_err(|_| {
                    FfiBindingError::from_status(-6, "managed observed diagnostic text is not UTF-8".to_owned())
                })?;
                let value = if stream == 6 {
                    let mut kind = 0_u32;
                    let mut required_length = 0_i32;
                    call_result = new_call_result(&mut diagnostic);
                    let status = unsafe {
                        (observed_invocation.observed_diagnostic_page_copy_record_value_fn)(
                            page_handle,
                            index,
                            &mut kind,
                            std::ptr::null_mut(),
                            0,
                            &mut required_length,
                            &mut call_result,
                        )
                    };
                    check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
                    if kind != VALUE_KIND_PROPERTY_BAG || required_length < 0 || required_length as usize > 64 * 1024 {
                        return Err(FfiBindingError::from_status(
                            -6,
                            "managed observed progress value exceeds its fixed bounds".to_owned(),
                        ));
                    }
                    let mut payload = vec![0_u8; required_length as usize];
                    call_result = new_call_result(&mut diagnostic);
                    let status = unsafe {
                        (observed_invocation.observed_diagnostic_page_copy_record_value_fn)(
                            page_handle,
                            index,
                            &mut kind,
                            payload.as_mut_ptr(),
                            required_length,
                            &mut required_length,
                            &mut call_result,
                        )
                    };
                    check_status(status, &call_result, &diagnostic)?;
                    if kind != VALUE_KIND_PROPERTY_BAG || required_length as usize != payload.len() {
                        return Err(FfiBindingError::from_status(
                            -6,
                            "managed observed progress value changed during copy".to_owned(),
                        ));
                    }
                    Some(FfiTypedResultRecord {
                        sequence,
                        kind,
                        payload,
                    })
                } else {
                    None
                };
                records.push(FfiObservedDiagnosticRecord {
                    stream,
                    sequence,
                    text,
                    value,
                });
            }

            if records.is_empty() {
                if next_sequence != acknowledged_sequence {
                    return Err(FfiBindingError::from_status(
                        -6,
                        "managed observed diagnostic page cursor is inconsistent".to_owned(),
                    ));
                }
            } else if next_sequence != previous_sequence {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed observed diagnostic page cursor is inconsistent".to_owned(),
                ));
            }

            Ok(FfiObservedDiagnosticPage {
                acknowledged_sequence,
                next_sequence,
                total_record_count,
                dropped_record_count,
                terminal_status,
                is_terminal,
                is_truncated,
                is_complete,
                records,
            })
        })();

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let release_status =
            unsafe { (observed_invocation.observed_diagnostic_page_release_fn)(page_handle, &mut call_result) };
        match (page, check_status(release_status, &call_result, &diagnostic)) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn complete(&self) -> Result<(), FfiBindingError> {
        let observed_invocation = self.bindings.observed_invocation;
        self.call(|handle, result| unsafe { (observed_invocation.observed_invocation_complete_fn)(handle, result) })
    }

    pub fn stop(&self) -> Result<(), FfiBindingError> {
        let observed_invocation = self.bindings.observed_invocation;
        self.call(|handle, result| unsafe { (observed_invocation.observed_invocation_stop_fn)(handle, result) })
    }

    fn call<F>(&self, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let handle = self.handle.ok_or_else(|| {
            FfiBindingError::from_status(-4, "Observed invocation handle has been released".to_owned())
        })?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = operation(handle, &mut call_result);
        check_status(status, &call_result, &diagnostic)
    }
}

impl Drop for FfiObservedInvocation {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let observed_invocation = self.bindings.observed_invocation;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (observed_invocation.observed_invocation_release_fn)(handle, &mut call_result);
        }
    }
}

pub struct FfiPowerShellSession {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

#[derive(Clone, Copy, Debug)]
pub struct FfiSessionSnapshot {
    pub state: u32,
    pub runspace_state: u32,
    pub flags: u32,
    pub active_pipeline_count: u32,
    pub event_count: u32,
    pub invocation_count: u64,
    pub history_count: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct FfiSessionEvent {
    pub sequence: u64,
    pub state: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct FfiStreamRecordProjectionInfo {
    pub property_entry_count: u32,
    pub dropped_property_entry_count: u32,
    pub type_name_count: u32,
    pub dropped_type_name_count: u32,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct FfiSnapshotValue {
    pub kind: u32,
    pub payload: Vec<u8>,
}

impl FfiPowerShellSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new_for_runtime(
        runtime: Arc<HostedRuntime>,
        runspace_mode: u32,
        initial_configuration: u32,
        history_mode: u32,
        error_preference: u32,
        warning_preference: u32,
        verbose_preference: u32,
        debug_preference: u32,
        information_preference: u32,
        execution_policy: u32,
        initial_variables: &[u8],
        module_imports: &[u8],
        allowed_module_paths: &[u8],
        working_directory: &str,
        environment: &[u8],
    ) -> Result<Self, FfiBindingError> {
        let bindings = runtime.ffi_bindings();
        let mut handle = std::ptr::null_mut();
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let initial_variables_length = checked_value_length(initial_variables)?;
        let module_imports_length = checked_value_length(module_imports)?;
        let module_paths_length = checked_value_length(allowed_module_paths)?;
        let working_directory_length = checked_utf8_length(working_directory)?;
        let environment_length = checked_value_length(environment)?;
        let status = unsafe {
            (bindings.session_create_configured_fn)(
                runspace_mode,
                initial_configuration,
                history_mode,
                error_preference,
                warning_preference,
                verbose_preference,
                debug_preference,
                information_preference,
                execution_policy,
                initial_variables.as_ptr(),
                initial_variables_length,
                module_imports.as_ptr(),
                module_imports_length,
                allowed_module_paths.as_ptr(),
                module_paths_length,
                working_directory.as_ptr(),
                working_directory_length,
                environment.as_ptr(),
                environment_length,
                &mut handle,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        if handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed PowerShell session creation returned a null handle".to_owned(),
            ));
        }

        Ok(Self {
            _runtime: runtime,
            bindings,
            handle: Some(handle),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn preflight_configured(
        runtime: Arc<HostedRuntime>,
        runspace_mode: u32,
        initial_configuration: u32,
        history_mode: u32,
        error_preference: u32,
        warning_preference: u32,
        verbose_preference: u32,
        debug_preference: u32,
        information_preference: u32,
        execution_policy: u32,
        initial_variables: &[u8],
        module_imports: &[u8],
        allowed_module_paths: &[u8],
        working_directory: &str,
        environment: &[u8],
    ) -> Result<Vec<u8>, FfiBindingError> {
        let bindings = runtime.ffi_bindings();
        let initial_variables_length = checked_value_length(initial_variables)?;
        let module_imports_length = checked_value_length(module_imports)?;
        let module_paths_length = checked_value_length(allowed_module_paths)?;
        let working_directory_length = checked_utf8_length(working_directory)?;
        let environment_length = checked_value_length(environment)?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let mut required_length = 0_i32;
        let status = unsafe {
            (bindings.session_preflight_configured_fn)(
                runspace_mode,
                initial_configuration,
                history_mode,
                error_preference,
                warning_preference,
                verbose_preference,
                debug_preference,
                information_preference,
                execution_policy,
                initial_variables.as_ptr(),
                initial_variables_length,
                module_imports.as_ptr(),
                module_imports_length,
                allowed_module_paths.as_ptr(),
                module_paths_length,
                working_directory.as_ptr(),
                working_directory_length,
                environment.as_ptr(),
                environment_length,
                std::ptr::null_mut(),
                0,
                &mut required_length,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        if required_length < 0 || required_length as usize > 64 * 1024 {
            return Err(FfiBindingError::from_status(
                -6,
                "managed PowerShell session preflight report exceeds its fixed bound".to_owned(),
            ));
        }

        let mut payload = vec![0_u8; required_length as usize];
        call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (bindings.session_preflight_configured_fn)(
                runspace_mode,
                initial_configuration,
                history_mode,
                error_preference,
                warning_preference,
                verbose_preference,
                debug_preference,
                information_preference,
                execution_policy,
                initial_variables.as_ptr(),
                initial_variables_length,
                module_imports.as_ptr(),
                module_imports_length,
                allowed_module_paths.as_ptr(),
                module_paths_length,
                working_directory.as_ptr(),
                working_directory_length,
                environment.as_ptr(),
                environment_length,
                payload.as_mut_ptr(),
                required_length,
                &mut required_length,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)?;
        if required_length as usize != payload.len() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed PowerShell session preflight report changed during copy".to_owned(),
            ));
        }

        Ok(payload)
    }

    pub fn create_builder(&self) -> Result<FfiPowerShell, FfiBindingError> {
        let session_handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let mut builder_handle = std::ptr::null_mut();
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status =
            unsafe { (self.bindings.session_create_builder_fn)(session_handle, &mut builder_handle, &mut call_result) };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        if builder_handle.is_null() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed PowerShell session builder creation returned a null handle".to_owned(),
            ));
        }

        Ok(FfiPowerShell {
            _runtime: Arc::clone(&self._runtime),
            bindings: self.bindings,
            handle: Some(builder_handle),
        })
    }

    pub fn snapshot(&self) -> Result<FfiSessionSnapshot, FfiBindingError> {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let mut state = 0;
        let mut runspace_state = 0;
        let mut flags = 0;
        let mut active_pipeline_count = 0;
        let mut event_count = 0;
        let mut invocation_count = 0_i64;
        let mut history_count = 0_i64;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_get_snapshot_fn)(
                handle,
                &mut state,
                &mut runspace_state,
                &mut flags,
                &mut active_pipeline_count,
                &mut event_count,
                &mut invocation_count,
                &mut history_count,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        Ok(FfiSessionSnapshot {
            state,
            runspace_state,
            flags,
            active_pipeline_count,
            event_count,
            invocation_count: u64::try_from(invocation_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed session invocation count is invalid".to_owned())
            })?,
            history_count: u64::try_from(history_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed session history count is invalid".to_owned()))?,
        })
    }

    pub fn event(&self, event_index: u32) -> Result<FfiSessionEvent, FfiBindingError> {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let event_index = i32::try_from(event_index)
            .map_err(|_| FfiBindingError::from_status(-1, "PowerShell session event index is invalid".to_owned()))?;
        let mut sequence = 0_i64;
        let mut state = 0;
        let mut flags = 0;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_get_event_info_fn)(
                handle,
                event_index,
                &mut sequence,
                &mut state,
                &mut flags,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        Ok(FfiSessionEvent {
            sequence: u64::try_from(sequence).map_err(|_| {
                FfiBindingError::from_status(-6, "managed session event sequence is invalid".to_owned())
            })?,
            state,
            flags,
        })
    }

    pub fn set_variable(&self, name: &str, kind: u32, payload: &[u8]) -> Result<(), FfiBindingError> {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let name_length = checked_utf8_length(name)?;
        let payload_length = checked_value_length(payload)?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_set_variable_fn)(
                handle,
                name.as_ptr(),
                name_length,
                kind,
                payload.as_ptr(),
                payload_length,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)
    }

    /// # Safety
    ///
    /// `com_object` must be a valid IUnknown pointer for the built-in probe
    /// contract and must remain valid for the managed projection call.
    pub unsafe fn set_live_object_variable(
        &self,
        name: &str,
        com_object: *mut libc::c_void,
    ) -> Result<(), FfiBindingError> {
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live session object probe pointer is null".to_owned(),
            ));
        }

        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let name_length = checked_utf8_length(name)?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.power_shell_session_set_live_object_variable_fn)(
                handle,
                name.as_ptr(),
                name_length,
                com_object,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)
    }

    /// # Safety
    ///
    /// `com_object` must be a valid IUnknown pointer that implements
    /// `contract` and must remain valid for the managed projection call.
    pub unsafe fn set_live_object_contract_variable(
        &self,
        name: &str,
        contract: &FfiLiveObjectContractDescriptor,
        com_object: *mut libc::c_void,
    ) -> Result<(), FfiBindingError> {
        if com_object.is_null() {
            return Err(FfiBindingError::from_status(
                -1,
                "live object pointer is null".to_owned(),
            ));
        }

        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let name_length = checked_utf8_length(name)?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.power_shell_session_set_live_object_contract_variable_fn)(
                handle,
                name.as_ptr(),
                name_length,
                contract,
                com_object,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)
    }

    pub fn remove_variable(&self, name: &str) -> Result<bool, FfiBindingError> {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let name_length = checked_utf8_length(name)?;
        let mut removed = 0_u32;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_remove_variable_fn)(
                handle,
                name.as_ptr(),
                name_length,
                &mut removed,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)?;
        if removed > 1 {
            return Err(FfiBindingError::from_status(
                -6,
                "managed session variable removal returned an invalid flag".to_owned(),
            ));
        }
        Ok(removed != 0)
    }

    pub fn variable_snapshot(&self, name: &str) -> Result<Option<FfiSnapshotValue>, FfiBindingError> {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "PowerShell session has been released".to_owned()))?;
        let name_length = checked_utf8_length(name)?;
        let mut found = 0_u32;
        let mut kind = 0_u32;
        let mut required_length = 0_i32;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_get_variable_snapshot_fn)(
                handle,
                name.as_ptr(),
                name_length,
                &mut found,
                &mut kind,
                std::ptr::null_mut(),
                0,
                &mut required_length,
                &mut call_result,
            )
        };
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        if found == 0 {
            if required_length != 0 {
                return Err(FfiBindingError::from_status(
                    -6,
                    "managed session variable snapshot reported bytes for an absent variable".to_owned(),
                ));
            }
            return Ok(None);
        }
        if found != 1 || required_length < 0 || required_length as usize > 64 * 1024 {
            return Err(FfiBindingError::from_status(
                -6,
                "managed session variable snapshot metadata is invalid".to_owned(),
            ));
        }

        let mut payload = vec![0_u8; required_length as usize];
        call_result = new_call_result(&mut diagnostic);
        let status = unsafe {
            (self.bindings.session_get_variable_snapshot_fn)(
                handle,
                name.as_ptr(),
                name_length,
                &mut found,
                &mut kind,
                payload.as_mut_ptr(),
                required_length,
                &mut required_length,
                &mut call_result,
            )
        };
        check_status(status, &call_result, &diagnostic)?;
        if found != 1 || required_length as usize != payload.len() {
            return Err(FfiBindingError::from_status(
                -6,
                "managed session variable snapshot changed while it was copied".to_owned(),
            ));
        }
        Ok(Some(FfiSnapshotValue { kind, payload }))
    }
}

impl Drop for FfiPowerShellSession {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (self.bindings.session_release_fn)(handle, &mut call_result);
        }
    }
}

pub struct FfiInvocationResult {
    _runtime: Arc<HostedRuntime>,
    bindings: FfiBindings,
    handle: Option<PowerShellHandle>,
}

impl FfiInvocationResult {
    pub fn info(&self) -> Result<(u32, usize), FfiBindingError> {
        let mut flags = 0;
        let mut sequence_count = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_info_fn)(handle, &mut flags, &mut sequence_count, result)
        })?;
        let sequence_count = usize::try_from(sequence_count)
            .map_err(|_| FfiBindingError::from_status(-6, "managed sequence count is invalid".to_owned()))?;
        Ok((flags, sequence_count))
    }

    pub fn metadata(&self) -> Result<(u32, u64, bool), FfiBindingError> {
        let mut state = 0;
        let mut invocation_id = 0;
        let mut had_errors = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_metadata_fn)(
                handle,
                &mut state,
                &mut invocation_id,
                &mut had_errors,
                result,
            )
        })?;
        let invocation_id = u64::try_from(invocation_id)
            .map_err(|_| FfiBindingError::from_status(-6, "managed invocation ID is invalid".to_owned()))?;
        match had_errors {
            0 => Ok((state, invocation_id, false)),
            1 => Ok((state, invocation_id, true)),
            _ => Err(FfiBindingError::from_status(
                -6,
                "managed invocation error metadata is invalid".to_owned(),
            )),
        }
    }

    pub fn stream_info(&self, stream: i32) -> Result<(usize, u32), FfiBindingError> {
        let mut record_count = 0;
        let mut flags = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_stream_info_fn)(handle, stream, &mut record_count, &mut flags, result)
        })?;
        let record_count = usize::try_from(record_count)
            .map_err(|_| FfiBindingError::from_status(-6, "managed stream record count is invalid".to_owned()))?;
        Ok((record_count, flags))
    }

    pub fn stream_totals(&self, stream: i32) -> Result<(u64, u64), FfiBindingError> {
        let mut total_record_count = 0_i64;
        let mut dropped_record_count = 0_i64;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_stream_totals_fn)(
                handle,
                stream,
                &mut total_record_count,
                &mut dropped_record_count,
                result,
            )
        })?;
        let total_record_count = u64::try_from(total_record_count)
            .map_err(|_| FfiBindingError::from_status(-6, "managed stream total is invalid".to_owned()))?;
        let dropped_record_count = u64::try_from(dropped_record_count)
            .map_err(|_| FfiBindingError::from_status(-6, "managed stream dropped total is invalid".to_owned()))?;
        if dropped_record_count > total_record_count {
            return Err(FfiBindingError::from_status(
                -6,
                "managed stream dropped total exceeds its total".to_owned(),
            ));
        }
        Ok((total_record_count, dropped_record_count))
    }

    pub fn stream_record_info(&self, stream: i32, record_index: i32) -> Result<(i64, u32), FfiBindingError> {
        let mut sequence = 0;
        let mut flags = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_stream_record_info_fn)(
                handle,
                stream,
                record_index,
                &mut sequence,
                &mut flags,
                result,
            )
        })?;
        Ok((sequence, flags))
    }

    pub fn stream_record_projection_info(
        &self,
        stream: i32,
        record_index: i32,
    ) -> Result<FfiStreamRecordProjectionInfo, FfiBindingError> {
        let mut property_entry_count = 0;
        let mut dropped_property_entry_count = 0;
        let mut type_name_count = 0;
        let mut dropped_type_name_count = 0;
        let mut flags = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_stream_record_projection_info_fn)(
                handle,
                stream,
                record_index,
                &mut property_entry_count,
                &mut dropped_property_entry_count,
                &mut type_name_count,
                &mut dropped_type_name_count,
                &mut flags,
                result,
            )
        })?;
        Ok(FfiStreamRecordProjectionInfo {
            property_entry_count: u32::try_from(property_entry_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed property count is invalid".to_owned()))?,
            dropped_property_entry_count: u32::try_from(dropped_property_entry_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed dropped property count is invalid".to_owned())
            })?,
            type_name_count: u32::try_from(type_name_count)
                .map_err(|_| FfiBindingError::from_status(-6, "managed type-name count is invalid".to_owned()))?,
            dropped_type_name_count: u32::try_from(dropped_type_name_count).map_err(|_| {
                FfiBindingError::from_status(-6, "managed dropped type-name count is invalid".to_owned())
            })?,
            flags: u32::try_from(flags)
                .map_err(|_| FfiBindingError::from_status(-6, "managed projection flags are invalid".to_owned()))?,
        })
    }

    pub fn stream_record_value(
        &self,
        stream: i32,
        record_index: i32,
        value_slot: i32,
    ) -> Result<FfiSnapshotValue, FfiBindingError> {
        let mut kind = 0;
        let mut required_length = 0;
        let status = self.call_status(|handle, result| unsafe {
            (self.bindings.invocation_result_copy_stream_record_value_fn)(
                handle,
                stream,
                record_index,
                value_slot,
                &mut kind,
                std::ptr::null_mut(),
                0,
                &mut required_length,
                result,
            )
        })?;
        if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
            return Err(FfiBindingError::from_status(
                status,
                "managed invocation stream value is unavailable".to_owned(),
            ));
        }
        let required_length = usize::try_from(required_length).map_err(|_| {
            FfiBindingError::from_status(-1, "managed invocation stream value length is invalid".to_owned())
        })?;
        if required_length > 16 * 1024 {
            return Err(FfiBindingError::from_status(
                -6,
                "managed invocation stream value exceeds its bound".to_owned(),
            ));
        }
        let mut payload = vec![0_u8; required_length];
        let payload_length = i32::try_from(payload.len()).map_err(|_| {
            FfiBindingError::from_status(-1, "managed invocation stream value length is invalid".to_owned())
        })?;
        let mut copied_length = payload_length;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_copy_stream_record_value_fn)(
                handle,
                stream,
                record_index,
                value_slot,
                &mut kind,
                payload.as_mut_ptr(),
                payload_length,
                &mut copied_length,
                result,
            )
        })?;
        if usize::try_from(copied_length).ok() != Some(payload.len()) {
            return Err(FfiBindingError::from_status(
                -6,
                "managed invocation stream value length changed during copy".to_owned(),
            ));
        }
        Ok(FfiSnapshotValue { kind, payload })
    }

    pub fn stream_record_field(&self, stream: i32, record_index: i32, field: i32) -> Result<String, FfiBindingError> {
        let mut required_length = 0;
        let status = self.call_status(|handle, result| unsafe {
            (self.bindings.invocation_result_copy_stream_record_field_to_utf8_fn)(
                handle,
                stream,
                record_index,
                field,
                std::ptr::null_mut(),
                0,
                &mut required_length,
                result,
            )
        })?;
        if status != STATUS_SUCCESS && status != STATUS_BUFFER_TOO_SMALL {
            return Err(FfiBindingError::from_status(
                status,
                "managed invocation stream field is unavailable".to_owned(),
            ));
        }

        let mut value = vec![
            0_u8;
            usize::try_from(required_length).map_err(|_| {
                FfiBindingError::from_status(-1, "managed invocation stream field length is invalid".to_owned())
            })?
        ];
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_copy_stream_record_field_to_utf8_fn)(
                handle,
                stream,
                record_index,
                field,
                value.as_mut_ptr(),
                required_length,
                &mut required_length,
                result,
            )
        })?;
        String::from_utf8(value)
            .map_err(|_| FfiBindingError::from_status(-6, "managed invocation stream field is not UTF-8".to_owned()))
    }

    pub fn sequence_record(&self, sequence_index: i32) -> Result<(i32, i32, i64), FfiBindingError> {
        let mut stream = 0;
        let mut record_index = 0;
        let mut sequence = 0;
        self.call(|handle, result| unsafe {
            (self.bindings.invocation_result_get_sequence_record_fn)(
                handle,
                sequence_index,
                &mut stream,
                &mut record_index,
                &mut sequence,
                result,
            )
        })?;
        Ok((stream, record_index, sequence))
    }

    fn call<F>(&self, operation: F) -> Result<(), FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let status = self.call_status(operation)?;
        if status == STATUS_SUCCESS {
            Ok(())
        } else {
            Err(FfiBindingError::from_status(
                status,
                "managed invocation result binding failed".to_owned(),
            ))
        }
    }

    fn call_status<F>(&self, operation: F) -> Result<i32, FfiBindingError>
    where
        F: FnOnce(PowerShellHandle, *mut FfiCallResult) -> i32,
    {
        let handle = self
            .handle
            .ok_or_else(|| FfiBindingError::from_status(-4, "Invocation result handle has been released".to_owned()))?;
        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        let status = operation(handle, &mut call_result);
        check_status_allow_buffer_too_small(status, &call_result, &diagnostic)?;
        Ok(status)
    }
}

impl Drop for FfiInvocationResult {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        let mut diagnostic = [0_u8; FFI_CALL_DIAGNOSTIC_CAPACITY];
        let mut call_result = new_call_result(&mut diagnostic);
        unsafe {
            (self.bindings.invocation_result_release_fn)(handle, &mut call_result);
        }
    }
}

fn checked_utf8_length(value: &str) -> Result<i32, FfiBindingError> {
    i32::try_from(value.len())
        .map_err(|_| FfiBindingError::from_status(-1, "UTF-8 input exceeds managed binding limits".to_owned()))
}

fn checked_value_length(value: &[u8]) -> Result<i32, FfiBindingError> {
    i32::try_from(value.len())
        .map_err(|_| FfiBindingError::from_status(-1, "Tagged value payload exceeds managed binding limits".to_owned()))
}

fn new_call_result(diagnostic: &mut [u8; FFI_CALL_DIAGNOSTIC_CAPACITY]) -> FfiCallResult {
    FfiCallResult {
        size: mem::size_of::<FfiCallResult>() as u32,
        status: STATUS_SUCCESS,
        flags: 0,
        diagnostic: diagnostic.as_mut_ptr(),
        diagnostic_capacity: diagnostic.len() as i32,
        diagnostic_required_length: 0,
        diagnostic_written_length: 0,
    }
}

fn check_status(status: i32, result: &FfiCallResult, diagnostic: &[u8]) -> Result<(), FfiBindingError> {
    if status == STATUS_SUCCESS {
        return Ok(());
    }

    Err(FfiBindingError::from_status(
        status,
        call_diagnostic(result, diagnostic),
    ))
}

fn check_status_allow_buffer_too_small(
    status: i32,
    result: &FfiCallResult,
    diagnostic: &[u8],
) -> Result<(), FfiBindingError> {
    if status == STATUS_SUCCESS || status == STATUS_BUFFER_TOO_SMALL {
        return Ok(());
    }

    Err(FfiBindingError::from_status(
        status,
        call_diagnostic(result, diagnostic),
    ))
}

fn call_diagnostic(result: &FfiCallResult, diagnostic: &[u8]) -> String {
    let written = usize::try_from(result.diagnostic_written_length.max(0)).unwrap_or(0);
    let written = written.min(diagnostic.len());
    let message = String::from_utf8_lossy(&diagnostic[..written]).into_owned();
    if message.is_empty() {
        format!("managed binding returned status {}", result.status)
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SMALL_FFI_API_V1: FfiApiV1Header = FfiApiV1Header {
        size: mem::size_of::<FfiApiV1Header>(),
        abi_version: FFI_BINDINGS_ABI_VERSION,
        feature_flags: FFI_REQUIRED_FEATURES,
    };

    static MISSING_BRIDGE_FEATURE_FFI_API_V1: FfiApiV1Header = FfiApiV1Header {
        size: mem::size_of::<FfiApiV1>(),
        abi_version: FFI_BINDINGS_ABI_VERSION,
        feature_flags: FFI_REQUIRED_FEATURES & !FFI_FEATURE_GENERATED_BRIDGE_ATTACHMENT,
    };

    static MISSING_OBSERVED_PRESENTATION_FEATURE_FFI_API_V1: FfiApiV1Header = FfiApiV1Header {
        size: mem::size_of::<FfiApiV1>(),
        abi_version: FFI_BINDINGS_ABI_VERSION,
        feature_flags: FFI_REQUIRED_FEATURES & !FFI_FEATURE_OBSERVED_PRESENTATION,
    };

    unsafe extern "system" fn get_small_ffi_api_v1() -> *const FfiApiV1 {
        &SMALL_FFI_API_V1 as *const FfiApiV1Header as *const FfiApiV1
    }

    unsafe extern "system" fn get_missing_bridge_feature_ffi_api_v1() -> *const FfiApiV1 {
        &MISSING_BRIDGE_FEATURE_FFI_API_V1 as *const FfiApiV1Header as *const FfiApiV1
    }

    unsafe extern "system" fn get_missing_observed_presentation_feature_ffi_api_v1() -> *const FfiApiV1 {
        &MISSING_OBSERVED_PRESENTATION_FEATURE_FFI_API_V1 as *const FfiApiV1Header as *const FfiApiV1
    }

    #[test]
    fn rejects_smaller_ffi_api_before_copying_extended_fields() {
        assert!(unsafe { load_ffi_api_v1(get_small_ffi_api_v1) }.is_err());
    }

    #[test]
    fn rejects_missing_generated_bridge_feature_before_copying_extended_fields() {
        assert!(unsafe { load_ffi_api_v1(get_missing_bridge_feature_ffi_api_v1) }.is_err());
    }

    #[test]
    fn rejects_missing_observed_presentation_feature_before_copying_extended_fields() {
        assert!(unsafe { load_ffi_api_v1(get_missing_observed_presentation_feature_ffi_api_v1) }.is_err());
    }
}
