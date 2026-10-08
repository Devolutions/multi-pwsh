#![allow(clippy::missing_safety_doc)]

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryFrom;
use std::fs;
use std::mem;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::MutexGuard;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(test)]
use pwsh_host::FfiLiveStreamRecord;
use pwsh_host::{
    find_pwsh_dir, FfiBindingError, FfiBridgeContext, FfiCredentialResult, FfiInvocationResult, FfiLiveInvocation,
    FfiLiveObjectContractDescriptor, FfiLiveStreamBatch, FfiObservedDiagnosticPage, FfiObservedInvocation,
    FfiPowerShell, FfiPowerShellSession, FfiSessionEvent, FfiSessionSnapshot, FfiSnapshotValue,
    FfiTypedResultInvocation, FfiTypedResultPage, FfiTypedResultRecord, HostedRuntime, LiveObjectContractPack,
};

const ABI_VERSION: u32 = 2;
const MINIMUM_COMPATIBLE_ABI_VERSION: u32 = 2;
const FEATURE_STRUCTURED_INVOCATION_ERRORS: u64 = 1;
const FEATURE_PER_CALL_DIAGNOSTICS: u64 = 1 << 1;
const FEATURE_UTF8_SPANS: u64 = 1 << 2;
const FEATURE_IMMUTABLE_RESULTS: u64 = 1 << 3;
const FEATURE_TAGGED_VALUES: u64 = 1 << 4;
const FEATURE_COMMAND_OPTIONS: u64 = 1 << 5;
const FEATURE_BOUNDED_INPUT: u64 = 1 << 6;
const FEATURE_INVOCATION_METADATA: u64 = 1 << 7;
const FEATURE_ASYNC_OPERATIONS: u64 = 1 << 8;
const FEATURE_SESSIONS: u64 = 1 << 10;
const FEATURE_SESSION_POLLING: u64 = 1 << 11;
const FEATURE_SESSION_POOL_REJECTION: u64 = 1 << 12;
const FEATURE_SNAPSHOT_PROJECTIONS: u64 = 1 << 13;
const FEATURE_SESSION_CONFIGURATION: u64 = 1 << 14;
const FEATURE_SESSION_VARIABLES: u64 = 1 << 15;
const FEATURE_CAPABILITY_RPC: u64 = 1 << 16;
const FEATURE_LIVE_OBJECT_PROBE: u64 = 1 << 17;
const FEATURE_LIVE_SESSION_OBJECT_PROBE: u64 = 1 << 18;
const FEATURE_LIVE_OBJECT_CONTRACTS: u64 = 1 << 19;
const FEATURE_LIVE_STREAM_POLLING: u64 = 1 << 20;
const FEATURE_TYPED_RESULT_PAGING: u64 = 1 << 21;
const FEATURE_OBSERVED_INVOCATION: u64 = 1 << 22;
const FEATURE_SESSION_PREFLIGHT: u64 = 1 << 23;
const FEATURE_RUNTIME_DIAGNOSTICS: u64 = 1 << 24;
const FEATURE_DUPLEX_BROKER_CHANNEL: u64 = 1 << 25;
const FEATURE_GENERATED_BRIDGE_ATTACHMENT: u64 = 1 << 26;
const FEATURE_BROKER_TERMINAL_OBSERVATION: u64 = 1 << 27;
const FEATURE_RELIABLE_BRIDGE_EVENTS: u64 = 1 << 28;
const FEATURE_OBSERVED_PRESENTATION: u64 = 1 << 29;
const FEATURE_SECRET_ADAPTERS: u64 = 1 << 30;
const FEATURE_CREDENTIAL_RESULT: u64 = 1 << 31;
const FEATURE_REMOTE_PROVIDER_DIAGNOSTICS: u64 = 1 << 32;
const SECRET_VALUE_KIND_UTF16: u32 = 15;
const SECRET_VALUE_KIND_CREDENTIAL: u32 = 16;
const MAX_SECRET_UTF16_CODE_UNITS: usize = 4_096;
const MAX_SECRET_USER_NAME_UTF8_BYTES: usize = 1_024;
const MAX_CREDENTIAL_TEXT_UTF8_BYTES: usize = 4 * 1024;
const MAX_CREDENTIAL_MESSAGE_COUNT: usize = 16;
const MAX_CREDENTIAL_MESSAGE_UTF8_BYTES: usize = 4 * 1024;
const MAX_CREDENTIAL_MESSAGE_BUFFER_BYTES: usize = std::mem::size_of::<u32>()
    + MAX_CREDENTIAL_MESSAGE_COUNT * (std::mem::size_of::<u32>() + MAX_CREDENTIAL_MESSAGE_UTF8_BYTES);
const CALL_RESULT_DIAGNOSTIC_TRUNCATED: u32 = 1;
#[cfg(test)]
const RESULT_RECORD_SCALAR_VALUE_PRESENT: u32 = 1 << 1;
#[cfg(test)]
const RESULT_RECORD_PROPERTY_BAG_PRESENT: u32 = 1 << 2;
#[cfg(test)]
const RESULT_RECORD_ERROR_TARGET_VALUE_PRESENT: u32 = 1 << 5;
const MAX_VALUE_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_VALUE_CONTAINER_ENTRIES: u32 = 64;
const MAX_VALUE_DEPTH: u8 = 8;
const MAX_OPERATION_DIAGNOSTIC_BYTES: usize = 4096;
const MAX_OPERATION_STREAM_RECORDS: usize = 32;
const MAX_OPERATION_STREAM_RECORD_BYTES: usize = 4096;
const MAX_TYPED_RESULT_RECORDS: usize = 64;
const MAX_TYPED_RESULT_PAYLOAD_BYTES: usize = 64 * 1024;
const OPERATION_STREAM_RECORD_TEXT_TRUNCATED: u32 = 1;
const MAX_SESSION_CONFIGURATION_ENTRIES: usize = 32;
const MAX_SESSION_PATH_BYTES: usize = 16 * 1024;

const BROKER_ABI_V1: u32 = 1;
const BROKER_MAX_INFLIGHT_LIMIT: u32 = 32;
const BROKER_MAX_BODY_BYTES_LIMIT: u32 = 64 * 1024;
const BROKER_MAX_DEADLINE_MS: u32 = 30_000;
const BROKER_MAX_ERROR_MESSAGE_BYTES: usize = 512;
const BROKER_MAX_OBSERVERS_PER_FRAME: u32 = 4;
const BROKER_FRAME_FLAG_ONE_WAY: u32 = 1;
const BROKER_FRAME_FLAG_MUTATING: u32 = 1 << 1;
const BROKER_FRAME_FLAG_MASK: u32 = BROKER_FRAME_FLAG_ONE_WAY | BROKER_FRAME_FLAG_MUTATING;
const BROKER_FRAME_STATE_QUEUED: u32 = 0;
const BROKER_FRAME_STATE_DISPATCHED: u32 = 1;
const BROKER_FRAME_STATE_COMPLETED: u32 = 2;
const BROKER_FRAME_STATE_FAILED: u32 = 3;
const BROKER_FRAME_STATE_CANCELLED: u32 = 4;
const BROKER_FRAME_STATE_TIMED_OUT: u32 = 5;
const BROKER_FRAME_STATE_ABORTED: u32 = 6;
const BRIDGE_BROKER_REQUEST_KIND: u32 = 0x4252_0001;
const BRIDGE_ROUTE_HEADER_SIZE: usize = 8;
const BRIDGE_REQUEST_HEADER_SIZE: usize = 32;
const BRIDGE_PROTOCOL_VERSION: u8 = 2;
const BRIDGE_CLOSE_FRAME_KIND: u8 = 4;
const SESSION_OPTIONS_PREFIX_SIZE: u32 = mem::size_of::<SessionOptionsPrefix>() as u32;
const EMPTY_VALUE_CONTAINER: [u8; 4] = [0; 4];
const VALUE_KIND_STRING: u32 = 1;
const VALUE_KIND_UNSIGNED_INTEGER: u32 = 5;
const VALUE_KIND_ARRAY: u32 = 13;
const VALUE_KIND_PROPERTY_BAG: u32 = 14;
const VALUE_KIND_TIME_SPAN: u32 = 17;
const CAPABILITY_REGISTRATION_VERSION: u32 = 1;
const MAX_CAPABILITIES: usize = 16;
const MAX_CAPABILITY_NAME_BYTES: usize = 64;
const MAX_BRIDGE_VARIABLE_NAME_BYTES: usize = 64;
const MAX_CAPABILITY_DEADLINE_MILLISECONDS: u32 = 30_000;
const MAX_CAPABILITY_PERMISSIONS: u32 = 0x0f;
const MAX_LIVE_OBJECT_CONTRACT_PACKS: usize = 16;
const MAX_LIVE_OBJECT_CONTRACT_PACK_TYPE_NAME_BYTES: usize = 512;

#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Success = 0,
    BufferTooSmall = 1,
    InvalidArgument = -1,
    NotInitialized = -2,
    IncompatiblePayload = -3,
    InvalidHandle = -4,
    HostFailure = -5,
    ManagedFailure = -6,
    Panic = -7,
    InputNotCompleted = -8,
    Backpressure = -9,
    UnsupportedValue = -10,
    OperationCancelled = -11,
    OperationNotTerminal = -12,
    UnsupportedCapability = -17,
    BrokerBusy = -18,
    BrokerNoConsumer = -19,
    BrokerClosed = -20,
    BrokerInvalidTerminalState = -21,
    BrokerDispatchViolation = -22,
    BrokerTimeout = -23,
}

impl Status {
    fn value(self) -> i32 {
        self as i32
    }
}

#[repr(C)]
pub struct AbiInfo {
    size: u32,
    abi_version: u32,
    feature_flags: u64,
    minimum_compatible_abi_version: u32,
    _reserved: u32,
}

#[repr(C)]
pub struct RuntimeDiagnosticsInfo {
    size: u32,
    bindings_abi_version: u32,
    payload_table_size: usize,
    payload_table_slot_count: u32,
    payload_table_shape: u32,
    power_shell_file_version_available: u32,
    contract_pack_count: u32,
    _reserved: u32,
    remote_provider_status: u32,
    remote_provider_unavailable_reason: u32,
    _reserved2: u32,
    remote_provider_capabilities: u64,
}

#[repr(C)]
pub struct OperationStreamBatchInfo {
    size: u32,
    operation_state: u32,
    terminal_status: i32,
    flags: u32,
    next_sequence: u64,
    total_record_count: u64,
    dropped_record_count: u64,
    source_dropped_record_count: u64,
    lost_record_count: u64,
    record_count: u32,
    _reserved: u32,
}

#[repr(C)]
pub struct TypedResultPageInfo {
    size: u32,
    flags: u32,
    terminal_status: i32,
    _reserved: u32,
    acknowledged_sequence: u64,
    next_sequence: u64,
    total_record_count: u64,
    dropped_record_count: u64,
    record_count: u32,
    _reserved2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Utf8Span {
    data: *const u8,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DataValue {
    size: u32,
    kind: u32,
    flags: u32,
    _reserved: u32,
    data: *const u8,
    data_len: usize,
}

#[repr(C)]
pub struct CallResult {
    size: u32,
    status: i32,
    flags: u32,
    _reserved: u32,
    diagnostic: *mut u8,
    diagnostic_capacity: usize,
    diagnostic_required: usize,
    diagnostic_written: usize,
}

type CapabilityDispatchCallback = unsafe extern "C" fn(
    u64,
    u64,
    Utf8Span,
    *const DataValue,
    u32,
    u32,
    *mut u32,
    *mut u8,
    usize,
    *mut usize,
    *mut CallResult,
) -> i32;
type CapabilityCancelCallback = unsafe extern "C" fn(u64, u64);

#[repr(C)]
pub struct CapabilityRegistration {
    size: u32,
    flags: u32,
    definitions: *const DataValue,
    dispatch: Option<CapabilityDispatchCallback>,
    cancel: Option<CapabilityCancelCallback>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SessionOptions {
    size: u32,
    runspace_mode: u32,
    initial_configuration: u32,
    history_mode: u32,
    error_preference: u32,
    warning_preference: u32,
    verbose_preference: u32,
    debug_preference: u32,
    information_preference: u32,
    flags: u32,
    _reserved: u32,
    allowed_module_path: Utf8Span,
    execution_policy: u32,
    configuration_flags: u32,
    initial_variables: DataValue,
    module_imports: DataValue,
    allowed_module_paths: DataValue,
    working_directory: Utf8Span,
    environment: DataValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SessionOptionsPrefix {
    size: u32,
    runspace_mode: u32,
    initial_configuration: u32,
    history_mode: u32,
    error_preference: u32,
    warning_preference: u32,
    verbose_preference: u32,
    debug_preference: u32,
    information_preference: u32,
    flags: u32,
    _reserved: u32,
    allowed_module_path: Utf8Span,
}

#[repr(C)]
pub struct SessionSnapshot {
    size: u32,
    state: u32,
    runspace_state: u32,
    flags: u32,
    active_pipeline_count: u32,
    event_count: u32,
    invocation_count: u64,
    history_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SessionPoolOptions {
    size: u32,
    minimum_sessions: u32,
    maximum_sessions: u32,
    flags: u32,
    _reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LiveObjectContractPackInput {
    size: u32,
    flags: u32,
    payload_adapter_assembly_path: Utf8Span,
    payload_adapter_type_name: Utf8Span,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrokerChannelOptions {
    size: u32,
    abi_version: u32,
    max_inflight: u32,
    max_body_bytes: u32,
    default_deadline_ms: u32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BrokerFrameInfo {
    size: u32,
    abi_version: u32,
    correlation_id: u64,
    ordering_key: u64,
    deadline_epoch_ms: u64,
    remaining_ms: u32,
    kind: u32,
    flags: u32,
    body_length: u32,
    state: u32,
    dropped_before: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BrokerTerminalInfo {
    size: u32,
    abi_version: u32,
    state: u32,
    terminal_status: i32,
    terminal_epoch_ms: u64,
}

#[derive(Clone, Copy)]
struct BrokerTerminalState {
    state: u32,
    terminal_status: i32,
    terminal_epoch_ms: u64,
}

struct BrokerTerminalObservation {
    state: Mutex<BrokerTerminalState>,
    signal: Condvar,
    leases: AtomicU32,
}

impl BrokerTerminalObservation {
    fn new() -> Self {
        Self {
            state: Mutex::new(BrokerTerminalState {
                state: BROKER_FRAME_STATE_QUEUED,
                terminal_status: Status::OperationNotTerminal.value(),
                terminal_epoch_ms: 0,
            }),
            signal: Condvar::new(),
            leases: AtomicU32::new(0),
        }
    }

    fn complete(&self, state: u32, terminal_epoch_ms: u64) {
        let mut observed = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if observed.state >= BROKER_FRAME_STATE_COMPLETED {
            return;
        }

        observed.state = state;
        observed.terminal_status = broker_terminal_status(state).value();
        observed.terminal_epoch_ms = terminal_epoch_ms;
        drop(observed);
        self.signal.notify_all();
    }

    fn mark_dispatched(&self) {
        let mut observed = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if observed.state < BROKER_FRAME_STATE_COMPLETED {
            observed.state = BROKER_FRAME_STATE_DISPATCHED;
        }
    }

    fn try_acquire(&self) -> bool {
        let mut current = self.leases.load(Ordering::Acquire);
        loop {
            if current >= BROKER_MAX_OBSERVERS_PER_FRAME {
                return false;
            }

            match self
                .leases
                .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }

    fn release(&self) {
        let _ = self.leases.fetch_sub(1, Ordering::AcqRel);
    }
}

struct BrokerFrame {
    correlation_id: u64,
    generation: u64,
    ordering_key: u64,
    kind: u32,
    flags: u32,
    deadline_epoch_ms: u64,
    body: Vec<u8>,
    state: u32,
    dropped_before: u32,
    reply: Vec<u8>,
    error_message: String,
    owner_token: u64,
    terminal_observation: Arc<BrokerTerminalObservation>,
}

impl BrokerFrame {
    fn is_terminal(&self) -> bool {
        self.state >= BROKER_FRAME_STATE_COMPLETED
    }

    fn is_one_way(&self) -> bool {
        self.flags & BROKER_FRAME_FLAG_ONE_WAY != 0
    }
}

#[derive(Default)]
struct BrokerChannelInner {
    closed: bool,
    consumer_attached: bool,
    queue: VecDeque<u64>,
    frames: HashMap<u64, BrokerFrame>,
    next_correlation: u64,
    pending_drops: u32,
}

struct BrokerChannel {
    epoch: Instant,
    max_inflight: u32,
    max_body_bytes: u32,
    default_deadline_ms: u32,
    observer_leases: AtomicU32,
    inner: Mutex<BrokerChannelInner>,
    payload_signal: Condvar,
    consumer_signal: Condvar,
}

impl BrokerChannel {
    fn now_epoch_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn lock(&self) -> MutexGuard<'_, BrokerChannelInner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A payload-visible broker attachment. The generation is validated on every
/// trampoline call so script that captures `$DpsBroker` beyond its invocation
/// cannot reach a channel afterwards.
struct BrokerAttachment {
    channel_handle: u64,
    channel: Arc<BrokerChannel>,
    generation: u64,
    active: AtomicBool,
    cancelled: AtomicBool,
    bridge: Option<BridgeAttachment>,
}

#[derive(Clone)]
struct BridgeAttachment {
    binding_id: u64,
    contract_id_low: u64,
    contract_id_high: u64,
    contract_major_version: u16,
    contract_minor_version: u16,
    maximum_request_bytes: u32,
    maximum_reply_bytes: u32,
    variable_name: String,
}

struct BrokerConfiguration {
    channel_handle: u64,
    channel: Arc<BrokerChannel>,
    bridge: Option<BridgeAttachment>,
}

struct State {
    runtime: Option<Arc<HostedRuntime>>,
    activation_source_root: Option<PathBuf>,
    live_object_contract_packs: Vec<LiveObjectContractPack>,
    sessions: HashMap<u64, Arc<Session>>,
    runspace_sessions: HashMap<u64, Arc<RunspaceSession>>,
    results: HashMap<u64, Arc<InvocationResult>>,
    operations: HashMap<u64, Arc<Operation>>,
    operation_stream_batches: HashMap<u64, Arc<OperationStreamBatch>>,
    typed_result_operations: HashMap<u64, Arc<TypedResultOperation>>,
    typed_result_pages: HashMap<u64, Arc<FfiTypedResultPage>>,
    observed_operations: HashMap<u64, Arc<ObservedInvocationOperation>>,
    observed_diagnostic_pages: HashMap<u64, Arc<FfiObservedDiagnosticPage>>,
    capabilities: HashMap<u64, Arc<CapabilityRegistrationState>>,
    broker_channels: HashMap<u64, Arc<BrokerChannel>>,
    next_handle: u64,
    next_result_handle: u64,
    next_operation_handle: u64,
    next_operation_stream_batch_handle: u64,
    next_typed_result_operation_handle: u64,
    next_typed_result_page_handle: u64,
    next_observed_operation_handle: u64,
    next_observed_diagnostic_page_handle: u64,
    next_runspace_session_handle: u64,
    next_capability_handle: u64,
    next_broker_channel_handle: u64,
}

struct Session {
    power_shell: FfiPowerShell,
    operation_active: Mutex<bool>,
    runspace_session: Option<Arc<RunspaceSession>>,
    capability_registration: Mutex<Option<Arc<CapabilityRegistrationState>>>,
    active_capability: Mutex<Option<CapabilityInvocation>>,
    broker_channel: Mutex<Option<BrokerConfiguration>>,
    active_broker: Mutex<Option<Arc<BrokerAttachment>>>,
}

struct RunspaceSession {
    session: FfiPowerShellSession,
    operation_active: Mutex<bool>,
}

struct InvocationResult {
    result: FfiInvocationResult,
}

struct CapabilityDefinition {
    argument_kinds: Vec<Vec<u32>>,
    response_kinds: Vec<u32>,
    maximum_input_bytes: usize,
    maximum_output_bytes: usize,
    deadline_milliseconds: u32,
}

struct CapabilityRegistrationState {
    handle: u64,
    definitions: HashMap<String, CapabilityDefinition>,
    dispatch: CapabilityDispatchCallback,
    cancel: CapabilityCancelCallback,
    active: AtomicBool,
    invocations: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

#[derive(Clone)]
struct CapabilityInvocation {
    registration: Arc<CapabilityRegistrationState>,
    invocation_id: u64,
    cancelled: Arc<AtomicBool>,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationState {
    Pending = 1,
    Running = 2,
    Completed = 3,
    Cancelled = 4,
    Failed = 5,
}

impl OperationState {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

struct OperationCompletion {
    state: OperationState,
    terminal_status: Status,
    diagnostic: String,
    result: Option<Arc<InvocationResult>>,
}

struct Operation {
    builder_handle: u64,
    session: Arc<Session>,
    runspace_session: Option<Arc<RunspaceSession>>,
    supports_live_stream_polling: bool,
    cancellation_requested: AtomicBool,
    capability: Option<CapabilityInvocation>,
    completion: (Mutex<OperationCompletion>, Condvar),
    stream: Mutex<OperationStreamState>,
}

#[derive(Clone)]
struct OperationStreamRecord {
    stream: u32,
    sequence: u64,
    flags: u32,
    text: String,
}

struct OperationStreamState {
    records: VecDeque<OperationStreamRecord>,
    next_sequence: u64,
    total_record_count: u64,
    dropped_record_count: u64,
    source_dropped_record_count: u64,
}

struct OperationStreamBatch {
    state: OperationState,
    terminal_status: Status,
    next_sequence: u64,
    total_record_count: u64,
    dropped_record_count: u64,
    source_dropped_record_count: u64,
    lost_record_count: u64,
    records: Vec<OperationStreamRecord>,
}

struct TypedResultOperation {
    session: Arc<Session>,
    capability: Option<CapabilityInvocation>,
    invocation: Mutex<Option<FfiTypedResultInvocation>>,
    pipeline_completed: AtomicBool,
    cancellation_requested: AtomicBool,
    finalized: AtomicBool,
    worker_error: Mutex<Option<(Status, String)>>,
}

struct ObservedInvocationOperation {
    session: Arc<Session>,
    capability: Option<CapabilityInvocation>,
    invocation: Mutex<Option<Arc<FfiObservedInvocation>>>,
    pipeline_completed: AtomicBool,
    cancellation_requested: AtomicBool,
    finalized: AtomicBool,
    worker_error: Mutex<Option<(Status, String)>>,
}

impl OperationStreamState {
    fn capture_batch(&mut self, batch: FfiLiveStreamBatch) -> Result<(), (Status, String)> {
        if batch.total_record_count < self.total_record_count {
            return Err((
                Status::ManagedFailure,
                "managed live stream total record count is not monotonic".to_owned(),
            ));
        }
        self.source_dropped_record_count = self.source_dropped_record_count.saturating_add(batch.lost_record_count);
        self.total_record_count = batch.total_record_count;
        for record in batch.records {
            if record.stream >= 7 || record.sequence == 0 || record.sequence > batch.total_record_count {
                return Err((
                    Status::ManagedFailure,
                    "managed live stream record is invalid".to_owned(),
                ));
            }
            let sequence = self.next_sequence;
            if sequence == 0 {
                return Err((
                    Status::ManagedFailure,
                    "native live stream sequence exhausted".to_owned(),
                ));
            }
            self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
                (
                    Status::ManagedFailure,
                    "native live stream sequence exhausted".to_owned(),
                )
            })?;
            if self.records.len() == MAX_OPERATION_STREAM_RECORDS {
                self.records.pop_front();
                self.dropped_record_count = self.dropped_record_count.saturating_add(1);
            }
            let mut flags = record.flags;
            let text = bound_operation_stream_text(record.text, &mut flags);
            self.records.push_back(OperationStreamRecord {
                stream: record.stream,
                sequence,
                flags,
                text,
            });
        }

        Ok(())
    }

    fn read_batch(
        &self,
        state: OperationState,
        terminal_status: Status,
        after_sequence: u64,
        maximum_records: u32,
    ) -> Result<OperationStreamBatch, (Status, String)> {
        if maximum_records == 0 || maximum_records as usize > MAX_OPERATION_STREAM_RECORDS {
            return Err((
                Status::InvalidArgument,
                "live stream batch maximum records must be between 1 and 32".to_owned(),
            ));
        }
        let last_sequence = self.next_sequence.saturating_sub(1);
        if after_sequence > last_sequence {
            return Err((
                Status::InvalidArgument,
                "live stream batch cursor is beyond the latest sequence".to_owned(),
            ));
        }
        let first_sequence = self.records.front().map_or(0, |record| record.sequence);
        let lost_record_count = if first_sequence != 0 && after_sequence < first_sequence {
            first_sequence.saturating_sub(after_sequence).saturating_sub(1)
        } else {
            0
        };
        let records: Vec<_> = self
            .records
            .iter()
            .filter(|record| record.sequence > after_sequence)
            .take(maximum_records as usize)
            .cloned()
            .collect();
        let next_sequence = records.last().map_or(after_sequence, |record| record.sequence);
        Ok(OperationStreamBatch {
            state,
            terminal_status,
            next_sequence,
            total_record_count: self.total_record_count,
            dropped_record_count: self.dropped_record_count,
            source_dropped_record_count: self.source_dropped_record_count,
            lost_record_count,
            records,
        })
    }
}

fn bound_operation_stream_text(mut text: String, flags: &mut u32) -> String {
    if text.len() <= MAX_OPERATION_STREAM_RECORD_BYTES {
        return text;
    }

    let mut end = MAX_OPERATION_STREAM_RECORD_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    *flags |= OPERATION_STREAM_RECORD_TEXT_TRUNCATED;
    text
}

// Normal operations are serialized by SESSION_OPERATION_LOCK. Stop is the sole
// concurrent call and maps to PowerShell's supported cross-thread cancellation API.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}
unsafe impl Send for RunspaceSession {}
unsafe impl Sync for RunspaceSession {}
unsafe impl Send for InvocationResult {}
unsafe impl Sync for InvocationResult {}
unsafe impl Send for Operation {}
unsafe impl Sync for Operation {}
unsafe impl Send for TypedResultOperation {}
unsafe impl Sync for TypedResultOperation {}
unsafe impl Send for ObservedInvocationOperation {}
unsafe impl Sync for ObservedInvocationOperation {}

impl Session {
    fn clear_operation_active(&self) {
        *self
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
        if let Some(runspace_session) = &self.runspace_session {
            *runspace_session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
        }
        // Invalidate the broker generation so a `$DpsBroker` object captured by
        // script beyond its invocation can no longer reach the channel.
        self.deactivate_broker();
    }

    fn deactivate_broker(&self) {
        let mut active = self
            .active_broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(attachment) = active.take() {
            broker_deactivate_attachment(&attachment);
        }
    }

    fn cancel_broker(&self) {
        let active = self
            .active_broker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(attachment) = active.as_ref() {
            broker_cancel_attachment(attachment);
        }
    }
}

impl Operation {
    fn new(builder_handle: u64, session: Arc<Session>, capability: Option<CapabilityInvocation>) -> Self {
        Self {
            builder_handle,
            runspace_session: session.runspace_session.clone(),
            supports_live_stream_polling: session.power_shell.supports_live_stream_polling(),
            session,
            cancellation_requested: AtomicBool::new(false),
            capability,
            completion: (
                Mutex::new(OperationCompletion {
                    state: OperationState::Pending,
                    terminal_status: Status::Success,
                    diagnostic: String::new(),
                    result: None,
                }),
                Condvar::new(),
            ),
            stream: Mutex::new(OperationStreamState {
                records: VecDeque::with_capacity(MAX_OPERATION_STREAM_RECORDS),
                next_sequence: 1,
                total_record_count: 0,
                dropped_record_count: 0,
                source_dropped_record_count: 0,
            }),
        }
    }

    fn begin(&self) -> bool {
        let mut completion = self
            .completion
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if completion.state != OperationState::Pending {
            return false;
        }
        if self.cancellation_requested.load(Ordering::Acquire) {
            completion.state = OperationState::Cancelled;
            completion.terminal_status = Status::OperationCancelled;
            completion.diagnostic = "PowerShell async operation was cancelled before invocation started.".to_owned();
            drop(completion);
            self.completion.1.notify_all();
            self.clear_session_operation();
            self.finish_capability();
            return false;
        }

        completion.state = OperationState::Running;
        true
    }

    fn complete(
        &self,
        state: OperationState,
        terminal_status: Status,
        diagnostic: String,
        result: Option<Arc<InvocationResult>>,
    ) {
        let mut completion = self
            .completion
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if completion.state.is_terminal() {
            return;
        }

        if self.cancellation_requested() {
            completion.state = OperationState::Cancelled;
            completion.terminal_status = Status::OperationCancelled;
            completion.diagnostic = "PowerShell async operation was cancelled; no result is available.".to_owned();
            completion.result = None;
        } else {
            completion.state = state;
            completion.terminal_status = terminal_status;
            completion.diagnostic = bounded_operation_diagnostic(diagnostic);
            completion.result = result;
        }
        drop(completion);
        self.completion.1.notify_all();
        self.clear_session_operation();
    }

    fn request_stop(&self) {
        let (should_stop, finish_cancelled_capability) = {
            let mut completion = self
                .completion
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match completion.state {
                OperationState::Pending => {
                    self.cancellation_requested.store(true, Ordering::Release);
                    completion.state = OperationState::Cancelled;
                    completion.terminal_status = Status::OperationCancelled;
                    completion.diagnostic =
                        "PowerShell async operation was cancelled before invocation started.".to_owned();
                    drop(completion);
                    self.completion.1.notify_all();
                    self.clear_session_operation();
                    (false, true)
                }
                OperationState::Running => (!self.cancellation_requested.swap(true, Ordering::AcqRel), false),
                OperationState::Completed | OperationState::Cancelled | OperationState::Failed => (false, false),
            }
        };

        if finish_cancelled_capability {
            self.session.cancel_broker();
            cancel_and_finish_capability(self.capability.as_ref());
        } else if should_stop {
            // A cancellation request wins even if the managed Stop call races a
            // natural completion; the worker discards any captured result.
            self.session.cancel_broker();
            self.cancel_capability();
            let _ = self.session.power_shell.stop();
        }
    }

    fn snapshot(&self) -> (OperationState, Status, String, Option<Arc<InvocationResult>>) {
        let completion = self
            .completion
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            completion.state,
            completion.terminal_status,
            completion.diagnostic.clone(),
            completion.result.clone(),
        )
    }

    fn wait(&self, timeout: Option<Duration>) -> (OperationState, Status, String, Option<Arc<InvocationResult>>) {
        let mut completion = self
            .completion
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !completion.state.is_terminal() {
            if let Some(timeout) = timeout {
                let deadline = Instant::now()
                    .checked_add(timeout)
                    .unwrap_or_else(|| Instant::now() + timeout);
                while !completion.state.is_terminal() {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    let (next, wait_result) = self
                        .completion
                        .1
                        .wait_timeout(completion, remaining)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    completion = next;
                    if wait_result.timed_out() {
                        break;
                    }
                }
            } else {
                while !completion.state.is_terminal() {
                    completion = self
                        .completion
                        .1
                        .wait(completion)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
        }

        (
            completion.state,
            completion.terminal_status,
            completion.diagnostic.clone(),
            completion.result.clone(),
        )
    }

    fn capture_stream_batch(&self, batch: FfiLiveStreamBatch) -> Result<(), (Status, String)> {
        let mut stream = self.stream.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        stream.capture_batch(batch)
    }

    fn stream_batch(
        &self,
        after_sequence: u64,
        maximum_records: u32,
    ) -> Result<OperationStreamBatch, (Status, String)> {
        if !self.supports_live_stream_polling {
            return Err((
                Status::UnsupportedCapability,
                "The selected PowerShell payload does not support live stream polling.".to_owned(),
            ));
        }
        let (state, terminal_status, _, _) = self.snapshot();
        let stream = self.stream.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        stream.read_batch(state, terminal_status, after_sequence, maximum_records)
    }

    fn cancellation_requested(&self) -> bool {
        self.cancellation_requested.load(Ordering::Acquire)
    }

    fn clear_session_operation(&self) {
        let mut active = self
            .session
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = false;
        if let Some(runspace_session) = &self.runspace_session {
            let mut active = runspace_session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *active = false;
        }
        drop(active);
        // Invalidate the broker generation so a captured \ cannot
        // reach the channel after this invocation terminates.
        self.session.deactivate_broker();
    }

    fn cancel_capability(&self) {
        if let Some(capability) = &self.capability {
            capability.cancel();
        }
    }

    fn finish_capability(&self) {
        set_active_capability(&self.session, None);
        finish_capability(self.capability.as_ref());
    }
}

impl TypedResultOperation {
    fn new(
        session: Arc<Session>,
        capability: Option<CapabilityInvocation>,
        invocation: FfiTypedResultInvocation,
    ) -> Self {
        Self {
            session,
            capability,
            invocation: Mutex::new(Some(invocation)),
            pipeline_completed: AtomicBool::new(false),
            cancellation_requested: AtomicBool::new(false),
            finalized: AtomicBool::new(false),
            worker_error: Mutex::new(None),
        }
    }

    fn request_stop(&self) {
        if !self.cancellation_requested.swap(true, Ordering::AcqRel) {
            self.session.cancel_broker();
            self.cancel_capability();
            if let Some(invocation) = self
                .invocation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
            {
                let _ = invocation.stop();
            }
        }
    }

    fn read_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiTypedResultPage, (Status, String)> {
        if let Some(error) = self
            .worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            return Err(error);
        }

        let invocation = self.invocation.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let invocation = invocation.as_ref().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "typed result invocation has been released".to_owned(),
            )
        })?;
        let mut page = invocation
            .read_page(acknowledged_through, maximum_records)
            .map_err(managed_failure)?;
        if page.records.is_empty() && self.pipeline_completed.load(Ordering::Acquire) {
            invocation.complete().map_err(managed_failure)?;
            page = invocation
                .read_page(acknowledged_through, maximum_records)
                .map_err(managed_failure)?;
            self.finalize();
        }
        Ok(page)
    }

    fn poll_pipeline(&self) -> Result<bool, (Status, String)> {
        let invocation = self.invocation.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let invocation = invocation.as_ref().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "typed result invocation has been released".to_owned(),
            )
        })?;
        invocation.poll().map_err(managed_failure)
    }

    fn record_worker_error(&self, error: (Status, String)) {
        *self
            .worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
        self.request_stop();
    }

    fn abort(&self) {
        self.request_stop();
        self.invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        self.finalize();
    }

    fn finalize(&self) {
        if self.finalized.swap(true, Ordering::AcqRel) {
            return;
        }

        if self.capability.is_some() {
            let _ = unsafe { self.session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
        }
        set_active_capability(&self.session, None);
        finish_capability(self.capability.as_ref());
        self.clear_session_operation();
    }

    fn clear_session_operation(&self) {
        let mut active = self
            .session
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = false;
        if let Some(runspace_session) = &self.session.runspace_session {
            let mut active = runspace_session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *active = false;
        }
        drop(active);
        // Invalidate the broker generation so a captured \ cannot
        // reach the channel after this invocation terminates.
        self.session.deactivate_broker();
    }

    fn cancel_capability(&self) {
        if let Some(capability) = &self.capability {
            capability.cancel();
        }
    }
}

impl ObservedInvocationOperation {
    fn new(session: Arc<Session>, capability: Option<CapabilityInvocation>, invocation: FfiObservedInvocation) -> Self {
        Self {
            session,
            capability,
            invocation: Mutex::new(Some(Arc::new(invocation))),
            pipeline_completed: AtomicBool::new(false),
            cancellation_requested: AtomicBool::new(false),
            finalized: AtomicBool::new(false),
            worker_error: Mutex::new(None),
        }
    }

    fn request_stop(&self) {
        if !self.cancellation_requested.swap(true, Ordering::AcqRel) {
            self.session.cancel_broker();
            self.cancel_capability();
            let invocation = self
                .invocation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .cloned();
            if let Some(invocation) = invocation {
                let _ = invocation.stop();
            }
        }
    }

    fn read_result_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiTypedResultPage, (Status, String)> {
        if let Some(error) = self
            .worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            return Err(error);
        }

        let invocation = self
            .invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "observed invocation has been released".to_owned(),
                )
            })?;
        let page = invocation
            .read_result_page(acknowledged_through, maximum_records)
            .map_err(managed_failure)?;
        if page.is_complete && self.pipeline_completed.load(Ordering::Acquire) {
            self.finalize();
        }
        Ok(page)
    }

    fn read_diagnostic_page(
        &self,
        acknowledged_through: u64,
        maximum_records: u32,
    ) -> Result<FfiObservedDiagnosticPage, (Status, String)> {
        if let Some(error) = self
            .worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            return Err(error);
        }

        let invocation = self
            .invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "observed invocation has been released".to_owned(),
                )
            })?;
        let page = invocation
            .read_diagnostic_page(acknowledged_through, maximum_records)
            .map_err(managed_failure)?;
        if page.is_complete && self.pipeline_completed.load(Ordering::Acquire) {
            self.finalize();
        }
        Ok(page)
    }

    fn poll_pipeline(&self) -> Result<bool, (Status, String)> {
        let invocation = self
            .invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "observed invocation has been released".to_owned(),
                )
            })?;
        invocation.poll().map_err(managed_failure)
    }

    fn complete_pipeline(&self) -> Result<(), (Status, String)> {
        let invocation = self
            .invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "observed invocation has been released".to_owned(),
                )
            })?;
        invocation.complete().map_err(managed_failure)
    }

    fn record_worker_error(&self, error: (Status, String)) {
        *self
            .worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
        self.request_stop();
    }

    fn abort(&self) {
        self.request_stop();
        self.invocation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        self.finalize();
    }

    fn finalize(&self) {
        if self.finalized.swap(true, Ordering::AcqRel) {
            return;
        }

        if self.capability.is_some() {
            let _ = unsafe { self.session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
        }
        set_active_capability(&self.session, None);
        finish_capability(self.capability.as_ref());
        self.clear_session_operation();
    }

    fn clear_session_operation(&self) {
        let mut active = self
            .session
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = false;
        if let Some(runspace_session) = &self.session.runspace_session {
            let mut active = runspace_session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *active = false;
        }
        drop(active);
        // Invalidate the broker generation so a captured \ cannot
        // reach the channel after this invocation terminates.
        self.session.deactivate_broker();
    }

    fn cancel_capability(&self) {
        if let Some(capability) = &self.capability {
            capability.cancel();
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            runtime: None,
            activation_source_root: None,
            live_object_contract_packs: Vec::new(),
            sessions: HashMap::new(),
            runspace_sessions: HashMap::new(),
            results: HashMap::new(),
            operations: HashMap::new(),
            operation_stream_batches: HashMap::new(),
            typed_result_operations: HashMap::new(),
            typed_result_pages: HashMap::new(),
            observed_operations: HashMap::new(),
            observed_diagnostic_pages: HashMap::new(),
            capabilities: HashMap::new(),
            broker_channels: HashMap::new(),
            next_handle: 1,
            next_result_handle: 1_u64 << 63,
            next_operation_handle: 1_u64 << 62,
            next_operation_stream_batch_handle: 1_u64 << 59,
            next_typed_result_operation_handle: 1_u64 << 58,
            next_typed_result_page_handle: 1_u64 << 57,
            next_observed_operation_handle: 1_u64 << 56,
            next_observed_diagnostic_page_handle: 1_u64 << 55,
            next_runspace_session_handle: 1_u64 << 61,
            next_capability_handle: 1_u64 << 60,
            next_broker_channel_handle: 1_u64 << 54,
        }
    }
}

// The cdylib serializes all access through STATE. Managed delegates and handles
// never escape the mutex-protected State.
unsafe impl Send for State {}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static SESSION_OPERATION_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_PIPELINE_COUNT: AtomicU32 = AtomicU32::new(0);
#[cfg(test)]
static TEST_PIPELINE_SCOPE_LOCK: Mutex<()> = Mutex::new(());
static NEXT_CAPABILITY_INVOCATION_ID: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static CAPABILITY_CALLBACK_DEPTH: Cell<u32> = const { Cell::new(0) };
    static PIPELINE_EXECUTION_DEPTH: Cell<u32> = const { Cell::new(0) };
}
#[cfg(test)]
thread_local! {
    static TEST_FFI_CALL_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

struct InvocationExecutionScope {
    #[cfg(test)]
    _test_scope_lock: Option<MutexGuard<'static, ()>>,
}

#[cfg(test)]
struct TestFfiCallScope {
    _lock: MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestFfiCallScope {
    fn enter() -> Self {
        TEST_FFI_CALL_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self {
            _lock: TEST_PIPELINE_SCOPE_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }
}

#[cfg(test)]
impl Drop for TestFfiCallScope {
    fn drop(&mut self) {
        TEST_FFI_CALL_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

impl InvocationExecutionScope {
    fn enter() -> Self {
        #[cfg(test)]
        let test_scope_lock = if pipeline_execution_depth() == 0 && TEST_FFI_CALL_DEPTH.with(Cell::get) == 0 {
            Some(
                TEST_PIPELINE_SCOPE_LOCK
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            )
        } else {
            None
        };
        PIPELINE_EXECUTION_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        ACTIVE_PIPELINE_COUNT.fetch_add(1, Ordering::AcqRel);
        Self {
            #[cfg(test)]
            _test_scope_lock: test_scope_lock,
        }
    }
}

impl Drop for InvocationExecutionScope {
    fn drop(&mut self) {
        ACTIVE_PIPELINE_COUNT.fetch_sub(1, Ordering::AcqRel);
        PIPELINE_EXECUTION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn pipeline_execution_depth() -> u32 {
    PIPELINE_EXECUTION_DEPTH.with(Cell::get)
}

fn active_pipeline_count() -> u32 {
    ACTIVE_PIPELINE_COUNT.load(Ordering::Acquire)
}

fn reject_active_pipeline_ffi_call() -> Result<(), (Status, String)> {
    if pipeline_execution_depth() != 0 {
        return Err((
            Status::Backpressure,
            "PowerShell FFI calls are not permitted from code invoked by an active PowerShell pipeline.".to_owned(),
        ));
    }
    if active_pipeline_count() != 0 {
        return Err((
            Status::Backpressure,
            "PowerShell FFI calls are not permitted while any PowerShell pipeline is running.".to_owned(),
        ));
    }
    Ok(())
}

fn reject_active_pipeline_session_mutation() -> Result<(), (Status, String)> {
    if active_pipeline_count() != 0 {
        return Err((
            Status::Backpressure,
            "PowerShell session variables cannot be read or changed while any PowerShell pipeline is running."
                .to_owned(),
        ));
    }
    Ok(())
}

unsafe fn utf8_input<'a>(ptr: *const u8, len: usize) -> Result<&'a str, Status> {
    if ptr.is_null() {
        return Err(Status::InvalidArgument);
    }

    let bytes = slice::from_raw_parts(ptr, len);
    let value = std::str::from_utf8(bytes).map_err(|_| Status::InvalidArgument)?;
    if value.as_bytes().contains(&0) {
        return Err(Status::InvalidArgument);
    }

    Ok(value)
}

unsafe fn utf8_span<'a>(value: Utf8Span) -> Result<&'a str, Status> {
    if value.len == 0 {
        return Ok("");
    }

    utf8_input(value.data, value.len)
}

unsafe fn data_value_input<'a>(value: *const DataValue) -> Result<(u32, &'a [u8]), (Status, String)> {
    let (kind, payload) = data_value_input_unvalidated(value)?;
    validate_value_payload(kind, payload, 0)?;
    Ok((kind, payload))
}

unsafe fn data_value_input_unvalidated<'a>(value: *const DataValue) -> Result<(u32, &'a [u8]), (Status, String)> {
    if value.is_null() {
        return Err((Status::InvalidArgument, "tagged value pointer is null".to_owned()));
    }

    let value = &*value;
    if value.size < std::mem::size_of::<DataValue>() as u32 || value.flags != 0 || value._reserved != 0 {
        return Err((Status::InvalidArgument, "tagged value header is invalid".to_owned()));
    }
    if value.data_len > MAX_VALUE_PAYLOAD_BYTES || (value.data_len != 0 && value.data.is_null()) {
        return Err((
            Status::InvalidArgument,
            "tagged value payload is invalid or exceeds its bound".to_owned(),
        ));
    }

    let payload = if value.data_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(value.data, value.data_len)
    };
    Ok((value.kind, payload))
}

fn validate_secret_parameter_value(kind: u32, payload: &[u8]) -> Result<(), (Status, String)> {
    fn validate_secret_utf16(payload: &[u8]) -> Result<(), (Status, String)> {
        if payload.is_empty()
            || payload.len() > MAX_SECRET_UTF16_CODE_UNITS * std::mem::size_of::<u16>()
            || !payload.len().is_multiple_of(std::mem::size_of::<u16>())
            || payload
                .chunks_exact(std::mem::size_of::<u16>())
                .any(|unit| u16::from_ne_bytes([unit[0], unit[1]]) == 0)
        {
            return Err((Status::InvalidArgument, "secret adapter payload is invalid".to_owned()));
        }
        Ok(())
    }

    match kind {
        SECRET_VALUE_KIND_UTF16 => validate_secret_utf16(payload),
        SECRET_VALUE_KIND_CREDENTIAL => {
            if payload.len() < std::mem::size_of::<u32>() {
                return Err((Status::InvalidArgument, "secret adapter payload is invalid".to_owned()));
            }
            let user_name_length = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
            if user_name_length == 0
                || user_name_length > MAX_SECRET_USER_NAME_UTF8_BYTES
                || payload.len() <= std::mem::size_of::<u32>() + user_name_length
            {
                return Err((Status::InvalidArgument, "secret adapter payload is invalid".to_owned()));
            }
            let user_name = &payload[std::mem::size_of::<u32>()..std::mem::size_of::<u32>() + user_name_length];
            if user_name.contains(&0) || std::str::from_utf8(user_name).is_err() {
                return Err((Status::InvalidArgument, "secret adapter payload is invalid".to_owned()));
            }
            validate_secret_utf16(&payload[std::mem::size_of::<u32>() + user_name_length..])
        }
        _ => validate_value_payload(kind, payload, 0),
    }
}

unsafe fn live_object_contract_input<'a>(
    value: *const FfiLiveObjectContractDescriptor,
) -> Result<&'a FfiLiveObjectContractDescriptor, (Status, String)> {
    if value.is_null() {
        return Err((
            Status::InvalidArgument,
            "live object contract descriptor is null".to_owned(),
        ));
    }

    let value = &*value;
    if value.size < mem::size_of::<FfiLiveObjectContractDescriptor>() as u32
        || value.directions == 0
        || value.directions & !0x07 != 0
        // A bridge contract marker (0x04) is an addition to a consumer-to-session
        // surface (0x02), never a replacement for it. This validator is
        // independent of the managed one on purpose, so it enforces the pairing
        // itself rather than trusting the caller to have done so.
        || value.directions & 0x04 != 0 && value.directions & 0x02 == 0
        || value.interface_id_low == 0 && value.interface_id_high == 0
        || value.major_version == 0
        || value.reserved != 0
    {
        return Err((
            Status::InvalidArgument,
            "live object contract descriptor is invalid".to_owned(),
        ));
    }

    Ok(value)
}

unsafe fn live_object_contract_pack_inputs(
    values: *const LiveObjectContractPackInput,
    count: usize,
) -> Result<Vec<LiveObjectContractPack>, (Status, String)> {
    if count == 0 || count > MAX_LIVE_OBJECT_CONTRACT_PACKS || values.is_null() {
        return Err((
            Status::InvalidArgument,
            "live object contract pack input is invalid".to_owned(),
        ));
    }

    let values = slice::from_raw_parts(values, count);
    let mut packs = Vec::with_capacity(count);
    for value in values {
        if value.size < mem::size_of::<LiveObjectContractPackInput>() as u32 || value.flags != 0 {
            return Err((
                Status::InvalidArgument,
                "live object contract pack header is invalid".to_owned(),
            ));
        }

        let assembly_path = utf8_span(value.payload_adapter_assembly_path).map_err(|_| {
            (
                Status::InvalidArgument,
                "live object contract pack assembly path must be UTF-8 without NUL".to_owned(),
            )
        })?;
        let adapter_type_name = utf8_span(value.payload_adapter_type_name).map_err(|_| {
            (
                Status::InvalidArgument,
                "live object contract pack adapter type must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if adapter_type_name.is_empty() || adapter_type_name.len() > MAX_LIVE_OBJECT_CONTRACT_PACK_TYPE_NAME_BYTES {
            return Err((
                Status::InvalidArgument,
                "live object contract pack adapter type is invalid".to_owned(),
            ));
        }

        let assembly_path = PathBuf::from(assembly_path);
        if !assembly_path.is_absolute() {
            return Err((
                Status::InvalidArgument,
                "live object contract pack assembly path must be absolute".to_owned(),
            ));
        }
        let assembly_path = fs::canonicalize(&assembly_path).map_err(|error| {
            (
                Status::InvalidArgument,
                format!("live object contract pack assembly cannot be resolved: {}", error),
            )
        })?;
        if !assembly_path.is_file() {
            return Err((
                Status::InvalidArgument,
                "live object contract pack assembly path must name a file".to_owned(),
            ));
        }

        packs.push(LiveObjectContractPack {
            payload_adapter_assembly_path: assembly_path,
            payload_adapter_type_name: adapter_type_name.to_owned(),
        });
    }

    Ok(packs)
}

fn validate_value_payload(kind: u32, payload: &[u8], depth: u8) -> Result<(), (Status, String)> {
    if depth > MAX_VALUE_DEPTH {
        return Err((
            Status::InvalidArgument,
            "tagged value nesting exceeds its bound".to_owned(),
        ));
    }

    match kind {
        0 => require_value_length(payload, 0, "null"),
        1 | 7 | 11 | 12 => validate_value_utf8(payload),
        2 | 3 => {
            require_value_length(payload, 1, "boolean")?;
            if payload[0] > 1 {
                return Err((
                    Status::InvalidArgument,
                    "boolean payload must be zero or one".to_owned(),
                ));
            }
            Ok(())
        }
        4 | 5 | 6 | 9 | 17 => require_value_length(payload, 8, "numeric"),
        8 => Ok(()),
        10 => require_value_length(payload, 10, "date-time offset"),
        13 => validate_value_array(payload, depth + 1),
        14 => validate_value_property_bag(payload, depth + 1),
        _ => Err((
            Status::UnsupportedValue,
            format!("tagged value kind {} is not supported", kind),
        )),
    }
}

fn require_value_length(payload: &[u8], expected: usize, description: &str) -> Result<(), (Status, String)> {
    if payload.len() == expected {
        Ok(())
    } else {
        Err((
            Status::InvalidArgument,
            format!("{} tagged value payload length is invalid", description),
        ))
    }
}

fn validate_value_utf8(payload: &[u8]) -> Result<(), (Status, String)> {
    if payload.contains(&0) || std::str::from_utf8(payload).is_err() {
        return Err((Status::InvalidArgument, "tagged UTF-8 value is invalid".to_owned()));
    }
    Ok(())
}

fn read_value_u32(payload: &[u8], offset: &mut usize, description: &str) -> Result<u32, (Status, String)> {
    let bytes = read_value_bytes(payload, offset, 4, description)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_value_bytes<'a>(
    payload: &'a [u8],
    offset: &mut usize,
    length: usize,
    description: &str,
) -> Result<&'a [u8], (Status, String)> {
    let end = offset.checked_add(length).ok_or_else(|| {
        (
            Status::InvalidArgument,
            format!("{} tagged value payload is invalid", description),
        )
    })?;
    if end > payload.len() {
        return Err((
            Status::InvalidArgument,
            format!("{} tagged value payload is truncated", description),
        ));
    }
    let bytes = &payload[*offset..end];
    *offset = end;
    Ok(bytes)
}

fn validate_nested_value(payload: &[u8], offset: &mut usize, depth: u8) -> Result<(), (Status, String)> {
    let kind = read_value_u32(payload, offset, "nested")?;
    let length = read_value_u32(payload, offset, "nested")? as usize;
    let nested = read_value_bytes(payload, offset, length, "nested")?;
    validate_value_payload(kind, nested, depth)
}

fn validate_value_array(payload: &[u8], depth: u8) -> Result<(), (Status, String)> {
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, "array count")?;
    if count > MAX_VALUE_CONTAINER_ENTRIES {
        return Err((Status::InvalidArgument, "array item count exceeds its bound".to_owned()));
    }
    for _ in 0..count {
        validate_nested_value(payload, &mut offset, depth)?;
    }
    if offset == payload.len() {
        Ok(())
    } else {
        Err((
            Status::InvalidArgument,
            "array payload contains trailing bytes".to_owned(),
        ))
    }
}

fn validate_value_property_bag(payload: &[u8], depth: u8) -> Result<(), (Status, String)> {
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, "property bag count")?;
    if count > MAX_VALUE_CONTAINER_ENTRIES {
        return Err((
            Status::InvalidArgument,
            "property bag entry count exceeds its bound".to_owned(),
        ));
    }
    for _ in 0..count {
        let key_length = read_value_u32(payload, &mut offset, "property bag key")? as usize;
        let key = read_value_bytes(payload, &mut offset, key_length, "property bag key")?;
        if key.is_empty() {
            return Err((
                Status::InvalidArgument,
                "property bag keys must be non-empty".to_owned(),
            ));
        }
        validate_value_utf8(key)?;
        validate_nested_value(payload, &mut offset, depth)?;
    }
    if offset == payload.len() {
        Ok(())
    } else {
        Err((
            Status::InvalidArgument,
            "property bag payload contains trailing bytes".to_owned(),
        ))
    }
}

fn read_nested_value<'a>(
    payload: &'a [u8],
    offset: &mut usize,
    description: &str,
) -> Result<(u32, &'a [u8]), (Status, String)> {
    let kind = read_value_u32(payload, offset, description)?;
    let length = read_value_u32(payload, offset, description)? as usize;
    let value = read_value_bytes(payload, offset, length, description)?;
    validate_value_payload(kind, value, 0)?;
    Ok((kind, value))
}

fn read_value_array<'a>(payload: &'a [u8], description: &str) -> Result<Vec<(u32, &'a [u8])>, (Status, String)> {
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, description)?;
    if count > MAX_VALUE_CONTAINER_ENTRIES {
        return Err((Status::InvalidArgument, format!("{} exceeds its bound", description)));
    }

    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(read_nested_value(payload, &mut offset, description)?);
    }
    if offset != payload.len() {
        return Err((
            Status::InvalidArgument,
            format!("{} contains trailing bytes", description),
        ));
    }
    Ok(values)
}

fn read_property_bag<'a>(
    payload: &'a [u8],
    description: &str,
) -> Result<HashMap<&'a str, (u32, &'a [u8])>, (Status, String)> {
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, description)?;
    if count > MAX_VALUE_CONTAINER_ENTRIES {
        return Err((Status::InvalidArgument, format!("{} exceeds its bound", description)));
    }

    let mut values = HashMap::with_capacity(count as usize);
    for _ in 0..count {
        let key_length = read_value_u32(payload, &mut offset, description)? as usize;
        let key = read_value_bytes(payload, &mut offset, key_length, description)?;
        let key = std::str::from_utf8(key)
            .map_err(|_| (Status::InvalidArgument, format!("{} has an invalid key", description)))?;
        if key.is_empty() || key.as_bytes().contains(&0) {
            return Err((Status::InvalidArgument, format!("{} has an invalid key", description)));
        }
        let value = read_nested_value(payload, &mut offset, description)?;
        if values.insert(key, value).is_some() {
            return Err((Status::InvalidArgument, format!("{} has duplicate keys", description)));
        }
    }
    if offset != payload.len() {
        return Err((
            Status::InvalidArgument,
            format!("{} contains trailing bytes", description),
        ));
    }
    Ok(values)
}

fn required_property<'a>(
    properties: &'a HashMap<&str, (u32, &'a [u8])>,
    key: &str,
    description: &str,
) -> Result<(u32, &'a [u8]), (Status, String)> {
    properties
        .get(key)
        .copied()
        .ok_or_else(|| (Status::InvalidArgument, format!("{} is missing {}", description, key)))
}

fn read_unsigned_value(kind: u32, payload: &[u8], description: &str) -> Result<u64, (Status, String)> {
    if kind != VALUE_KIND_UNSIGNED_INTEGER || payload.len() != 8 {
        return Err((
            Status::InvalidArgument,
            format!("{} must be an unsigned integer", description),
        ));
    }
    Ok(u64::from_le_bytes([
        payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6], payload[7],
    ]))
}

fn read_capability_kind_list(kind: u32, payload: &[u8], description: &str) -> Result<Vec<u32>, (Status, String)> {
    if kind != VALUE_KIND_ARRAY {
        return Err((Status::InvalidArgument, format!("{} must be an array", description)));
    }
    let values = read_value_array(payload, description)?;
    if values.is_empty() {
        return Err((Status::InvalidArgument, format!("{} must not be empty", description)));
    }
    let mut kinds = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for (kind, value) in values {
        let value_kind = read_unsigned_value(kind, value, description)?;
        let value_kind = u32::try_from(value_kind).map_err(|_| {
            (
                Status::InvalidArgument,
                format!("{} contains an invalid value kind", description),
            )
        })?;
        if (value_kind > VALUE_KIND_PROPERTY_BAG && value_kind != VALUE_KIND_TIME_SPAN) || !seen.insert(value_kind) {
            return Err((
                Status::InvalidArgument,
                format!("{} contains an invalid or duplicate value kind", description),
            ));
        }
        kinds.push(value_kind);
    }
    Ok(kinds)
}

fn is_canonical_capability_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_CAPABILITY_NAME_BYTES {
        return false;
    }

    let mut previous_separator = true;
    let mut has_namespace_separator = false;
    for byte in bytes {
        let separator = matches!(*byte, b'.' | b'-');
        if !matches!(*byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-') || (separator && previous_separator) {
            return false;
        }
        if *byte == b'.' {
            has_namespace_separator = true;
        }
        previous_separator = separator;
    }
    has_namespace_separator && !previous_separator
}

fn parse_capability_definitions(
    kind: u32,
    payload: &[u8],
) -> Result<HashMap<String, CapabilityDefinition>, (Status, String)> {
    if kind != VALUE_KIND_PROPERTY_BAG {
        return Err((
            Status::InvalidArgument,
            "capability registration must be a tagged property bag".to_owned(),
        ));
    }
    let root = read_property_bag(payload, "capability registration")?;
    if root.len() != 2 {
        return Err((
            Status::InvalidArgument,
            "capability registration contains unsupported fields".to_owned(),
        ));
    }
    let (protocol_kind, protocol_value) = required_property(&root, "protocol", "capability registration")?;
    if read_unsigned_value(protocol_kind, protocol_value, "capability protocol")?
        != u64::from(CAPABILITY_REGISTRATION_VERSION)
    {
        return Err((
            Status::UnsupportedCapability,
            "capability protocol version is unsupported".to_owned(),
        ));
    }
    let (capabilities_kind, capabilities_value) = required_property(&root, "capabilities", "capability registration")?;
    if capabilities_kind != VALUE_KIND_ARRAY {
        return Err((Status::InvalidArgument, "capabilities must be an array".to_owned()));
    }

    let items = read_value_array(capabilities_value, "capabilities")?;
    if items.is_empty() || items.len() > MAX_CAPABILITIES {
        return Err((
            Status::InvalidArgument,
            "capability count is outside its bound".to_owned(),
        ));
    }
    let mut definitions = HashMap::with_capacity(items.len());
    for (kind, value) in items {
        if kind != VALUE_KIND_PROPERTY_BAG {
            return Err((
                Status::InvalidArgument,
                "capability definition must be a property bag".to_owned(),
            ));
        }
        let properties = read_property_bag(value, "capability definition")?;
        if properties.len() != 7 {
            return Err((
                Status::InvalidArgument,
                "capability definition contains unsupported fields".to_owned(),
            ));
        }
        let (name_kind, name_value) = required_property(&properties, "name", "capability definition")?;
        if name_kind != VALUE_KIND_STRING {
            return Err((Status::InvalidArgument, "capability name must be UTF-8 text".to_owned()));
        }
        let name = std::str::from_utf8(name_value)
            .map_err(|_| (Status::InvalidArgument, "capability name is invalid UTF-8".to_owned()))?;
        if !is_canonical_capability_name(name) {
            return Err((Status::InvalidArgument, "capability name is not canonical".to_owned()));
        }

        let (permissions_kind, permissions_value) =
            required_property(&properties, "permissions", "capability definition")?;
        let permissions = u32::try_from(read_unsigned_value(
            permissions_kind,
            permissions_value,
            "capability permissions",
        )?)
        .map_err(|_| (Status::InvalidArgument, "capability permissions are invalid".to_owned()))?;
        if permissions == 0 || permissions & !MAX_CAPABILITY_PERMISSIONS != 0 {
            return Err((Status::InvalidArgument, "capability permissions are invalid".to_owned()));
        }

        let (input_kind, input_value) = required_property(&properties, "maximumInputBytes", "capability definition")?;
        let maximum_input_bytes = usize::try_from(read_unsigned_value(
            input_kind,
            input_value,
            "capability maximum input bytes",
        )?)
        .map_err(|_| {
            (
                Status::InvalidArgument,
                "capability maximum input bytes are invalid".to_owned(),
            )
        })?;
        let (output_kind, output_value) =
            required_property(&properties, "maximumOutputBytes", "capability definition")?;
        let maximum_output_bytes = usize::try_from(read_unsigned_value(
            output_kind,
            output_value,
            "capability maximum output bytes",
        )?)
        .map_err(|_| {
            (
                Status::InvalidArgument,
                "capability maximum output bytes are invalid".to_owned(),
            )
        })?;
        if maximum_input_bytes == 0
            || maximum_input_bytes > MAX_VALUE_PAYLOAD_BYTES
            || maximum_output_bytes == 0
            || maximum_output_bytes > MAX_VALUE_PAYLOAD_BYTES
        {
            return Err((Status::InvalidArgument, "capability byte bounds are invalid".to_owned()));
        }

        let (deadline_kind, deadline_value) =
            required_property(&properties, "deadlineMilliseconds", "capability definition")?;
        let deadline_milliseconds = u32::try_from(read_unsigned_value(
            deadline_kind,
            deadline_value,
            "capability deadline",
        )?)
        .map_err(|_| (Status::InvalidArgument, "capability deadline is invalid".to_owned()))?;
        if deadline_milliseconds == 0 || deadline_milliseconds > MAX_CAPABILITY_DEADLINE_MILLISECONDS {
            return Err((Status::InvalidArgument, "capability deadline is invalid".to_owned()));
        }

        let (arguments_kind, arguments_value) = required_property(&properties, "arguments", "capability definition")?;
        if arguments_kind != VALUE_KIND_ARRAY {
            return Err((
                Status::InvalidArgument,
                "capability arguments must be an array".to_owned(),
            ));
        }
        let argument_values = read_value_array(arguments_value, "capability arguments")?;
        let mut argument_kinds = Vec::with_capacity(argument_values.len());
        for (argument_kind, argument_value) in argument_values {
            argument_kinds.push(read_capability_kind_list(
                argument_kind,
                argument_value,
                "capability argument schema",
            )?);
        }

        let (responses_kind, responses_value) =
            required_property(&properties, "responseKinds", "capability definition")?;
        let response_kinds = read_capability_kind_list(responses_kind, responses_value, "capability response schema")?;
        let definition = CapabilityDefinition {
            argument_kinds,
            response_kinds,
            maximum_input_bytes,
            maximum_output_bytes,
            deadline_milliseconds,
        };
        if definitions.insert(name.to_owned(), definition).is_some() {
            return Err((Status::InvalidArgument, "capability names must be unique".to_owned()));
        }
    }
    Ok(definitions)
}

struct CapabilityCallbackScope;

impl CapabilityCallbackScope {
    fn enter() -> Self {
        CAPABILITY_CALLBACK_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for CapabilityCallbackScope {
    fn drop(&mut self) {
        CAPABILITY_CALLBACK_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

impl CapabilityRegistrationState {
    fn begin_invocation(self: &Arc<Self>, invocation_id: u64) -> Result<CapabilityInvocation, (Status, String)> {
        if !self.active.load(Ordering::Acquire) {
            return Err((
                Status::InvalidHandle,
                "capability registration has been released".to_owned(),
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut invocations = self.invocations.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if invocations.insert(invocation_id, Arc::clone(&cancelled)).is_some() {
            return Err((
                Status::ManagedFailure,
                "capability invocation identifier collision".to_owned(),
            ));
        }
        Ok(CapabilityInvocation {
            registration: Arc::clone(self),
            invocation_id,
            cancelled,
        })
    }

    fn end_invocation(&self, invocation_id: u64) {
        self.invocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&invocation_id);
    }

    fn cancel_invocation(&self, invocation_id: u64, cancelled: &AtomicBool) {
        if !cancelled.swap(true, Ordering::AcqRel) {
            unsafe {
                (self.cancel)(self.handle, invocation_id);
            }
        }
    }

    fn revoke(&self) {
        self.active.store(false, Ordering::Release);
        let active = self
            .invocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        for (invocation_id, cancelled) in active {
            self.cancel_invocation(invocation_id, &cancelled);
        }
    }
}

impl CapabilityInvocation {
    fn cancel(&self) {
        self.registration.cancel_invocation(self.invocation_id, &self.cancelled);
    }
}

fn finish_capability(capability: Option<&CapabilityInvocation>) {
    if let Some(capability) = capability {
        capability.registration.end_invocation(capability.invocation_id);
    }
}

fn cancel_and_finish_capability(capability: Option<&CapabilityInvocation>) {
    if let Some(capability) = capability {
        capability.cancel();
    }
    finish_capability(capability);
}

fn take_session_capability(session: &Session) -> Result<Option<CapabilityInvocation>, (Status, String)> {
    let registration = session
        .capability_registration
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let Some(registration) = registration else {
        return Ok(None);
    };
    let invocation_id = NEXT_CAPABILITY_INVOCATION_ID.fetch_add(1, Ordering::AcqRel);
    if invocation_id == 0 {
        return Err((
            Status::ManagedFailure,
            "capability invocation identifier allocation failed".to_owned(),
        ));
    }
    registration.begin_invocation(invocation_id).map(Some)
}

fn set_active_capability(session: &Session, capability: Option<CapabilityInvocation>) {
    *session
        .active_capability
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = capability;
}

fn cancel_active_capability(session: &Session) {
    if let Some(capability) = session
        .active_capability
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
    {
        capability.cancel();
    }
}

fn invoke_with_capability(
    session: &Session,
    capability: Option<CapabilityInvocation>,
) -> Result<FfiInvocationResult, FfiBindingError> {
    if let Some(capability) = capability {
        set_active_capability(session, Some(capability.clone()));
        let configured = unsafe {
            session.power_shell.set_capability_context(
                capability.registration.handle,
                capability.invocation_id,
                capability_dispatch as *const () as *const _,
            )
        };
        if let Err(error) = configured {
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
            return Err(error);
        }

        let result = {
            let _execution_scope = InvocationExecutionScope::enter();
            session.power_shell.invoke_to_result()
        };
        let clear = unsafe { session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
        set_active_capability(session, None);
        capability.registration.end_invocation(capability.invocation_id);
        result.and_then(|result| clear.map(|_| result))
    } else {
        let _execution_scope = InvocationExecutionScope::enter();
        session.power_shell.invoke_to_result()
    }
}

/// Installs the payload broker context for one asynchronous invocation.
/// Called before the invocation starts; the generation is invalidated when the
/// operation reaches a terminal state.
fn configure_broker(session: &Session, attachment: Option<&Arc<BrokerAttachment>>) -> Result<(), FfiBindingError> {
    let Some(attachment) = attachment else {
        return Ok(());
    };
    set_active_broker(session, Some(Arc::clone(attachment)));
    broker_register_attachment(attachment);
    let configured = unsafe {
        session.power_shell.set_broker_context(
            attachment.channel_handle,
            attachment.generation,
            broker_enqueue_and_wait as *const () as *const _,
            broker_post as *const () as *const _,
            attachment.channel.max_body_bytes,
        )
    };
    if let Err(error) = configured {
        finish_broker(session, Some(attachment));
        return Err(error);
    }
    if let Some(bridge) = &attachment.bridge {
        let configured = session.power_shell.set_bridge_context(&FfiBridgeContext {
            binding_id: bridge.binding_id,
            contract_id_low: bridge.contract_id_low,
            contract_id_high: bridge.contract_id_high,
            contract_major_version: bridge.contract_major_version,
            contract_minor_version: bridge.contract_minor_version,
            maximum_request_bytes: bridge.maximum_request_bytes,
            maximum_reply_bytes: bridge.maximum_reply_bytes,
            variable_name: &bridge.variable_name,
        });
        if let Err(error) = configured {
            clear_broker_context(session);
            finish_broker(session, Some(attachment));
            return Err(error);
        }
    }
    Ok(())
}

fn begin_live_invocation_with_capability(
    session: &Session,
    capability: Option<CapabilityInvocation>,
) -> Result<FfiLiveInvocation, FfiBindingError> {
    if let Some(capability) = capability {
        set_active_capability(session, Some(capability.clone()));
        let configured = unsafe {
            session.power_shell.set_capability_context(
                capability.registration.handle,
                capability.invocation_id,
                capability_dispatch as *const () as *const _,
            )
        };
        if let Err(error) = configured {
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
            return Err(error);
        }
        let invocation = session.power_shell.begin_live_invocation();
        if invocation.is_err() {
            let _ = unsafe { session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
        }
        invocation
    } else {
        session.power_shell.begin_live_invocation()
    }
}

fn begin_typed_result_invocation_with_capability(
    session: &Session,
    capability: Option<CapabilityInvocation>,
    maximum_buffered_records: u32,
    maximum_page_records: u32,
) -> Result<FfiTypedResultInvocation, FfiBindingError> {
    if let Some(capability) = capability {
        set_active_capability(session, Some(capability.clone()));
        let configured = unsafe {
            session.power_shell.set_capability_context(
                capability.registration.handle,
                capability.invocation_id,
                capability_dispatch as *const () as *const _,
            )
        };
        if let Err(error) = configured {
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
            return Err(error);
        }
        let invocation = session
            .power_shell
            .begin_typed_result_invocation(maximum_buffered_records, maximum_page_records);
        if invocation.is_err() {
            let _ = unsafe { session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
        }
        invocation
    } else {
        session
            .power_shell
            .begin_typed_result_invocation(maximum_buffered_records, maximum_page_records)
    }
}

fn begin_observed_invocation_with_capability(
    session: &Session,
    capability: Option<CapabilityInvocation>,
    maximum_buffered_result_records: u32,
    maximum_result_page_records: u32,
    maximum_buffered_diagnostic_records: u32,
    maximum_diagnostic_page_records: u32,
) -> Result<FfiObservedInvocation, FfiBindingError> {
    if let Some(capability) = capability {
        set_active_capability(session, Some(capability.clone()));
        let configured = unsafe {
            session.power_shell.set_capability_context(
                capability.registration.handle,
                capability.invocation_id,
                capability_dispatch as *const () as *const _,
            )
        };
        if let Err(error) = configured {
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
            return Err(error);
        }
        let invocation = session.power_shell.begin_observed_invocation(
            maximum_buffered_result_records,
            maximum_result_page_records,
            maximum_buffered_diagnostic_records,
            maximum_diagnostic_page_records,
        );
        if invocation.is_err() {
            let _ = unsafe { session.power_shell.set_capability_context(0, 0, std::ptr::null()) };
            set_active_capability(session, None);
            capability.registration.end_invocation(capability.invocation_id);
        }
        invocation
    } else {
        session.power_shell.begin_observed_invocation(
            maximum_buffered_result_records,
            maximum_result_page_records,
            maximum_buffered_diagnostic_records,
            maximum_diagnostic_page_records,
        )
    }
}

fn status_from_callback(value: i32) -> Status {
    match value {
        0 => Status::Success,
        1 => Status::BufferTooSmall,
        -1 => Status::InvalidArgument,
        -4 => Status::InvalidHandle,
        -6 => Status::ManagedFailure,
        -9 => Status::Backpressure,
        -10 => Status::UnsupportedValue,
        -11 => Status::OperationCancelled,
        -17 => Status::UnsupportedCapability,
        _ => Status::ManagedFailure,
    }
}

unsafe extern "C" fn capability_dispatch(
    registration_handle: u64,
    invocation_id: u64,
    name: Utf8Span,
    arguments: *const DataValue,
    argument_count: u32,
    deadline_milliseconds: u32,
    response_kind: *mut u32,
    response_buffer: *mut u8,
    response_capacity: usize,
    response_required: *mut usize,
    result: *mut CallResult,
) -> i32 {
    let result = match prepare_call_result(result) {
        Ok(result) => result,
        Err(status) => return status.value(),
    };
    let dispatch = || -> Result<Status, (Status, String)> {
        if response_kind.is_null() || response_required.is_null() {
            return Err((
                Status::InvalidArgument,
                "capability response output pointer is null".to_owned(),
            ));
        }
        let registration = {
            let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.capabilities.get(&registration_handle).cloned().ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "capability registration has been released".to_owned(),
                )
            })?
        };
        if !registration.active.load(Ordering::Acquire) {
            return Err((
                Status::InvalidHandle,
                "capability registration has been released".to_owned(),
            ));
        }
        let name = utf8_span(name).map_err(|status| (status, "capability name is invalid".to_owned()))?;
        if !is_canonical_capability_name(name) {
            return Err((Status::InvalidArgument, "capability name is not canonical".to_owned()));
        }
        let definition = registration.definitions.get(name).ok_or_else(|| {
            (
                Status::UnsupportedCapability,
                "capability is not registered for this invocation".to_owned(),
            )
        })?;
        if deadline_milliseconds == 0 {
            return Err((Status::InvalidArgument, "capability deadline is invalid".to_owned()));
        }
        let effective_deadline_milliseconds = deadline_milliseconds.min(definition.deadline_milliseconds);
        let cancelled = registration
            .invocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&invocation_id)
            .cloned()
            .ok_or_else(|| {
                (
                    Status::InvalidHandle,
                    "capability invocation is no longer active".to_owned(),
                )
            })?;
        if cancelled.load(Ordering::Acquire) {
            return Err((
                Status::OperationCancelled,
                "capability invocation was cancelled".to_owned(),
            ));
        }
        let (argument_kind, argument_payload) = data_value_input(arguments)?;
        if argument_kind != VALUE_KIND_ARRAY
            || argument_payload.len() > definition.maximum_input_bytes
            || argument_count as usize != definition.argument_kinds.len()
        {
            return Err((
                Status::InvalidArgument,
                "capability argument envelope is invalid".to_owned(),
            ));
        }
        let values = read_value_array(argument_payload, "capability arguments")?;
        if values.len() != definition.argument_kinds.len() {
            return Err((
                Status::InvalidArgument,
                "capability argument count is invalid".to_owned(),
            ));
        }
        for ((kind, _), allowed) in values.iter().zip(definition.argument_kinds.iter()) {
            if !allowed.contains(kind) {
                return Err((
                    Status::InvalidArgument,
                    "capability argument type is invalid".to_owned(),
                ));
            }
        }
        if response_capacity < definition.maximum_output_bytes || response_buffer.is_null() {
            return Err((
                Status::InvalidArgument,
                "capability response buffer is invalid".to_owned(),
            ));
        }

        let mut callback_result = CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: Status::Success.value(),
            flags: 0,
            _reserved: 0,
            diagnostic: std::ptr::null_mut(),
            diagnostic_capacity: 0,
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        let _scope = CapabilityCallbackScope::enter();
        let status = (registration.dispatch)(
            registration_handle,
            invocation_id,
            Utf8Span {
                data: name.as_ptr(),
                len: name.len(),
            },
            arguments,
            argument_count,
            effective_deadline_milliseconds,
            response_kind,
            response_buffer,
            response_capacity,
            response_required,
            &mut callback_result,
        );
        let callback_status = status_from_callback(if callback_result.status != Status::Success.value() {
            callback_result.status
        } else {
            status
        });
        if callback_status != Status::Success {
            return Err((callback_status, "capability callback failed".to_owned()));
        }
        if cancelled.load(Ordering::Acquire) {
            return Err((
                Status::OperationCancelled,
                "capability invocation was cancelled".to_owned(),
            ));
        }
        if *response_required > definition.maximum_output_bytes || *response_required > response_capacity {
            return Err((
                Status::InvalidArgument,
                "capability response exceeds its bound".to_owned(),
            ));
        }
        let response = slice::from_raw_parts(response_buffer, *response_required);
        validate_value_payload(*response_kind, response, 0)?;
        if !definition.response_kinds.contains(&*response_kind) {
            return Err((
                Status::InvalidArgument,
                "capability response type is invalid".to_owned(),
            ));
        }
        Ok(Status::Success)
    };
    match catch_unwind(AssertUnwindSafe(dispatch)) {
        Ok(Ok(status)) => complete_call_result(result, status, ""),
        Ok(Err((status, _))) => complete_call_result(result, status, "capability dispatch failed"),
        Err(_) => complete_call_result(result, Status::Panic, "capability dispatch panic was contained"),
    }
}

unsafe fn write_bytes(buffer: *mut u8, buffer_len: usize, required_len: *mut usize, value: &[u8]) -> Status {
    if required_len.is_null() {
        return Status::InvalidArgument;
    }

    *required_len = value.len();
    if buffer_len < value.len() {
        return Status::BufferTooSmall;
    }

    if value.is_empty() {
        return Status::Success;
    }

    if buffer.is_null() {
        return Status::InvalidArgument;
    }

    std::ptr::copy_nonoverlapping(value.as_ptr(), buffer, value.len());
    Status::Success
}

unsafe fn write_utf8(buffer: *mut u8, buffer_len: usize, required_len: *mut usize, value: &str) -> Status {
    write_bytes(buffer, buffer_len, required_len, value.as_bytes())
}

unsafe fn prepare_call_result<'a>(result: *mut CallResult) -> Result<&'a mut CallResult, Status> {
    if result.is_null() || (*result).size < std::mem::size_of::<CallResult>() as u32 {
        return Err(Status::InvalidArgument);
    }

    if (*result).diagnostic_capacity != 0 && (*result).diagnostic.is_null() {
        return Err(Status::InvalidArgument);
    }

    (*result).status = Status::Success.value();
    (*result).flags = 0;
    (*result).diagnostic_required = 0;
    (*result).diagnostic_written = 0;
    Ok(&mut *result)
}

unsafe fn complete_call_result(result: &mut CallResult, status: Status, diagnostic: &str) -> i32 {
    let diagnostic_bytes = diagnostic.as_bytes();
    result.status = status.value();
    result.flags = 0;
    result.diagnostic_required = diagnostic_bytes.len();
    result.diagnostic_written = diagnostic_bytes.len().min(result.diagnostic_capacity);

    if result.diagnostic_written != 0 {
        std::ptr::copy_nonoverlapping(diagnostic_bytes.as_ptr(), result.diagnostic, result.diagnostic_written);
    }
    if result.diagnostic_written != result.diagnostic_required {
        result.flags |= CALL_RESULT_DIAGNOSTIC_TRUNCATED;
    }

    status.value()
}

unsafe fn v2_call<F>(result: *mut CallResult, operation: F) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    v2_call_with_active_pipeline_policy(result, false, operation)
}

unsafe fn v2_call_allow_active_pipeline<F>(result: *mut CallResult, operation: F) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    v2_call_with_active_pipeline_policy(result, true, operation)
}

/// Broker data-plane exports. They run while a pipeline is active and are the
/// only exports a thread holding a delivery handle may call.
unsafe fn v2_broker_call<F>(result: *mut CallResult, operation: F) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    v2_call_with_policy(result, true, false, operation)
}

unsafe fn v2_call_with_active_pipeline_policy<F>(
    result: *mut CallResult,
    allow_active_pipeline: bool,
    operation: F,
) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    v2_call_with_policy(result, allow_active_pipeline, true, operation)
}

unsafe fn v2_call_with_policy<F>(
    result: *mut CallResult,
    allow_active_pipeline: bool,
    enforce_broker_dispatch: bool,
    operation: F,
) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    let result = match prepare_call_result(result) {
        Ok(result) => result,
        Err(status) => return status.value(),
    };
    // Evaluated before any test-only scope lock so a misusing call fails
    // deterministically instead of deadlocking.
    if enforce_broker_dispatch && broker_holds_delivery_handle() {
        return complete_call_result(
            result,
            Status::BrokerDispatchViolation,
            "PowerShell FFI calls are not permitted while a broker frame is held; copy the frame, release it, and dispatch the work.",
        );
    }
    #[cfg(test)]
    let _test_call_lock = if !allow_active_pipeline && pipeline_execution_depth() == 0 {
        Some(TestFfiCallScope::enter())
    } else {
        None
    };
    if CAPABILITY_CALLBACK_DEPTH.with(|depth| depth.get() != 0) {
        return complete_call_result(
            result,
            Status::Backpressure,
            "PowerShell FFI calls are not permitted from a capability callback.",
        );
    }
    if !allow_active_pipeline {
        if let Err((status, diagnostic)) = reject_active_pipeline_ffi_call() {
            return complete_call_result(result, status, &diagnostic);
        }
    } else if pipeline_execution_depth() != 0 {
        return complete_call_result(
            result,
            Status::Backpressure,
            "PowerShell FFI calls are not permitted from code invoked by an active PowerShell pipeline.",
        );
    }
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(status)) => complete_call_result(result, status, ""),
        Ok(Err((status, diagnostic))) => complete_call_result(result, status, &diagnostic),
        Err(_) => complete_call_result(
            result,
            Status::Panic,
            "an unexpected native panic was contained by the PowerShell FFI",
        ),
    }
}

fn managed_failure(error: FfiBindingError) -> (Status, String) {
    let status = match error.status() {
        -8 => Status::InputNotCompleted,
        -9 => Status::Backpressure,
        -10 => Status::UnsupportedValue,
        -11 => Status::OperationCancelled,
        _ => Status::ManagedFailure,
    };
    (status, error.to_string())
}

fn bounded_operation_diagnostic(mut diagnostic: String) -> String {
    if diagnostic.len() <= MAX_OPERATION_DIAGNOSTIC_BYTES {
        return diagnostic;
    }

    let mut end = MAX_OPERATION_DIAGNOSTIC_BYTES;
    while !diagnostic.is_char_boundary(end) {
        end -= 1;
    }
    diagnostic.truncate(end);
    diagnostic
}

fn session_has_active_operation(session: &Session) -> bool {
    *session
        .operation_active
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn runspace_session_has_active_operation(session: &RunspaceSession) -> bool {
    *session
        .operation_active
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_session_result<F>(handle: u64, serialize_operation: bool, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiPowerShell) -> Result<Status, (Status, String)>,
{
    let session = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        match state.sessions.get(&handle) {
            Some(session) => Arc::clone(session),
            None => {
                return Err((Status::InvalidHandle, "PowerShell handle is invalid".to_owned()));
            }
        }
    };

    let _operation_lock = serialize_operation.then(|| {
        SESSION_OPERATION_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    });
    if session_has_active_operation(&session) {
        return Err((
            Status::Backpressure,
            "PowerShell builder has an active async operation; await or release it before mutating the builder."
                .to_owned(),
        ));
    }
    if let Some(runspace_session) = &session.runspace_session {
        if *runspace_session
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            return Err((
                Status::Backpressure,
                "PowerShell session has a pending or running async operation; await or release it before mutating a builder."
                    .to_owned(),
            ));
        }
    }
    operation(&session.power_shell)
}

fn invoke_result(handle: u64) -> Result<u64, (Status, String)> {
    let session = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        match state.sessions.get(&handle) {
            Some(session) => Arc::clone(session),
            None => {
                return Err((Status::InvalidHandle, "PowerShell handle is invalid".to_owned()));
            }
        }
    };
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if session_has_active_operation(&session) {
        return Err((
            Status::Backpressure,
            "PowerShell builder has an active async operation; await or release it before invoking again.".to_owned(),
        ));
    }
    if let Some(runspace_session) = &session.runspace_session {
        if *runspace_session
            .operation_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            return Err((
                Status::Backpressure,
                "PowerShell session has a pending or running async operation; await or release it before invoking again."
                    .to_owned(),
            ));
        }
    }
    reject_synchronous_broker_invocation(&session)?;
    let capability = take_session_capability(&session)?;
    let result = invoke_with_capability(&session, capability).map_err(managed_failure)?;
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let result_handle = state.next_result_handle;
    state.next_result_handle = state
        .next_result_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 63);
    state
        .results
        .insert(result_handle, Arc::new(InvocationResult { result }));
    Ok(result_handle)
}

fn run_operation(operation: Arc<Operation>) {
    if !operation.begin() {
        return;
    }

    if !operation.session.power_shell.supports_live_stream_polling() {
        run_legacy_operation(operation);
        return;
    }

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _execution_scope = InvocationExecutionScope::enter();
    let invocation = if operation.cancellation_requested() {
        None
    } else {
        Some(begin_live_invocation_with_capability(
            &operation.session,
            operation.capability.clone(),
        ))
    };

    if operation.cancellation_requested() {
        drop(invocation);
        operation.finish_capability();
        operation.complete(
            OperationState::Cancelled,
            Status::OperationCancelled,
            "PowerShell async operation was cancelled; no result is available.".to_owned(),
            None,
        );
        return;
    }

    match invocation {
        Some(Ok(live_invocation)) => {
            let mut source_cursor = 0_u64;
            let mut stream_failure = None;
            loop {
                if operation.cancellation_requested() {
                    let _ = live_invocation.stop();
                }

                match live_invocation.read_stream_batch(source_cursor, MAX_OPERATION_STREAM_RECORDS as u32) {
                    Ok(batch) => {
                        source_cursor = batch.next_sequence;
                        if let Err(error) = operation.capture_stream_batch(batch) {
                            stream_failure = Some(error);
                            let _ = live_invocation.stop();
                            break;
                        }
                    }
                    Err(error) => {
                        stream_failure = Some(managed_failure(error));
                        let _ = live_invocation.stop();
                        break;
                    }
                }

                match live_invocation.poll() {
                    Ok(true) => break,
                    Ok(false) => std::thread::sleep(Duration::from_millis(5)),
                    Err(error) => {
                        stream_failure = Some(managed_failure(error));
                        let _ = live_invocation.stop();
                        break;
                    }
                }
            }

            if stream_failure.is_none() {
                match live_invocation.read_stream_batch(source_cursor, MAX_OPERATION_STREAM_RECORDS as u32) {
                    Ok(batch) => {
                        source_cursor = batch.next_sequence;
                        if let Err(error) = operation.capture_stream_batch(batch) {
                            stream_failure = Some(error);
                        }
                    }
                    Err(error) => stream_failure = Some(managed_failure(error)),
                }
            }

            let completed = live_invocation.complete();
            if stream_failure.is_none() {
                match live_invocation.read_stream_batch(source_cursor, MAX_OPERATION_STREAM_RECORDS as u32) {
                    Ok(batch) => {
                        if let Err(error) = operation.capture_stream_batch(batch) {
                            stream_failure = Some(error);
                        }
                    }
                    Err(error) => stream_failure = Some(managed_failure(error)),
                }
            }
            let completion = if operation.cancellation_requested() {
                (
                    OperationState::Cancelled,
                    Status::OperationCancelled,
                    "PowerShell async operation was cancelled; no result is available.".to_owned(),
                    None,
                )
            } else if let Some((status, diagnostic)) = stream_failure {
                (OperationState::Failed, status, diagnostic, None)
            } else {
                match completed {
                    Ok(result) => (
                        OperationState::Completed,
                        Status::Success,
                        String::new(),
                        Some(Arc::new(InvocationResult { result })),
                    ),
                    Err(error) => {
                        let (status, diagnostic) = managed_failure(error);
                        (OperationState::Failed, status, diagnostic, None)
                    }
                }
            };
            drop(live_invocation);
            operation.complete(completion.0, completion.1, completion.2, completion.3);
        }
        Some(Err(error)) => {
            let (status, diagnostic) = managed_failure(error);
            operation.complete(OperationState::Failed, status, diagnostic, None);
        }
        None => operation.complete(
            OperationState::Cancelled,
            Status::OperationCancelled,
            "PowerShell async operation was cancelled before invocation started.".to_owned(),
            None,
        ),
    }
    operation.finish_capability();
}

fn run_legacy_operation(operation: Arc<Operation>) {
    let invocation = {
        let _operation_lock = SESSION_OPERATION_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if operation.cancellation_requested() {
            None
        } else {
            Some(invoke_with_capability(&operation.session, operation.capability.clone()))
        }
    };

    if operation.cancellation_requested() {
        operation.finish_capability();
        operation.complete(
            OperationState::Cancelled,
            Status::OperationCancelled,
            "PowerShell async operation was cancelled; no result is available.".to_owned(),
            None,
        );
        return;
    }

    match invocation {
        Some(Ok(result)) => operation.complete(
            OperationState::Completed,
            Status::Success,
            String::new(),
            Some(Arc::new(InvocationResult { result })),
        ),
        Some(Err(error)) => {
            let (status, diagnostic) = managed_failure(error);
            operation.complete(OperationState::Failed, status, diagnostic, None);
        }
        None => operation.complete(
            OperationState::Cancelled,
            Status::OperationCancelled,
            "PowerShell async operation was cancelled before invocation started.".to_owned(),
            None,
        ),
    }
    operation.finish_capability();
}

fn start_operation(handle: u64) -> Result<u64, (Status, String)> {
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (operation_handle, operation) = {
        let mut state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        {
            let mut active = session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *active {
                return Err((
                    Status::Backpressure,
                    "PowerShell builder already has an active async operation.".to_owned(),
                ));
            }
            *active = true;
        }
        if let Some(runspace_session) = &session.runspace_session {
            let parent_active = {
                let mut active = runspace_session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *active {
                    true
                } else {
                    *active = true;
                    false
                }
            };
            if parent_active {
                let mut builder_active = session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *builder_active = false;
                return Err((
                    Status::Backpressure,
                    "PowerShell session already has a pending or running async operation.".to_owned(),
                ));
            }
        }

        let operation_handle = state.next_operation_handle;
        state.next_operation_handle = state
            .next_operation_handle
            .checked_add(1)
            .filter(|value| *value != 0)
            .unwrap_or(1_u64 << 62);
        let capability = take_session_capability(&session)?;
        let broker = take_session_broker(&session);
        if let Err(error) = configure_broker(&session, broker.as_ref()) {
            // Match the sibling start paths: a failure here must not leave the
            // builder and its runspace session permanently backpressured.
            session.clear_operation_active();
            return Err(managed_failure(error));
        }
        let operation = Arc::new(Operation::new(handle, session, capability));
        state.operations.insert(operation_handle, Arc::clone(&operation));
        (operation_handle, operation)
    };

    if std::thread::Builder::new()
        .name("pwsh-sdk-ffi-operation".to_owned())
        .spawn({
            let operation = Arc::clone(&operation);
            move || run_operation(operation)
        })
        .is_err()
    {
        let mut state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.remove(&operation_handle);
        operation.clear_session_operation();
        return Err((
            Status::HostFailure,
            "failed to create the native PowerShell async operation thread".to_owned(),
        ));
    }

    Ok(operation_handle)
}

fn start_typed_result_operation(
    handle: u64,
    maximum_buffered_records: u32,
    maximum_page_records: u32,
) -> Result<u64, (Status, String)> {
    if maximum_buffered_records == 0
        || maximum_buffered_records as usize > MAX_TYPED_RESULT_RECORDS
        || maximum_page_records == 0
        || maximum_page_records > maximum_buffered_records
    {
        return Err((
            Status::InvalidArgument,
            "typed result invocation bounds are invalid".to_owned(),
        ));
    }

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (session, capability) = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        if !session.power_shell.supports_typed_result_paging() {
            return Err((
                Status::UnsupportedCapability,
                "The selected PowerShell payload does not support typed result paging.".to_owned(),
            ));
        }
        {
            let mut active = session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *active {
                return Err((
                    Status::Backpressure,
                    "PowerShell builder already has an active async operation.".to_owned(),
                ));
            }
            *active = true;
        }
        if let Some(runspace_session) = &session.runspace_session {
            let parent_active = {
                let mut active = runspace_session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *active {
                    true
                } else {
                    *active = true;
                    false
                }
            };
            if parent_active {
                let mut builder_active = session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *builder_active = false;
                return Err((
                    Status::Backpressure,
                    "PowerShell session already has a pending or running async operation.".to_owned(),
                ));
            }
        }
        let capability = match take_session_capability(&session) {
            Ok(capability) => capability,
            Err(error) => {
                session.clear_operation_active();
                return Err(error);
            }
        };
        let broker = take_session_broker(&session);
        if let Err(error) = configure_broker(&session, broker.as_ref()) {
            session.clear_operation_active();
            return Err(managed_failure(error));
        }
        (session, capability)
    };

    let invocation = begin_typed_result_invocation_with_capability(
        &session,
        capability.clone(),
        maximum_buffered_records,
        maximum_page_records,
    );
    let invocation = match invocation {
        Ok(invocation) => invocation,
        Err(error) => {
            let operation = TypedResultOperation {
                session,
                capability: None,
                invocation: Mutex::new(None),
                pipeline_completed: AtomicBool::new(false),
                cancellation_requested: AtomicBool::new(false),
                finalized: AtomicBool::new(false),
                worker_error: Mutex::new(None),
            };
            operation.clear_session_operation();
            return Err(managed_failure(error));
        }
    };

    let operation = Arc::new(TypedResultOperation::new(session, capability, invocation));
    let operation_handle = {
        let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let operation_handle = state.next_typed_result_operation_handle;
        state.next_typed_result_operation_handle = state
            .next_typed_result_operation_handle
            .checked_add(1)
            .filter(|value| *value != 0)
            .unwrap_or(1_u64 << 58);
        state
            .typed_result_operations
            .insert(operation_handle, Arc::clone(&operation));
        operation_handle
    };

    if std::thread::Builder::new()
        .name("pwsh-sdk-ffi-typed-result".to_owned())
        .spawn({
            let operation = Arc::clone(&operation);
            move || run_typed_result_operation(operation)
        })
        .is_err()
    {
        state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .typed_result_operations
            .remove(&operation_handle);
        operation.abort();
        return Err((
            Status::HostFailure,
            "failed to create the native typed result operation thread".to_owned(),
        ));
    }

    Ok(operation_handle)
}

fn start_observed_invocation_operation(
    handle: u64,
    maximum_buffered_result_records: u32,
    maximum_result_page_records: u32,
    maximum_buffered_diagnostic_records: u32,
    maximum_diagnostic_page_records: u32,
) -> Result<u64, (Status, String)> {
    if maximum_buffered_result_records == 0
        || maximum_buffered_result_records as usize > MAX_TYPED_RESULT_RECORDS
        || maximum_result_page_records == 0
        || maximum_result_page_records > maximum_buffered_result_records
        || maximum_buffered_diagnostic_records == 0
        || maximum_buffered_diagnostic_records as usize > MAX_TYPED_RESULT_RECORDS
        || maximum_diagnostic_page_records == 0
        || maximum_diagnostic_page_records > maximum_buffered_diagnostic_records
    {
        return Err((
            Status::InvalidArgument,
            "observed invocation bounds are invalid".to_owned(),
        ));
    }

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (session, capability) = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        if !session.power_shell.supports_observed_invocation() {
            return Err((
                Status::UnsupportedCapability,
                "The selected PowerShell payload does not support observed invocations.".to_owned(),
            ));
        }
        {
            let mut active = session
                .operation_active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *active {
                return Err((
                    Status::Backpressure,
                    "PowerShell builder already has an active async operation.".to_owned(),
                ));
            }
            *active = true;
        }
        if let Some(runspace_session) = &session.runspace_session {
            let parent_active = {
                let mut active = runspace_session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *active {
                    true
                } else {
                    *active = true;
                    false
                }
            };
            if parent_active {
                let mut builder_active = session
                    .operation_active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *builder_active = false;
                return Err((
                    Status::Backpressure,
                    "PowerShell session already has a pending or running async operation.".to_owned(),
                ));
            }
        }
        let capability = match take_session_capability(&session) {
            Ok(capability) => capability,
            Err(error) => {
                session.clear_operation_active();
                return Err(error);
            }
        };
        let broker = take_session_broker(&session);
        if let Err(error) = configure_broker(&session, broker.as_ref()) {
            session.clear_operation_active();
            return Err(managed_failure(error));
        }
        (session, capability)
    };

    let invocation = begin_observed_invocation_with_capability(
        &session,
        capability.clone(),
        maximum_buffered_result_records,
        maximum_result_page_records,
        maximum_buffered_diagnostic_records,
        maximum_diagnostic_page_records,
    );
    let invocation = match invocation {
        Ok(invocation) => invocation,
        Err(error) => {
            let operation = ObservedInvocationOperation {
                session,
                capability: None,
                invocation: Mutex::new(None),
                pipeline_completed: AtomicBool::new(false),
                cancellation_requested: AtomicBool::new(false),
                finalized: AtomicBool::new(false),
                worker_error: Mutex::new(None),
            };
            operation.clear_session_operation();
            return Err(managed_failure(error));
        }
    };

    let operation = Arc::new(ObservedInvocationOperation::new(session, capability, invocation));
    let operation_handle = {
        let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let operation_handle = state.next_observed_operation_handle;
        state.next_observed_operation_handle = state
            .next_observed_operation_handle
            .checked_add(1)
            .filter(|value| *value != 0)
            .unwrap_or(1_u64 << 56);
        state
            .observed_operations
            .insert(operation_handle, Arc::clone(&operation));
        operation_handle
    };

    if std::thread::Builder::new()
        .name("pwsh-sdk-ffi-observed-invocation".to_owned())
        .spawn({
            let operation = Arc::clone(&operation);
            move || run_observed_invocation_operation(operation)
        })
        .is_err()
    {
        state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observed_operations
            .remove(&operation_handle);
        operation.abort();
        return Err((
            Status::HostFailure,
            "failed to create the native observed invocation thread".to_owned(),
        ));
    }

    Ok(operation_handle)
}

fn run_typed_result_operation(operation: Arc<TypedResultOperation>) {
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _execution_scope = InvocationExecutionScope::enter();
    loop {
        if operation.cancellation_requested.load(Ordering::Acquire) {
            operation.request_stop();
        }

        match operation.poll_pipeline() {
            Ok(true) => {
                operation.pipeline_completed.store(true, Ordering::Release);
                break;
            }
            Ok(false) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                operation.record_worker_error(error);
                break;
            }
        }
    }
}

fn run_observed_invocation_operation(operation: Arc<ObservedInvocationOperation>) {
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _execution_scope = InvocationExecutionScope::enter();
    loop {
        if operation.cancellation_requested.load(Ordering::Acquire) {
            operation.request_stop();
        }

        match operation.poll_pipeline() {
            Ok(true) => {
                if let Err(error) = operation.complete_pipeline() {
                    operation.record_worker_error(error);
                } else {
                    operation.pipeline_completed.store(true, Ordering::Release);
                }
                break;
            }
            Ok(false) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                operation.record_worker_error(error);
                break;
            }
        }
    }
}

fn with_typed_result_operation<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&Arc<TypedResultOperation>) -> Result<Status, (Status, String)>,
{
    let operation_handle = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.typed_result_operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "typed result invocation handle is invalid".to_owned(),
            )
        })?
    };
    operation(&operation_handle)
}

fn read_typed_result_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
) -> Result<u64, (Status, String)> {
    let operation = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.typed_result_operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "typed result invocation handle is invalid".to_owned(),
            )
        })?
    };
    let page = Arc::new(operation.read_page(acknowledged_through, maximum_records)?);
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let page_handle = state.next_typed_result_page_handle;
    state.next_typed_result_page_handle = state
        .next_typed_result_page_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 57);
    state.typed_result_pages.insert(page_handle, page);
    Ok(page_handle)
}

fn release_typed_result_operation(handle: u64) -> Result<Status, (Status, String)> {
    let operation = {
        let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.typed_result_operations.remove(&handle).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "typed result invocation handle is invalid".to_owned(),
            )
        })?
    };
    operation.abort();
    Ok(Status::Success)
}

fn with_typed_result_page<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiTypedResultPage) -> Result<Status, (Status, String)>,
{
    let page = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .typed_result_pages
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "typed result page handle is invalid".to_owned()))?
    };
    operation(&page)
}

fn release_typed_result_page(handle: u64) -> Result<Status, (Status, String)> {
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .typed_result_pages
        .remove(&handle)
        .map(|_| Status::Success)
        .ok_or_else(|| (Status::InvalidHandle, "typed result page handle is invalid".to_owned()))
}

fn with_observed_operation<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&Arc<ObservedInvocationOperation>) -> Result<Status, (Status, String)>,
{
    let operation_handle = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observed_operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed invocation handle is invalid".to_owned(),
            )
        })?
    };
    operation(&operation_handle)
}

fn read_observed_result_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
) -> Result<u64, (Status, String)> {
    let operation = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observed_operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed invocation handle is invalid".to_owned(),
            )
        })?
    };
    let page = Arc::new(operation.read_result_page(acknowledged_through, maximum_records)?);
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let page_handle = state.next_typed_result_page_handle;
    state.next_typed_result_page_handle = state
        .next_typed_result_page_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 57);
    state.typed_result_pages.insert(page_handle, page);
    Ok(page_handle)
}

fn read_observed_diagnostic_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
) -> Result<u64, (Status, String)> {
    let operation = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observed_operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed invocation handle is invalid".to_owned(),
            )
        })?
    };
    let page = Arc::new(operation.read_diagnostic_page(acknowledged_through, maximum_records)?);
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let page_handle = state.next_observed_diagnostic_page_handle;
    state.next_observed_diagnostic_page_handle = state
        .next_observed_diagnostic_page_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 55);
    state.observed_diagnostic_pages.insert(page_handle, page);
    Ok(page_handle)
}

fn release_observed_operation(handle: u64) -> Result<Status, (Status, String)> {
    let operation = {
        let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observed_operations.remove(&handle).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed invocation handle is invalid".to_owned(),
            )
        })?
    };
    operation.abort();
    Ok(Status::Success)
}

fn with_observed_diagnostic_page<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiObservedDiagnosticPage) -> Result<Status, (Status, String)>,
{
    let page = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.observed_diagnostic_pages.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed diagnostic page handle is invalid".to_owned(),
            )
        })?
    };
    operation(&page)
}

fn release_observed_diagnostic_page(handle: u64) -> Result<Status, (Status, String)> {
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .observed_diagnostic_pages
        .remove(&handle)
        .map(|_| Status::Success)
        .ok_or_else(|| {
            (
                Status::InvalidHandle,
                "observed diagnostic page handle is invalid".to_owned(),
            )
        })
}

fn with_operation<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&Arc<Operation>) -> Result<Status, (Status, String)>,
{
    let operation_handle = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation handle is invalid".to_owned(),
            )
        })?
    };
    operation(&operation_handle)
}

fn write_operation_state(
    state_output: *mut u32,
    terminal_status_output: *mut i32,
    snapshot: (OperationState, Status, String, Option<Arc<InvocationResult>>),
) -> Result<Status, (Status, String)> {
    if state_output.is_null() || terminal_status_output.is_null() {
        return Err((
            Status::InvalidArgument,
            "PowerShell operation state output pointer is null".to_owned(),
        ));
    }

    let (state, terminal_status, diagnostic, _) = snapshot;
    unsafe {
        *state_output = state as u32;
        *terminal_status_output = terminal_status.value();
    }
    if state.is_terminal() && terminal_status != Status::Success {
        return Err((terminal_status, diagnostic));
    }
    Ok(Status::Success)
}

fn poll_operation(
    handle: u64,
    state_output: *mut u32,
    terminal_status_output: *mut i32,
) -> Result<Status, (Status, String)> {
    with_operation(handle, |operation| {
        write_operation_state(state_output, terminal_status_output, operation.snapshot())
    })
}

fn wait_operation(
    handle: u64,
    timeout_milliseconds: u32,
    state_output: *mut u32,
    terminal_status_output: *mut i32,
) -> Result<Status, (Status, String)> {
    with_operation(handle, |operation| {
        let timeout = if timeout_milliseconds == u32::MAX {
            None
        } else {
            Some(Duration::from_millis(u64::from(timeout_milliseconds)))
        };
        write_operation_state(state_output, terminal_status_output, operation.wait(timeout))
    })
}

fn stop_operation(handle: u64) -> Result<Status, (Status, String)> {
    with_operation(handle, |operation| {
        operation.request_stop();
        Ok(Status::Success)
    })
}

fn release_operation(handle: u64) -> Result<Status, (Status, String)> {
    let operation = {
        let mut state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.remove(&handle).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation handle is invalid".to_owned(),
            )
        })?
    };
    operation.request_stop();
    Ok(Status::Success)
}

fn operation_result(handle: u64) -> Result<u64, (Status, String)> {
    let operation = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation handle is invalid".to_owned(),
            )
        })?
    };
    let (operation_state, terminal_status, diagnostic, invocation_result) = operation.snapshot();
    if !operation_state.is_terminal() {
        return Err((
            Status::OperationNotTerminal,
            "PowerShell operation has not reached a terminal state.".to_owned(),
        ));
    }
    if terminal_status != Status::Success {
        return Err((terminal_status, diagnostic));
    }
    let invocation_result = invocation_result.ok_or_else(|| {
        (
            Status::ManagedFailure,
            "completed PowerShell operation has no immutable result snapshot".to_owned(),
        )
    })?;

    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let result_handle = state.next_result_handle;
    state.next_result_handle = state
        .next_result_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 63);
    state.results.insert(result_handle, invocation_result);
    Ok(result_handle)
}

fn read_operation_stream_batch(
    handle: u64,
    after_sequence: u64,
    maximum_records: u32,
) -> Result<u64, (Status, String)> {
    let operation = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.operations.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation handle is invalid".to_owned(),
            )
        })?
    };
    let batch = Arc::new(operation.stream_batch(after_sequence, maximum_records)?);
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let batch_handle = state.next_operation_stream_batch_handle;
    state.next_operation_stream_batch_handle = state
        .next_operation_stream_batch_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 59);
    state.operation_stream_batches.insert(batch_handle, batch);
    Ok(batch_handle)
}

fn with_operation_stream_batch<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&OperationStreamBatch) -> Result<Status, (Status, String)>,
{
    let batch = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.operation_stream_batches.get(&handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation stream batch handle is invalid".to_owned(),
            )
        })?
    };
    operation(&batch)
}

fn release_operation_stream_batch(handle: u64) -> Result<Status, (Status, String)> {
    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .operation_stream_batches
        .remove(&handle)
        .map(|_| Status::Success)
        .ok_or_else(|| {
            (
                Status::InvalidHandle,
                "PowerShell operation stream batch handle is invalid".to_owned(),
            )
        })
}

fn write_operation_stream_batch_info(
    batch: &OperationStreamBatch,
    info: *mut OperationStreamBatchInfo,
) -> Result<Status, (Status, String)> {
    if info.is_null() || unsafe { (*info).size } < mem::size_of::<OperationStreamBatchInfo>() as u32 {
        return Err((
            Status::InvalidArgument,
            "PowerShell operation stream batch info output is null or too small".to_owned(),
        ));
    }
    let mut flags = 0_u32;
    if batch.lost_record_count != 0 {
        flags |= 1;
    }
    if batch.dropped_record_count != 0
        || batch.source_dropped_record_count != 0
        || batch
            .records
            .iter()
            .any(|record| record.flags & OPERATION_STREAM_RECORD_TEXT_TRUNCATED != 0)
    {
        flags |= 1 << 1;
    }
    unsafe {
        (*info).operation_state = batch.state as u32;
        (*info).terminal_status = batch.terminal_status.value();
        (*info).flags = flags;
        (*info).next_sequence = batch.next_sequence;
        (*info).total_record_count = batch.total_record_count;
        (*info).dropped_record_count = batch.dropped_record_count;
        (*info).source_dropped_record_count = batch.source_dropped_record_count;
        (*info).lost_record_count = batch.lost_record_count;
        (*info).record_count = u32::try_from(batch.records.len()).map_err(|_| {
            (
                Status::ManagedFailure,
                "PowerShell operation stream batch exceeds its fixed record bound".to_owned(),
            )
        })?;
        (*info)._reserved = 0;
    }
    Ok(Status::Success)
}

fn operation_stream_batch_record(
    batch: &OperationStreamBatch,
    record_index: u32,
) -> Result<&OperationStreamRecord, (Status, String)> {
    batch.records.get(record_index as usize).ok_or_else(|| {
        (
            Status::InvalidArgument,
            "PowerShell operation stream batch record index is invalid".to_owned(),
        )
    })
}

fn typed_result_page_record(
    page: &FfiTypedResultPage,
    record_index: u32,
) -> Result<&FfiTypedResultRecord, (Status, String)> {
    page.records.get(record_index as usize).ok_or_else(|| {
        (
            Status::InvalidArgument,
            "typed result page record index is invalid".to_owned(),
        )
    })
}

fn stop_session_operation(handle: u64) -> Result<Status, (Status, String)> {
    let (session, operation) = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        let operation = state
            .operations
            .values()
            .find(|operation| operation.builder_handle == handle && !operation.snapshot().0.is_terminal())
            .cloned();
        (session, operation)
    };

    if let Some(operation) = operation {
        operation.request_stop();
        return Ok(Status::Success);
    }

    cancel_active_capability(&session);
    session
        .power_shell
        .stop()
        .map(|_| Status::Success)
        .map_err(managed_failure)
}

fn release_result(handle: u64) -> Result<Status, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state
        .results
        .remove(&handle)
        .map(|_| Status::Success)
        .ok_or_else(|| (Status::InvalidHandle, "Invocation result handle is invalid".to_owned()))
}

unsafe fn register_capabilities(registration: *const CapabilityRegistration) -> Result<u64, (Status, String)> {
    if registration.is_null() {
        return Err((
            Status::InvalidArgument,
            "capability registration pointer is null".to_owned(),
        ));
    }
    let registration = &*registration;
    if registration.size < mem::size_of::<CapabilityRegistration>() as u32
        || registration.flags != CAPABILITY_REGISTRATION_VERSION
    {
        return Err((
            Status::InvalidArgument,
            "capability registration header is invalid".to_owned(),
        ));
    }
    let dispatch = registration.dispatch.ok_or_else(|| {
        (
            Status::InvalidArgument,
            "capability registration dispatch callback is null".to_owned(),
        )
    })?;
    let cancel = registration.cancel.ok_or_else(|| {
        (
            Status::InvalidArgument,
            "capability registration cancellation callback is null".to_owned(),
        )
    })?;
    let (kind, payload) = data_value_input(registration.definitions)?;
    let definitions = parse_capability_definitions(kind, payload)?;

    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.runtime.is_none() {
        return Err((
            Status::NotInitialized,
            "PowerShell runtime is not initialized".to_owned(),
        ));
    }
    let handle = state.next_capability_handle;
    state.next_capability_handle = state
        .next_capability_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 60);
    state.capabilities.insert(
        handle,
        Arc::new(CapabilityRegistrationState {
            handle,
            definitions,
            dispatch,
            cancel,
            active: AtomicBool::new(true),
            invocations: Mutex::new(HashMap::new()),
        }),
    );
    Ok(handle)
}

fn release_capabilities(handle: u64) -> Result<Status, (Status, String)> {
    let registration = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .capabilities
        .remove(&handle)
        .ok_or_else(|| {
            (
                Status::InvalidHandle,
                "capability registration handle is invalid".to_owned(),
            )
        })?;
    registration.revoke();
    Ok(Status::Success)
}

fn set_capabilities(handle: u64, capability_handle: u64) -> Result<Status, (Status, String)> {
    let (session, registration) = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        let registration = state.capabilities.get(&capability_handle).cloned().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "capability registration handle is invalid".to_owned(),
            )
        })?;
        (session, registration)
    };
    if !registration.active.load(Ordering::Acquire) {
        return Err((
            Status::InvalidHandle,
            "capability registration has been released".to_owned(),
        ));
    }
    if session_has_active_operation(&session)
        || session
            .runspace_session
            .as_ref()
            .is_some_and(|runspace| runspace_session_has_active_operation(runspace))
    {
        return Err((
            Status::Backpressure,
            "capability context cannot change while its session has an active invocation".to_owned(),
        ));
    }
    let mut attached = session
        .capability_registration
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if attached.is_some() {
        return Err((
            Status::Backpressure,
            "a capability context is already attached; invoke or clear the builder first".to_owned(),
        ));
    }
    *attached = Some(registration);
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------
// Duplex Broker Channel
//
// Lock discipline (never nested):
//   STATE                   -> nothing
//   BROKER_FRAME_REGISTRY   -> nothing
//   BrokerChannel::inner    -> nothing
//
// A blocked payload raiser waits only on its channel's own condvar. It clones
// the channel Arc under STATE, drops STATE, and never touches
// SESSION_OPERATION_LOCK, which is held across pipeline execution.
// ---------------------------------------------------------------------------

static NEXT_BROKER_FRAME_HANDLE: AtomicU64 = AtomicU64::new(1_u64 << 53);
static NEXT_BROKER_OWNER_TOKEN: AtomicU64 = AtomicU64::new(1);
static NEXT_BROKER_GENERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_BROKER_OBSERVATION_HANDLE: AtomicU64 = AtomicU64::new(1_u64 << 54);

/// One delivery of a frame to one pump thread. The owner token is recorded
/// here, not on the frame, so releasing a delivery handle still succeeds after
/// its request has already reached a terminal state and been removed.
struct BrokerDelivery {
    channel: Arc<BrokerChannel>,
    correlation: u64,
    owner_token: u64,
}

/// Delivery handle -> delivery record.
type BrokerFrameRegistry = Option<HashMap<u64, BrokerDelivery>>;
struct BrokerObservation {
    channel: Arc<BrokerChannel>,
    completion: Arc<BrokerTerminalObservation>,
}
/// Correlation terminal observation handle -> copied completion cell.
type BrokerObservationRegistry = Option<HashMap<u64, BrokerObservation>>;
/// Generation -> the attachment that owns it.
type BrokerAttachmentRegistry = Option<HashMap<u64, Arc<BrokerAttachment>>>;

static BROKER_FRAME_REGISTRY: Mutex<BrokerFrameRegistry> = Mutex::new(None);
static BROKER_OBSERVATION_REGISTRY: Mutex<BrokerObservationRegistry> = Mutex::new(None);
static BROKER_ATTACHMENTS: Mutex<BrokerAttachmentRegistry> = Mutex::new(None);

thread_local! {
    /// Owner tokens for delivery handles currently held by this thread. While
    /// this is non-empty, every non-broker FFI export fails with
    /// `BrokerDispatchViolation`.
    static BROKER_HELD_TOKENS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn broker_frame_registry() -> MutexGuard<'static, BrokerFrameRegistry> {
    BROKER_FRAME_REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn broker_observation_registry() -> MutexGuard<'static, BrokerObservationRegistry> {
    BROKER_OBSERVATION_REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn broker_holds_delivery_handle() -> bool {
    BROKER_HELD_TOKENS.with(|tokens| !tokens.borrow().is_empty())
}

fn broker_push_owner_token(token: u64) {
    BROKER_HELD_TOKENS.with(|tokens| tokens.borrow_mut().push(token));
}

fn broker_take_owner_token(token: u64) -> bool {
    BROKER_HELD_TOKENS.with(|tokens| {
        let mut tokens = tokens.borrow_mut();
        match tokens.iter().position(|candidate| *candidate == token) {
            Some(index) => {
                tokens.remove(index);
                true
            }
            None => false,
        }
    })
}

fn broker_terminal_status(state: u32) -> Status {
    match state {
        BROKER_FRAME_STATE_COMPLETED => Status::Success,
        BROKER_FRAME_STATE_FAILED => Status::ManagedFailure,
        BROKER_FRAME_STATE_CANCELLED => Status::OperationCancelled,
        BROKER_FRAME_STATE_TIMED_OUT => Status::BrokerTimeout,
        _ => Status::BrokerClosed,
    }
}

/// Counts every non-terminal correlation, so releasing a delivery handle does
/// not free an inflight slot.
fn broker_inflight_count(inner: &BrokerChannelInner) -> u32 {
    inner
        .frames
        .values()
        .filter(|frame| !frame.is_terminal())
        .count()
        .min(u32::MAX as usize) as u32
}

fn broker_has_active_mutating_key(inner: &BrokerChannelInner, ordering_key: u64) -> bool {
    inner.frames.values().any(|frame| {
        !frame.is_terminal() && frame.flags & BROKER_FRAME_FLAG_MUTATING != 0 && frame.ordering_key == ordering_key
    })
}

/// Applies a terminal transition exactly once and wakes every terminal observer.
fn broker_finish_frame(inner: &mut BrokerChannelInner, correlation: u64, state: u32, terminal_epoch_ms: u64) -> bool {
    let Some(frame) = inner.frames.get_mut(&correlation) else {
        return false;
    };
    if frame.is_terminal() {
        return false;
    }
    frame.state = state;
    frame.terminal_observation.complete(state, terminal_epoch_ms);
    true
}

fn broker_expire_due_frames(inner: &mut BrokerChannelInner, now_ms: u64) -> bool {
    let expired: Vec<u64> = inner
        .frames
        .values()
        .filter(|frame| !frame.is_terminal() && frame.deadline_epoch_ms <= now_ms)
        .map(|frame| frame.correlation_id)
        .collect();
    let changed = !expired.is_empty();
    for correlation in expired {
        broker_finish_frame(inner, correlation, BROKER_FRAME_STATE_TIMED_OUT, now_ms);
        inner.queue.retain(|queued| *queued != correlation);
    }
    // A one-way frame has no blocked raiser to collect it, so an expired event
    // that no consumer ever received would otherwise be retained forever.
    // Only drop the ones nobody currently holds a delivery handle for.
    inner
        .frames
        .retain(|_, frame| !(frame.is_terminal() && frame.is_one_way() && frame.owner_token == 0));
    changed
}

/// Terminates only the frames created by one invocation attachment. A channel
/// may be shared by independent sessions, so its remaining frames must not be
/// affected when another attachment stops or completes.
fn broker_finish_attachment_frames(attachment: &BrokerAttachment, state: u32) {
    let mut inner = attachment.channel.lock();
    broker_finish_attachment_frames_locked(attachment, &mut inner, state);
    drop(inner);
    attachment.channel.payload_signal.notify_all();
    attachment.channel.consumer_signal.notify_all();
}

fn broker_finish_attachment_frames_locked(attachment: &BrokerAttachment, inner: &mut BrokerChannelInner, state: u32) {
    let now_ms = attachment.channel.now_epoch_ms();
    let correlations: Vec<u64> = inner
        .frames
        .values()
        .filter(|frame| {
            !frame.is_terminal()
                && frame.generation == attachment.generation
                // A sink-validated bridge Close is teardown metadata, not
                // application work. Preserve one admitted before cancellation
                // as well as one admitted after the atomic cancellation sweep.
                && (state != BROKER_FRAME_STATE_CANCELLED || !is_bridge_close_request(frame.kind, &frame.body))
        })
        .map(|frame| frame.correlation_id)
        .collect();
    for correlation in correlations {
        broker_finish_frame(inner, correlation, state, now_ms);
        inner.queue.retain(|queued| *queued != correlation);
    }
}

fn broker_channel_for(handle: u64) -> Result<Arc<BrokerChannel>, (Status, String)> {
    let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .broker_channels
        .get(&handle)
        .cloned()
        .ok_or_else(|| (Status::InvalidHandle, "broker channel handle is invalid".to_owned()))
}

unsafe fn broker_open(
    options: *const BrokerChannelOptions,
    channel_handle: *mut u64,
) -> Result<Status, (Status, String)> {
    if channel_handle.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker channel output pointer is null".to_owned(),
        ));
    }
    if options.is_null() {
        return Err((Status::InvalidArgument, "broker channel options are null".to_owned()));
    }
    let size = std::ptr::read_unaligned(std::ptr::addr_of!((*options).size));
    if size as usize != mem::size_of::<BrokerChannelOptions>() {
        return Err((
            Status::InvalidArgument,
            "broker channel options report an unsupported size or ABI version".to_owned(),
        ));
    }
    let abi_version = std::ptr::read_unaligned(std::ptr::addr_of!((*options).abi_version));
    if abi_version != BROKER_ABI_V1 {
        return Err((
            Status::InvalidArgument,
            "broker channel options report an unsupported size or ABI version".to_owned(),
        ));
    }
    let header = options.read_unaligned();
    if header.flags != 0 {
        return Err((
            Status::InvalidArgument,
            "broker channel options reserved flags must be zero".to_owned(),
        ));
    }
    if header.max_inflight == 0 || header.max_inflight > BROKER_MAX_INFLIGHT_LIMIT {
        return Err((
            Status::InvalidArgument,
            "broker channel maximum inflight frames must be between 1 and 32".to_owned(),
        ));
    }
    if header.max_body_bytes == 0 || header.max_body_bytes > BROKER_MAX_BODY_BYTES_LIMIT {
        return Err((
            Status::InvalidArgument,
            "broker channel maximum body bytes must be between 1 and 65536".to_owned(),
        ));
    }
    if header.default_deadline_ms == 0 || header.default_deadline_ms > BROKER_MAX_DEADLINE_MS {
        return Err((
            Status::InvalidArgument,
            "broker channel default deadline must be between 1 and 30000 milliseconds".to_owned(),
        ));
    }

    let channel = Arc::new(BrokerChannel {
        epoch: Instant::now(),
        max_inflight: header.max_inflight,
        max_body_bytes: header.max_body_bytes,
        default_deadline_ms: header.default_deadline_ms,
        observer_leases: AtomicU32::new(0),
        inner: Mutex::new(BrokerChannelInner {
            next_correlation: 1,
            ..BrokerChannelInner::default()
        }),
        payload_signal: Condvar::new(),
        consumer_signal: Condvar::new(),
    });

    let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let handle = state.next_broker_channel_handle;
    state.next_broker_channel_handle = state
        .next_broker_channel_handle
        .checked_add(1)
        .ok_or_else(|| (Status::HostFailure, "broker channel handles are exhausted".to_owned()))?;
    state.broker_channels.insert(handle, channel);
    drop(state);
    *channel_handle = handle;
    Ok(Status::Success)
}

fn broker_close(handle: u64) -> Result<Status, (Status, String)> {
    let channel = {
        let mut state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .broker_channels
            .remove(&handle)
            .ok_or_else(|| (Status::InvalidHandle, "broker channel handle is invalid".to_owned()))?
    };

    {
        let mut inner = channel.lock();
        inner.closed = true;
        let outstanding: Vec<u64> = inner
            .frames
            .values()
            .filter(|frame| !frame.is_terminal())
            .map(|frame| frame.correlation_id)
            .collect();
        for correlation in outstanding {
            broker_finish_frame(
                &mut inner,
                correlation,
                BROKER_FRAME_STATE_ABORTED,
                channel.now_epoch_ms(),
            );
        }
        inner.queue.clear();
    }
    channel.payload_signal.notify_all();
    channel.consumer_signal.notify_all();

    // Reclaim any delivery handles whose owning thread never released them.
    let mut registry = broker_frame_registry();
    if let Some(map) = registry.as_mut() {
        map.retain(|_, entry| !Arc::ptr_eq(&entry.channel, &channel));
    }
    Ok(Status::Success)
}

fn broker_wait(handle: u64, timeout_ms: u32, frame_handle: *mut u64) -> Result<Status, (Status, String)> {
    if frame_handle.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker frame output pointer is null".to_owned(),
        ));
    }
    let channel = broker_channel_for(handle)?;
    // SAFETY: validated non-null above; the caller owns the destination.
    unsafe { *frame_handle = 0 };

    let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
    let mut inner = channel.lock();
    inner.consumer_attached = true;
    loop {
        if inner.closed {
            return Err((Status::BrokerClosed, "broker channel is closed".to_owned()));
        }
        let now_ms = channel.now_epoch_ms();
        if broker_expire_due_frames(&mut inner, now_ms) {
            channel.payload_signal.notify_all();
        }

        // Dispatch the first queued frame whose ordering key is free.
        let mut selected = None;
        for (index, correlation) in inner.queue.iter().enumerate() {
            let Some(frame) = inner.frames.get(correlation) else {
                continue;
            };
            if frame.is_terminal() {
                continue;
            }
            let blocked = frame.flags & BROKER_FRAME_FLAG_MUTATING != 0
                && inner.frames.values().any(|other| {
                    other.correlation_id != frame.correlation_id
                        && !other.is_terminal()
                        && other.state == BROKER_FRAME_STATE_DISPATCHED
                        && other.flags & BROKER_FRAME_FLAG_MUTATING != 0
                        && other.ordering_key == frame.ordering_key
                });
            if !blocked {
                selected = Some((index, *correlation));
                break;
            }
        }

        if let Some((index, correlation)) = selected {
            inner.queue.remove(index);
            let dropped_before = inner.pending_drops;
            inner.pending_drops = 0;
            let token = NEXT_BROKER_OWNER_TOKEN.fetch_add(1, Ordering::AcqRel);
            let delivery = NEXT_BROKER_FRAME_HANDLE.fetch_add(1, Ordering::AcqRel);
            if let Some(frame) = inner.frames.get_mut(&correlation) {
                frame.state = BROKER_FRAME_STATE_DISPATCHED;
                frame.terminal_observation.mark_dispatched();
                frame.owner_token = token;
                frame.dropped_before = dropped_before;
            }
            drop(inner);
            broker_push_owner_token(token);
            let mut registry = broker_frame_registry();
            registry.get_or_insert_with(HashMap::new).insert(
                delivery,
                BrokerDelivery {
                    channel: Arc::clone(&channel),
                    correlation,
                    owner_token: token,
                },
            );
            drop(registry);
            // SAFETY: validated non-null above.
            unsafe { *frame_handle = delivery };
            return Ok(Status::Success);
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(Status::Success);
        }
        let (guard, _) = channel
            .consumer_signal
            .wait_timeout(inner, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner = guard;
    }
}

fn broker_resolve_frame(delivery: u64) -> Result<(Arc<BrokerChannel>, u64), (Status, String)> {
    let registry = broker_frame_registry();
    registry
        .as_ref()
        .and_then(|map| map.get(&delivery))
        .map(|entry| (Arc::clone(&entry.channel), entry.correlation))
        .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))
}

unsafe fn broker_frame_get_info(delivery: u64, info: *mut BrokerFrameInfo) -> Result<Status, (Status, String)> {
    if info.is_null() {
        return Err((Status::InvalidArgument, "broker frame info pointer is null".to_owned()));
    }
    let size = std::ptr::read_unaligned(std::ptr::addr_of!((*info).size));
    if size as usize != mem::size_of::<BrokerFrameInfo>() {
        return Err((
            Status::InvalidArgument,
            "broker frame info reports an unsupported size or ABI version".to_owned(),
        ));
    }
    let abi_version = std::ptr::read_unaligned(std::ptr::addr_of!((*info).abi_version));
    if abi_version != BROKER_ABI_V1 {
        return Err((
            Status::InvalidArgument,
            "broker frame info reports an unsupported size or ABI version".to_owned(),
        ));
    }
    let (channel, correlation) = broker_resolve_frame(delivery)?;
    let inner = channel.lock();
    let frame = inner
        .frames
        .get(&correlation)
        .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))?;
    let now_ms = channel.now_epoch_ms();
    info.write(BrokerFrameInfo {
        size: mem::size_of::<BrokerFrameInfo>() as u32,
        abi_version: BROKER_ABI_V1,
        correlation_id: frame.correlation_id,
        ordering_key: frame.ordering_key,
        deadline_epoch_ms: frame.deadline_epoch_ms,
        remaining_ms: frame.deadline_epoch_ms.saturating_sub(now_ms).min(u64::from(u32::MAX)) as u32,
        kind: frame.kind,
        flags: frame.flags,
        body_length: frame.body.len() as u32,
        state: frame.state,
        dropped_before: frame.dropped_before,
    });
    Ok(Status::Success)
}

unsafe fn broker_frame_read(
    delivery: u64,
    buffer: *mut u8,
    capacity: u32,
    required: *mut u32,
) -> Result<Status, (Status, String)> {
    if required.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker frame required-length pointer is null".to_owned(),
        ));
    }
    let (channel, correlation) = broker_resolve_frame(delivery)?;
    let inner = channel.lock();
    let frame = inner
        .frames
        .get(&correlation)
        .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))?;
    let length = frame.body.len() as u32;
    *required = length;
    if buffer.is_null() || capacity < length {
        return Ok(Status::BufferTooSmall);
    }
    if length > 0 {
        std::ptr::copy_nonoverlapping(frame.body.as_ptr(), buffer, length as usize);
    }
    Ok(Status::Success)
}

/// Releases only the readable delivery handle. This is deliberately **not** a
/// state transition: the correlation stays outstanding so the consumer can
/// reply later from any thread. It must also succeed after the request has
/// already terminated, otherwise the receiving thread's owner token leaks and
/// that thread stays blocked from every non-broker call.
fn broker_frame_release(delivery: u64) -> Result<Status, (Status, String)> {
    let (channel, correlation) = {
        let mut registry = broker_frame_registry();
        let map = registry
            .as_mut()
            .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))?;
        let owner_token = map
            .get(&delivery)
            .map(|entry| entry.owner_token)
            .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))?;
        // Ownership is proven by the receiving thread's token, never by the
        // frame, which may already be gone.
        if !broker_take_owner_token(owner_token) {
            return Err((
                Status::BrokerDispatchViolation,
                "a broker frame must be released by the thread that received it".to_owned(),
            ));
        }
        let entry = map
            .remove(&delivery)
            .ok_or_else(|| (Status::InvalidHandle, "broker frame handle is invalid".to_owned()))?;
        (entry.channel, entry.correlation)
    };
    let mut inner = channel.lock();
    if let Some(frame) = inner.frames.get_mut(&correlation) {
        frame.owner_token = 0;
        if frame.is_one_way() && !frame.is_terminal() {
            frame.state = BROKER_FRAME_STATE_COMPLETED;
            frame
                .terminal_observation
                .complete(BROKER_FRAME_STATE_COMPLETED, channel.now_epoch_ms());
        }
    }
    inner
        .frames
        .retain(|_, frame| !(frame.is_terminal() && frame.is_one_way()));
    Ok(Status::Success)
}

fn broker_observe(
    channel_handle: u64,
    correlation: u64,
    observation_handle: *mut u64,
) -> Result<Status, (Status, String)> {
    if observation_handle.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker terminal observation output pointer is null".to_owned(),
        ));
    }

    let channel = broker_channel_for(channel_handle)?;
    let observation = {
        let inner = channel.lock();
        let frame = inner.frames.get(&correlation).ok_or_else(|| {
            (
                Status::BrokerInvalidTerminalState,
                "broker correlation identifier is unknown".to_owned(),
            )
        })?;
        if frame.is_one_way() {
            return Err((
                Status::InvalidArgument,
                "one-way broker frames do not have terminal observations".to_owned(),
            ));
        }
        Arc::clone(&frame.terminal_observation)
    };
    if !observation.try_acquire() {
        return Err((
            Status::Backpressure,
            "the broker correlation has reached its terminal observation limit".to_owned(),
        ));
    }
    let maximum_leases = channel.max_inflight.saturating_mul(BROKER_MAX_OBSERVERS_PER_FRAME);
    let mut leased = channel.observer_leases.load(Ordering::Acquire);
    loop {
        if leased >= maximum_leases {
            observation.release();
            return Err((
                Status::Backpressure,
                "the broker channel has reached its terminal observation limit".to_owned(),
            ));
        }
        match channel
            .observer_leases
            .compare_exchange_weak(leased, leased + 1, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => break,
            Err(next) => leased = next,
        }
    }

    let handle = NEXT_BROKER_OBSERVATION_HANDLE.fetch_add(1, Ordering::AcqRel);
    if handle == 0 {
        observation.release();
        let _ = channel.observer_leases.fetch_sub(1, Ordering::AcqRel);
        return Err((
            Status::HostFailure,
            "broker terminal observation handles are exhausted".to_owned(),
        ));
    }
    let mut registry = broker_observation_registry();
    registry.get_or_insert_with(HashMap::new).insert(
        handle,
        BrokerObservation {
            channel,
            completion: observation,
        },
    );
    // SAFETY: the pointer was checked above and remains caller-owned.
    unsafe { *observation_handle = handle };
    Ok(Status::Success)
}

fn broker_observation_for(handle: u64) -> Result<Arc<BrokerTerminalObservation>, (Status, String)> {
    let registry = broker_observation_registry();
    registry
        .as_ref()
        .and_then(|observations| observations.get(&handle))
        .map(|observation| Arc::clone(&observation.completion))
        .ok_or_else(|| {
            (
                Status::InvalidHandle,
                "broker terminal observation handle is invalid".to_owned(),
            )
        })
}

unsafe fn write_broker_terminal_info(
    info: *mut BrokerTerminalInfo,
    state: BrokerTerminalState,
) -> Result<Status, (Status, String)> {
    if info.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker terminal info pointer is null".to_owned(),
        ));
    }
    let size = std::ptr::read_unaligned(std::ptr::addr_of!((*info).size));
    let abi_version = std::ptr::read_unaligned(std::ptr::addr_of!((*info).abi_version));
    if size as usize != mem::size_of::<BrokerTerminalInfo>() || abi_version != BROKER_ABI_V1 {
        return Err((
            Status::InvalidArgument,
            "broker terminal info reports an unsupported size or ABI version".to_owned(),
        ));
    }

    info.write(BrokerTerminalInfo {
        size: mem::size_of::<BrokerTerminalInfo>() as u32,
        abi_version: BROKER_ABI_V1,
        state: state.state,
        terminal_status: state.terminal_status,
        terminal_epoch_ms: state.terminal_epoch_ms,
    });
    Ok(Status::Success)
}

unsafe fn broker_observation_get_info(
    observation_handle: u64,
    info: *mut BrokerTerminalInfo,
) -> Result<Status, (Status, String)> {
    let observation = broker_observation_for(observation_handle)?;
    let state = *observation
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    write_broker_terminal_info(info, state)
}

unsafe fn broker_observation_wait(
    observation_handle: u64,
    timeout_ms: u32,
    info: *mut BrokerTerminalInfo,
) -> Result<Status, (Status, String)> {
    let observation = broker_observation_for(observation_handle)?;
    let state = {
        let state = observation
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.state >= BROKER_FRAME_STATE_COMPLETED || timeout_ms == 0 {
            *state
        } else {
            let (state, _) = observation
                .signal
                .wait_timeout(state, Duration::from_millis(u64::from(timeout_ms)))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *state
        }
    };
    write_broker_terminal_info(info, state)
}

fn broker_observation_release(observation_handle: u64) -> Result<Status, (Status, String)> {
    let observation = {
        let mut registry = broker_observation_registry();
        let observations = registry.as_mut().ok_or_else(|| {
            (
                Status::InvalidHandle,
                "broker terminal observation handle is invalid".to_owned(),
            )
        })?;
        observations.remove(&observation_handle).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "broker terminal observation handle is invalid".to_owned(),
            )
        })?
    };
    observation.completion.release();
    let _ = observation.channel.observer_leases.fetch_sub(1, Ordering::AcqRel);
    Ok(Status::Success)
}

unsafe fn broker_complete(
    handle: u64,
    correlation: u64,
    state: u32,
    reply: &[u8],
    message: String,
) -> Result<Status, (Status, String)> {
    let channel = broker_channel_for(handle)?;
    if reply.len() > channel.max_body_bytes as usize {
        return Err((
            Status::InvalidArgument,
            "broker reply exceeds the channel maximum body size".to_owned(),
        ));
    }
    let mut inner = channel.lock();
    if inner.closed {
        return Err((Status::BrokerClosed, "broker channel is closed".to_owned()));
    }
    let Some(frame) = inner.frames.get_mut(&correlation) else {
        return Err((
            Status::BrokerInvalidTerminalState,
            "broker correlation identifier is unknown".to_owned(),
        ));
    };
    if frame.is_terminal() {
        return Err((
            Status::BrokerInvalidTerminalState,
            "broker frame already reached a terminal state".to_owned(),
        ));
    }
    if frame.is_one_way() && state != BROKER_FRAME_STATE_CANCELLED {
        return Err((
            Status::BrokerInvalidTerminalState,
            "a one-way broker frame does not accept a reply".to_owned(),
        ));
    }
    frame.state = state;
    frame.reply = reply.to_vec();
    frame.error_message = message;
    frame.terminal_observation.complete(state, channel.now_epoch_ms());
    let correlation_id = frame.correlation_id;
    inner.queue.retain(|queued| *queued != correlation_id);
    drop(inner);
    channel.payload_signal.notify_all();
    channel.consumer_signal.notify_all();
    Ok(Status::Success)
}

/// Attaches one channel to one builder invocation. Mirrors `set_capabilities`.
fn set_broker(handle: u64, channel_handle: u64) -> Result<Status, (Status, String)> {
    let (session, channel) = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        let channel = state
            .broker_channels
            .get(&channel_handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "broker channel handle is invalid".to_owned()))?;
        (session, channel)
    };
    if session_has_active_operation(&session)
        || session
            .runspace_session
            .as_ref()
            .is_some_and(|runspace| runspace_session_has_active_operation(runspace))
    {
        return Err((
            Status::Backpressure,
            "a broker channel cannot be attached while its session has an active invocation".to_owned(),
        ));
    }
    let mut attached = session
        .broker_channel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if attached.is_some() {
        return Err((
            Status::Backpressure,
            "a broker channel is already attached; invoke or clear the builder first".to_owned(),
        ));
    }
    *attached = Some(BrokerConfiguration {
        channel_handle,
        channel,
        bridge: None,
    });
    Ok(Status::Success)
}

fn set_bridge(handle: u64, channel_handle: u64, bridge: BridgeAttachment) -> Result<Status, (Status, String)> {
    if bridge.binding_id == 0
        || (bridge.contract_id_low == 0 && bridge.contract_id_high == 0)
        || bridge.contract_major_version == 0
        || bridge.maximum_request_bytes == 0
        || bridge.maximum_reply_bytes == 0
        || !is_valid_bridge_variable_name(&bridge.variable_name)
    {
        return Err((
            Status::InvalidArgument,
            "bridge attachment metadata is invalid".to_owned(),
        ));
    }

    let required_request_bytes = bridge
        .maximum_request_bytes
        .checked_add(8)
        .ok_or_else(|| (Status::InvalidArgument, "bridge request bound overflows".to_owned()))?;
    let required_reply_bytes = bridge
        .maximum_reply_bytes
        .checked_add(8)
        .ok_or_else(|| (Status::InvalidArgument, "bridge reply bound overflows".to_owned()))?;
    let (session, channel) = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        let channel = state
            .broker_channels
            .get(&channel_handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "broker channel handle is invalid".to_owned()))?;
        (session, channel)
    };
    if channel.max_body_bytes < required_request_bytes || channel.max_body_bytes < required_reply_bytes {
        return Err((
            Status::InvalidArgument,
            "broker channel body bound cannot carry the generated bridge frames".to_owned(),
        ));
    }
    if session_has_active_operation(&session)
        || session
            .runspace_session
            .as_ref()
            .is_some_and(|runspace| runspace_session_has_active_operation(runspace))
    {
        return Err((
            Status::Backpressure,
            "a bridge channel cannot be attached while its session has an active invocation".to_owned(),
        ));
    }
    let mut attached = session
        .broker_channel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if attached.is_some() {
        return Err((
            Status::Backpressure,
            "a broker or bridge channel is already attached; invoke or clear the builder first".to_owned(),
        ));
    }
    *attached = Some(BrokerConfiguration {
        channel_handle,
        channel,
        bridge: Some(bridge),
    });
    Ok(Status::Success)
}

fn is_valid_bridge_variable_name(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_BRIDGE_VARIABLE_NAME_BYTES {
        return false;
    }

    let mut characters = value.bytes();
    let Some(first) = characters.next() else {
        return false;
    };
    if !matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'_') {
        return false;
    }
    characters.all(|character| matches!(character, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn session_has_broker(session: &Session) -> bool {
    session
        .broker_channel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
}

/// A builder with a broker attached may only run on an asynchronous path.
/// Rejecting the synchronous paths removes the UI-dispatch deadlock in which no
/// FFI call occurs and therefore no guard could fire.
fn reject_synchronous_broker_invocation(session: &Session) -> Result<(), (Status, String)> {
    if session_has_broker(session) {
        return Err((
            Status::UnsupportedCapability,
            "a builder with a broker channel attached must be invoked asynchronously".to_owned(),
        ));
    }
    Ok(())
}

fn take_session_broker(session: &Session) -> Option<Arc<BrokerAttachment>> {
    let taken = session
        .broker_channel
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let configuration = taken?;
    Some(Arc::new(BrokerAttachment {
        channel_handle: configuration.channel_handle,
        channel: configuration.channel,
        generation: NEXT_BROKER_GENERATION.fetch_add(1, Ordering::AcqRel),
        active: AtomicBool::new(true),
        cancelled: AtomicBool::new(false),
        bridge: configuration.bridge,
    }))
}

fn set_active_broker(session: &Session, attachment: Option<Arc<BrokerAttachment>>) {
    *session
        .active_broker
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = attachment;
}

fn clear_broker_context(session: &Session) {
    // Bridge setup can fail after the broker context is installed but before an
    // invocation consumes it. Clear the managed slot before deactivating Rust state.
    let _ = unsafe {
        session
            .power_shell
            .set_broker_context(0, 0, std::ptr::null(), std::ptr::null(), 0)
    };
}

fn finish_broker(session: &Session, attachment: Option<&Arc<BrokerAttachment>>) {
    if let Some(attachment) = attachment {
        broker_deactivate_attachment(attachment);
    }
    set_active_broker(session, None);
}

fn broker_attachment_for(channel_handle: u64, generation: u64) -> Option<Arc<BrokerAttachment>> {
    let registry = BROKER_ATTACHMENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let attachment = registry.as_ref()?.get(&generation)?;
    if attachment.channel_handle == channel_handle && attachment.active.load(Ordering::Acquire) {
        Some(Arc::clone(attachment))
    } else {
        None
    }
}

fn broker_register_attachment(attachment: &Arc<BrokerAttachment>) {
    let mut registry = BROKER_ATTACHMENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .get_or_insert_with(HashMap::new)
        .insert(attachment.generation, Arc::clone(attachment));
}

/// Revokes one broker attachment: no later trampoline call with its generation
/// can resolve, and its registry entry is dropped.
fn broker_deactivate_attachment(attachment: &Arc<BrokerAttachment>) {
    // Close admission before terminating queued frames. A trampoline that
    // resolved the attachment before unregistering rechecks this flag while
    // holding the channel lock, so it cannot enqueue behind cleanup.
    attachment.active.store(false, Ordering::Release);
    broker_finish_attachment_frames(attachment, BROKER_FRAME_STATE_ABORTED);
    broker_unregister_attachment(attachment.generation);
}

fn broker_cancel_attachment(attachment: &BrokerAttachment) {
    // Cancellation is terminal for this invocation's attachment. The channel
    // lock recheck in each trampoline closes the lookup-to-enqueue race. Set
    // the flag and terminalize the already admitted frames under the same lock:
    // the subsequent fixed cleanup Close frame is therefore not swept.
    let mut inner = attachment.channel.lock();
    attachment.cancelled.store(true, Ordering::Release);
    broker_finish_attachment_frames_locked(attachment, &mut inner, BROKER_FRAME_STATE_CANCELLED);
    drop(inner);
    attachment.channel.payload_signal.notify_all();
    attachment.channel.consumer_signal.notify_all();
}

/// The payload sink has already structurally validated this one fixed cleanup
/// frame before it reaches the native broker. Cancellation admits it as the
/// sole exception so the host observes lease closure and drops retained events;
/// no application request or event can use this path.
fn is_bridge_close_request(kind: u32, body: &[u8]) -> bool {
    kind == BRIDGE_BROKER_REQUEST_KIND
        && body.len() == BRIDGE_ROUTE_HEADER_SIZE + BRIDGE_REQUEST_HEADER_SIZE
        && body[..BRIDGE_ROUTE_HEADER_SIZE].iter().any(|byte| *byte != 0)
        && body[BRIDGE_ROUTE_HEADER_SIZE] == BRIDGE_PROTOCOL_VERSION
        && body[BRIDGE_ROUTE_HEADER_SIZE + 1] == BRIDGE_CLOSE_FRAME_KIND
        && body[BRIDGE_ROUTE_HEADER_SIZE + 2..BRIDGE_ROUTE_HEADER_SIZE + 8]
            .iter()
            .all(|byte| *byte == 0)
        && body[BRIDGE_ROUTE_HEADER_SIZE + 8..BRIDGE_ROUTE_HEADER_SIZE + 16]
            .iter()
            .all(|byte| *byte == 0)
        && body[BRIDGE_ROUTE_HEADER_SIZE + 16..BRIDGE_ROUTE_HEADER_SIZE + 24]
            .iter()
            .any(|byte| *byte != 0)
        && body[BRIDGE_ROUTE_HEADER_SIZE + 24..BRIDGE_ROUTE_HEADER_SIZE + 28]
            .iter()
            .any(|byte| *byte != 0)
        && body[BRIDGE_ROUTE_HEADER_SIZE + 28..].iter().all(|byte| *byte == 0)
}

fn broker_unregister_attachment(generation: u64) {
    let mut registry = BROKER_ATTACHMENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(map) = registry.as_mut() {
        map.remove(&generation);
    }
}

unsafe fn broker_trampoline<F>(result: *mut CallResult, operation: F) -> i32
where
    F: FnOnce() -> Result<Status, (Status, String)>,
{
    // Payload trampolines run on a pipeline thread whose thread-local
    // execution depth is nonzero, so neither v2_call helper may be used.
    let result = match prepare_call_result(result) {
        Ok(result) => result,
        Err(status) => return status.value(),
    };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(status)) => complete_call_result(result, status, ""),
        Ok(Err((status, diagnostic))) => complete_call_result(result, status, &diagnostic),
        Err(_) => complete_call_result(
            result,
            Status::Panic,
            "an unexpected native panic was contained by the PowerShell FFI",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn broker_enqueue_and_wait(
    channel_handle: u64,
    generation: u64,
    kind: u32,
    flags: u32,
    ordering_key: u64,
    deadline_ms: u32,
    body: *const u8,
    body_len: u32,
    correlation_out: *mut u64,
    reply: *mut u8,
    reply_capacity: u32,
    reply_len: *mut u32,
    result: *mut CallResult,
) -> i32 {
    broker_trampoline(result, || {
        let attachment = broker_attachment_for(channel_handle, generation).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "the broker context is not active for this invocation".to_owned(),
            )
        })?;
        let channel = Arc::clone(&attachment.channel);
        let body = read_broker_body(body, body_len, channel.max_body_bytes)?;
        let teardown_close = is_bridge_close_request(kind, &body);
        let flags = (flags & BROKER_FRAME_FLAG_MASK) | BROKER_FRAME_FLAG_MUTATING;
        let deadline_ms = if deadline_ms == 0 {
            channel.default_deadline_ms
        } else {
            deadline_ms.min(channel.default_deadline_ms)
        };

        let correlation = {
            let mut inner = channel.lock();
            if attachment.cancelled.load(Ordering::Acquire) && !teardown_close {
                return Err((
                    Status::OperationCancelled,
                    "the broker context was cancelled before the request was admitted".to_owned(),
                ));
            }
            if !attachment.active.load(Ordering::Acquire) {
                return Err((
                    Status::InvalidHandle,
                    "the broker context is no longer active for this invocation".to_owned(),
                ));
            }
            if inner.closed {
                return Err((Status::BrokerClosed, "broker channel is closed".to_owned()));
            }
            if !inner.consumer_attached {
                return Err((
                    Status::BrokerNoConsumer,
                    "no consumer pump has attached to the broker channel".to_owned(),
                ));
            }
            let now_ms = channel.now_epoch_ms();
            broker_expire_due_frames(&mut inner, now_ms);
            if broker_inflight_count(&inner) >= channel.max_inflight {
                return Err((
                    Status::BrokerBusy,
                    "the broker channel has reached its maximum inflight frames".to_owned(),
                ));
            }
            if broker_has_active_mutating_key(&inner, ordering_key) {
                return Err((
                    Status::BrokerBusy,
                    "the broker ordering key already has an active mutating frame".to_owned(),
                ));
            }
            let correlation = inner.next_correlation;
            inner.next_correlation = inner
                .next_correlation
                .checked_add(1)
                .ok_or_else(|| (Status::HostFailure, "broker correlations are exhausted".to_owned()))?;
            inner.frames.insert(
                correlation,
                BrokerFrame {
                    correlation_id: correlation,
                    generation: attachment.generation,
                    ordering_key,
                    kind,
                    flags,
                    deadline_epoch_ms: now_ms.saturating_add(u64::from(deadline_ms)),
                    body,
                    state: BROKER_FRAME_STATE_QUEUED,
                    dropped_before: 0,
                    reply: Vec::new(),
                    error_message: String::new(),
                    owner_token: 0,
                    terminal_observation: Arc::new(BrokerTerminalObservation::new()),
                },
            );
            inner.queue.push_back(correlation);
            correlation
        };
        channel.consumer_signal.notify_all();
        if !correlation_out.is_null() {
            *correlation_out = correlation;
        }

        // Wait on the channel's own condvar only. STATE is not held here.
        let mut inner = channel.lock();
        loop {
            let now_ms = channel.now_epoch_ms();
            let (terminal_state, deadline_epoch_ms) = match inner.frames.get(&correlation) {
                Some(frame) => (
                    if frame.is_terminal() { Some(frame.state) } else { None },
                    frame.deadline_epoch_ms,
                ),
                None => (Some(BROKER_FRAME_STATE_ABORTED), now_ms),
            };
            if let Some(state) = terminal_state {
                let frame = inner.frames.remove(&correlation);
                // The correlation may still be queued if it terminated before a
                // consumer dequeued it; leaving it would dangle forever.
                inner.queue.retain(|queued| *queued != correlation);
                drop(inner);
                let frame = match frame {
                    Some(frame) => frame,
                    None => {
                        return Err((Status::BrokerClosed, "broker channel is closed".to_owned()));
                    }
                };
                channel.consumer_signal.notify_all();
                return finish_broker_wait(frame, state, reply, reply_capacity, reply_len);
            }
            if deadline_epoch_ms <= now_ms {
                broker_finish_frame(&mut inner, correlation, BROKER_FRAME_STATE_TIMED_OUT, now_ms);
                continue;
            }
            let remaining = Duration::from_millis(deadline_epoch_ms - now_ms);
            let (guard, _) = channel
                .payload_signal
                .wait_timeout(inner, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            inner = guard;
        }
    })
}

unsafe fn finish_broker_wait(
    frame: BrokerFrame,
    state: u32,
    reply: *mut u8,
    reply_capacity: u32,
    reply_len: *mut u32,
) -> Result<Status, (Status, String)> {
    if state != BROKER_FRAME_STATE_COMPLETED {
        let status = broker_terminal_status(state);
        let diagnostic = if frame.error_message.is_empty() {
            match status {
                Status::OperationCancelled => "the broker request was cancelled".to_owned(),
                Status::BrokerTimeout => "the broker request passed its deadline".to_owned(),
                Status::BrokerClosed => "the broker channel closed before the request completed".to_owned(),
                _ => "the broker request failed".to_owned(),
            }
        } else {
            frame.error_message
        };
        return Err((status, diagnostic));
    }
    let length = frame.reply.len() as u32;
    if !reply_len.is_null() {
        *reply_len = length;
    }
    if reply.is_null() || reply_capacity < length {
        return Ok(Status::BufferTooSmall);
    }
    if length > 0 {
        std::ptr::copy_nonoverlapping(frame.reply.as_ptr(), reply, length as usize);
    }
    Ok(Status::Success)
}

unsafe fn read_broker_body(body: *const u8, body_len: u32, maximum: u32) -> Result<Vec<u8>, (Status, String)> {
    if body_len > maximum {
        return Err((
            Status::InvalidArgument,
            "broker frame body exceeds the channel maximum body size".to_owned(),
        ));
    }
    if body_len == 0 {
        return Ok(Vec::new());
    }
    if body.is_null() {
        return Err((
            Status::InvalidArgument,
            "broker frame body pointer is null for a non-empty body".to_owned(),
        ));
    }
    Ok(slice::from_raw_parts(body, body_len as usize).to_vec())
}

unsafe extern "C" fn broker_post(
    channel_handle: u64,
    generation: u64,
    kind: u32,
    ordering_key: u64,
    body: *const u8,
    body_len: u32,
    result: *mut CallResult,
) -> i32 {
    broker_trampoline(result, || {
        let attachment = broker_attachment_for(channel_handle, generation).ok_or_else(|| {
            (
                Status::InvalidHandle,
                "the broker context is not active for this invocation".to_owned(),
            )
        })?;
        let channel = Arc::clone(&attachment.channel);
        let body = read_broker_body(body, body_len, channel.max_body_bytes)?;

        let mut inner = channel.lock();
        if attachment.cancelled.load(Ordering::Acquire) {
            return Err((
                Status::OperationCancelled,
                "the broker context was cancelled before the event was admitted".to_owned(),
            ));
        }
        if !attachment.active.load(Ordering::Acquire) {
            return Err((
                Status::InvalidHandle,
                "the broker context is no longer active for this invocation".to_owned(),
            ));
        }
        if inner.closed {
            return Err((Status::BrokerClosed, "broker channel is closed".to_owned()));
        }
        let now_ms = channel.now_epoch_ms();
        broker_expire_due_frames(&mut inner, now_ms);
        if broker_inflight_count(&inner) >= channel.max_inflight {
            // An event must never apply backpressure to a pipeline: evict the
            // oldest queued one-way frame of the same kind, then of any kind,
            // and otherwise drop this one. Every loss is counted.
            let victim = inner
                .queue
                .iter()
                .copied()
                .find(|correlation| {
                    inner
                        .frames
                        .get(correlation)
                        .is_some_and(|frame| frame.is_one_way() && frame.kind == kind)
                })
                .or_else(|| {
                    inner
                        .queue
                        .iter()
                        .copied()
                        .find(|correlation| inner.frames.get(correlation).is_some_and(|frame| frame.is_one_way()))
                });
            match victim {
                Some(correlation) => {
                    inner.queue.retain(|queued| *queued != correlation);
                    inner.frames.remove(&correlation);
                    inner.pending_drops = inner.pending_drops.saturating_add(1);
                }
                None => {
                    inner.pending_drops = inner.pending_drops.saturating_add(1);
                    return Ok(Status::Success);
                }
            }
        }
        let correlation = inner.next_correlation;
        inner.next_correlation = inner
            .next_correlation
            .checked_add(1)
            .ok_or_else(|| (Status::HostFailure, "broker correlations are exhausted".to_owned()))?;
        inner.frames.insert(
            correlation,
            BrokerFrame {
                correlation_id: correlation,
                generation: attachment.generation,
                ordering_key,
                kind,
                flags: BROKER_FRAME_FLAG_ONE_WAY,
                deadline_epoch_ms: now_ms.saturating_add(u64::from(channel.default_deadline_ms)),
                body,
                state: BROKER_FRAME_STATE_QUEUED,
                dropped_before: 0,
                reply: Vec::new(),
                error_message: String::new(),
                owner_token: 0,
                terminal_observation: Arc::new(BrokerTerminalObservation::new()),
            },
        );
        inner.queue.push_back(correlation);
        drop(inner);
        channel.consumer_signal.notify_all();
        Ok(Status::Success)
    })
}

fn with_result<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiInvocationResult) -> Result<Status, (Status, String)>,
{
    let result = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        match state.results.get(&handle) {
            Some(result) => Arc::clone(result),
            None => {
                return Err((Status::InvalidHandle, "Invocation result handle is invalid".to_owned()));
            }
        }
    };

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation(&result.result)
}

#[allow(clippy::arc_with_non_send_sync)]
fn initialize_direct_payload(payload_path: &str) -> Result<Status, (Status, String)> {
    initialize_direct_payload_with_contract_packs(payload_path, Vec::new())
}

fn initialize_direct_payload_with_contract_packs(
    payload_path: &str,
    contract_packs: Vec<LiveObjectContractPack>,
) -> Result<Status, (Status, String)> {
    let payload_path = fs::canonicalize(payload_path).map_err(|error| {
        (
            Status::InvalidArgument,
            format!("PowerShell payload directory cannot be resolved: {}", error),
        )
    })?;
    if !payload_path.is_dir() {
        return Err((
            Status::InvalidArgument,
            "PowerShell payload path must be an existing directory".to_owned(),
        ));
    }
    initialize_runtime(payload_path, contract_packs)
}

fn initialize_from_path() -> Result<Status, (Status, String)> {
    initialize_from_path_with_contract_packs(Vec::new())
}

fn initialize_from_path_with_contract_packs(
    contract_packs: Vec<LiveObjectContractPack>,
) -> Result<Status, (Status, String)> {
    let payload_path = find_pwsh_dir().ok_or_else(|| {
        (
            Status::HostFailure,
            "PowerShell was not found on PATH; provide an explicit payload directory instead".to_owned(),
        )
    })?;
    let payload_path = payload_path.to_str().ok_or_else(|| {
        (
            Status::HostFailure,
            "the PowerShell payload selected from PATH is not valid UTF-8".to_owned(),
        )
    })?;
    initialize_direct_payload_with_contract_packs(payload_path, contract_packs)
}

#[allow(clippy::arc_with_non_send_sync)]
fn initialize_runtime(
    payload_path: PathBuf,
    contract_packs: Vec<LiveObjectContractPack>,
) -> Result<Status, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(runtime) = &state.runtime {
        return if state.activation_source_root.as_ref() == Some(&payload_path)
            && state.live_object_contract_packs == contract_packs
        {
            Ok(Status::Success)
        } else {
            let selected_path = state
                .activation_source_root
                .as_deref()
                .unwrap_or_else(|| runtime.pwsh_dir());
            Err((
                Status::IncompatiblePayload,
                format!(
                    "PowerShell runtime is already initialized for {}; cannot select a different payload or contract-pack set",
                    selected_path.display(),
                ),
            ))
        };
    }

    let runtime = HostedRuntime::new_for_pwsh_dir_with_contract_packs(&payload_path, &contract_packs)
        .map_err(|error| (Status::HostFailure, error.to_string()))?;
    state.runtime = Some(Arc::new(runtime));
    state.activation_source_root = Some(payload_path);
    state.live_object_contract_packs = contract_packs;
    Ok(Status::Success)
}

fn active_payload_path() -> Result<String, (Status, String)> {
    let state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let payload_path = state.activation_source_root.as_deref().ok_or_else(|| {
        (
            Status::NotInitialized,
            "PowerShell runtime is not initialized".to_owned(),
        )
    })?;
    payload_path.to_str().map(str::to_owned).ok_or_else(|| {
        (
            Status::HostFailure,
            "the initialized PowerShell payload path is not valid UTF-8".to_owned(),
        )
    })
}

fn runtime_diagnostics() -> Result<(pwsh_host::FfiPayloadRuntimeDiagnostics, Vec<String>), (Status, String)> {
    let (runtime, contract_pack_identities) = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let runtime = state.runtime.as_ref().cloned().ok_or_else(|| {
            (
                Status::NotInitialized,
                "PowerShell runtime is not initialized".to_owned(),
            )
        })?;
        let identities = state
            .live_object_contract_packs
            .iter()
            .map(|pack| pack.payload_adapter_type_name.clone())
            .collect();
        (runtime, identities)
    };

    let diagnostics = runtime.runtime_diagnostics().map_err(managed_failure)?;
    Ok((diagnostics, contract_pack_identities))
}

fn create_session_result() -> Result<u64, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let runtime = state.runtime.as_ref().cloned().ok_or_else(|| {
        (
            Status::NotInitialized,
            "PowerShell runtime is not initialized".to_owned(),
        )
    })?;
    let power_shell =
        FfiPowerShell::new_for_runtime(runtime).map_err(|error| (Status::HostFailure, error.to_string()))?;
    let handle = state.next_handle;
    state.next_handle = state.next_handle.checked_add(1).unwrap_or(1);
    state.sessions.insert(
        handle,
        Arc::new(Session {
            power_shell,
            operation_active: Mutex::new(false),
            runspace_session: None,
            capability_registration: Mutex::new(None),
            active_capability: Mutex::new(None),
            broker_channel: Mutex::new(None),
            active_broker: Mutex::new(None),
        }),
    );
    Ok(handle)
}

fn release_session_result(handle: u64) -> Result<Status, (Status, String)> {
    let operations = {
        let mut state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .sessions
            .remove(&handle)
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell handle is invalid".to_owned()))?;
        state
            .operations
            .values()
            .filter(|operation| operation.builder_handle == handle)
            .cloned()
            .collect::<Vec<_>>()
    };
    for operation in operations {
        operation.request_stop();
    }
    Ok(Status::Success)
}

struct SessionOptionsInput<'a> {
    runspace_mode: u32,
    initial_configuration: u32,
    history_mode: u32,
    error_preference: u32,
    warning_preference: u32,
    verbose_preference: u32,
    debug_preference: u32,
    information_preference: u32,
    execution_policy: u32,
    initial_variables: &'a [u8],
    module_imports_payload: &'a [u8],
    allowed_module_paths_payload: &'a [u8],
    working_directory: &'a str,
    environment_payload: &'a [u8],
}

unsafe fn session_options_input<'a>(
    options: *const SessionOptions,
) -> Result<SessionOptionsInput<'a>, (Status, String)> {
    session_options_input_with_current_runspace_rejection(options, true)
}

unsafe fn session_options_input_with_current_runspace_rejection<'a>(
    options: *const SessionOptions,
    reject_current_runspace_configuration: bool,
) -> Result<SessionOptionsInput<'a>, (Status, String)> {
    if options.is_null() {
        return Err((
            Status::InvalidArgument,
            "PowerShell session options structure is null".to_owned(),
        ));
    }
    let prefix = &*(options as *const SessionOptionsPrefix);
    let extended_size = mem::size_of::<SessionOptions>() as u32;
    if prefix.size < SESSION_OPTIONS_PREFIX_SIZE
        || (prefix.size > SESSION_OPTIONS_PREFIX_SIZE && prefix.size < extended_size)
    {
        return Err((
            Status::InvalidArgument,
            "PowerShell session options structure is missing or too small".to_owned(),
        ));
    }
    let has_extended_configuration = prefix.size >= extended_size;
    if prefix.flags != 0 || prefix._reserved != 0 {
        return Err((
            Status::InvalidArgument,
            "PowerShell session options flags and reserved fields must be zero".to_owned(),
        ));
    }
    if prefix.runspace_mode > 1
        || prefix.initial_configuration > 1
        || prefix.history_mode > 1
        || prefix.error_preference > 3
        || prefix.warning_preference > 3
        || prefix.verbose_preference > 3
        || prefix.debug_preference > 3
        || prefix.information_preference > 3
    {
        return Err((
            Status::InvalidArgument,
            "PowerShell session options contain an unsupported enum value".to_owned(),
        ));
    }
    let legacy_allowed_module_path = utf8_span(prefix.allowed_module_path).map_err(|_| {
        (
            Status::InvalidArgument,
            "PowerShell session allowed module path must be UTF-8 without NUL".to_owned(),
        )
    })?;
    if !legacy_allowed_module_path.is_empty() {
        return Err((
            Status::InvalidArgument,
            "PowerShell session allowed_module_path is obsolete; use the appended allowed_module_paths configuration"
                .to_owned(),
        ));
    }
    let (
        execution_policy,
        initial_variables,
        initial_variables_are_empty,
        module_imports,
        module_imports_payload,
        allowed_module_paths,
        allowed_module_paths_payload,
        working_directory,
        environment,
        environment_payload,
    ) = if has_extended_configuration {
        let options = &*options;
        if options.configuration_flags != 0 || options.execution_policy > 1 {
            return Err((
                Status::InvalidArgument,
                "PowerShell session configuration flags and execution policy are invalid".to_owned(),
            ));
        }
        let (initial_variables_kind, initial_variables) = data_value_input(&options.initial_variables)?;
        if initial_variables_kind != VALUE_KIND_PROPERTY_BAG {
            return Err((
                Status::InvalidArgument,
                "PowerShell session initial variables must be a tagged property bag".to_owned(),
            ));
        }
        let initial_variables_are_empty = validate_session_initial_variables(initial_variables)?;
        let (module_imports_kind, module_imports_payload) = data_value_input(&options.module_imports)?;
        let module_imports = session_string_array(module_imports_kind, module_imports_payload, "module imports")?;
        let (module_paths_kind, module_paths_payload) = data_value_input(&options.allowed_module_paths)?;
        let allowed_module_paths = session_string_array(module_paths_kind, module_paths_payload, "module paths")?;
        let working_directory = utf8_span(options.working_directory).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session working directory must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if working_directory.len() > MAX_SESSION_PATH_BYTES
            || (!working_directory.is_empty() && !std::path::Path::new(working_directory).is_absolute())
        {
            return Err((
                Status::InvalidArgument,
                "PowerShell session working directory must be an absolute bounded path".to_owned(),
            ));
        }
        let (environment_kind, environment_payload) = data_value_input(&options.environment)?;
        let environment = session_environment(environment_kind, environment_payload)?;
        (
            options.execution_policy,
            initial_variables,
            initial_variables_are_empty,
            module_imports,
            module_imports_payload,
            allowed_module_paths,
            module_paths_payload,
            working_directory,
            environment,
            environment_payload,
        )
    } else {
        (
            0,
            &EMPTY_VALUE_CONTAINER[..],
            true,
            Vec::new(),
            &EMPTY_VALUE_CONTAINER[..],
            Vec::new(),
            &EMPTY_VALUE_CONTAINER[..],
            "",
            Vec::new(),
            &EMPTY_VALUE_CONTAINER[..],
        )
    };
    if reject_current_runspace_configuration
        && prefix.runspace_mode == 0
        && (prefix.initial_configuration != 0
            || prefix.history_mode != 0
            || prefix.error_preference != 0
            || prefix.warning_preference != 0
            || prefix.verbose_preference != 0
            || prefix.debug_preference != 0
            || prefix.information_preference != 0
            || execution_policy != 0
            || !initial_variables_are_empty
            || !module_imports.is_empty()
            || !allowed_module_paths.is_empty()
            || !working_directory.is_empty()
            || !environment.is_empty())
    {
        return Err((
            Status::UnsupportedCapability,
            "current-runspace sessions cannot change configuration, history, preferences, variables, imports, paths, working directory, or environment".to_owned(),
        ));
    }

    Ok(SessionOptionsInput {
        runspace_mode: prefix.runspace_mode,
        initial_configuration: prefix.initial_configuration,
        history_mode: prefix.history_mode,
        error_preference: prefix.error_preference,
        warning_preference: prefix.warning_preference,
        verbose_preference: prefix.verbose_preference,
        debug_preference: prefix.debug_preference,
        information_preference: prefix.information_preference,
        execution_policy,
        initial_variables,
        module_imports_payload,
        allowed_module_paths_payload,
        working_directory,
        environment_payload,
    })
}

fn session_string_array<'a>(kind: u32, payload: &'a [u8], description: &str) -> Result<Vec<&'a str>, (Status, String)> {
    if kind != VALUE_KIND_ARRAY {
        return Err((
            Status::InvalidArgument,
            format!("PowerShell session {} must be a tagged array", description),
        ));
    }
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, description)? as usize;
    if count > MAX_SESSION_CONFIGURATION_ENTRIES {
        return Err((
            Status::InvalidArgument,
            format!(
                "PowerShell session {} exceed the bound of {}",
                description, MAX_SESSION_CONFIGURATION_ENTRIES
            ),
        ));
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let nested_kind = read_value_u32(payload, &mut offset, description)?;
        let length = read_value_u32(payload, &mut offset, description)? as usize;
        let bytes = read_value_bytes(payload, &mut offset, length, description)?;
        if nested_kind != VALUE_KIND_STRING {
            return Err((
                Status::InvalidArgument,
                format!("PowerShell session {} must contain only tagged strings", description),
            ));
        }
        let value = std::str::from_utf8(bytes).map_err(|_| {
            (
                Status::InvalidArgument,
                format!("PowerShell session {} contains invalid UTF-8", description),
            )
        })?;
        if value.is_empty() || value.as_bytes().contains(&0) {
            return Err((
                Status::InvalidArgument,
                format!("PowerShell session {} contains an invalid string", description),
            ));
        }
        if description == "module imports" && !valid_module_import_name(value) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session module imports must use bounded ASCII module names".to_owned(),
            ));
        }
        if description == "module paths"
            && (value.len() > MAX_SESSION_PATH_BYTES || !std::path::Path::new(value).is_absolute())
        {
            return Err((
                Status::InvalidArgument,
                "PowerShell session module paths must be absolute bounded paths".to_owned(),
            ));
        }
        values.push(value);
    }
    if offset != payload.len() {
        return Err((
            Status::InvalidArgument,
            format!("PowerShell session {} contain trailing bytes", description),
        ));
    }
    Ok(values)
}

fn session_environment(kind: u32, payload: &[u8]) -> Result<Vec<(&str, &str)>, (Status, String)> {
    if kind != VALUE_KIND_PROPERTY_BAG {
        return Err((
            Status::InvalidArgument,
            "PowerShell session environment must be a tagged property bag".to_owned(),
        ));
    }
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, "environment")? as usize;
    if count > MAX_SESSION_CONFIGURATION_ENTRIES {
        return Err((
            Status::InvalidArgument,
            format!(
                "PowerShell session environment exceeds the bound of {}",
                MAX_SESSION_CONFIGURATION_ENTRIES
            ),
        ));
    }
    let mut entries: Vec<(&str, &str)> = Vec::with_capacity(count);
    for _ in 0..count {
        let key_length = read_value_u32(payload, &mut offset, "environment key")? as usize;
        let key_bytes = read_value_bytes(payload, &mut offset, key_length, "environment key")?;
        let key = std::str::from_utf8(key_bytes).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session environment key is invalid UTF-8".to_owned(),
            )
        })?;
        let value_kind = read_value_u32(payload, &mut offset, "environment value")?;
        let value_length = read_value_u32(payload, &mut offset, "environment value")? as usize;
        let value_bytes = read_value_bytes(payload, &mut offset, value_length, "environment value")?;
        if value_kind != VALUE_KIND_STRING {
            return Err((
                Status::InvalidArgument,
                "PowerShell session environment values must be tagged strings".to_owned(),
            ));
        }
        let value = std::str::from_utf8(value_bytes).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session environment value is invalid UTF-8".to_owned(),
            )
        })?;
        if !valid_session_name(key) || value.as_bytes().contains(&0) || value.len() > 4096 {
            return Err((
                Status::InvalidArgument,
                "PowerShell session environment entry is invalid".to_owned(),
            ));
        }
        if entries.iter().any(|(existing, _)| existing.eq_ignore_ascii_case(key)) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session environment keys must be unique".to_owned(),
            ));
        }
        entries.push((key, value));
    }
    if offset != payload.len() {
        return Err((
            Status::InvalidArgument,
            "PowerShell session environment contains trailing bytes".to_owned(),
        ));
    }
    Ok(entries)
}

fn valid_session_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    match bytes.next() {
        Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => {}
        _ => return false,
    }

    value.len() <= 64 && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn validate_session_initial_variables(payload: &[u8]) -> Result<bool, (Status, String)> {
    let mut offset = 0;
    let count = read_value_u32(payload, &mut offset, "initial variable count")? as usize;
    if count > MAX_SESSION_CONFIGURATION_ENTRIES {
        return Err((
            Status::InvalidArgument,
            format!(
                "PowerShell session initial variables exceed the bound of {}",
                MAX_SESSION_CONFIGURATION_ENTRIES
            ),
        ));
    }
    let mut names: Vec<&str> = Vec::with_capacity(count);
    for _ in 0..count {
        let key_length = read_value_u32(payload, &mut offset, "initial variable name")? as usize;
        let key_bytes = read_value_bytes(payload, &mut offset, key_length, "initial variable name")?;
        let key = std::str::from_utf8(key_bytes).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session initial variable name is invalid UTF-8".to_owned(),
            )
        })?;
        if !valid_session_name(key) || names.iter().any(|existing| existing.eq_ignore_ascii_case(key)) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session initial variable names must be unique identifiers".to_owned(),
            ));
        }
        names.push(key);
        validate_nested_value(payload, &mut offset, 1)?;
    }
    if offset == payload.len() {
        Ok(count == 0)
    } else {
        Err((
            Status::InvalidArgument,
            "PowerShell session initial variables contain trailing bytes".to_owned(),
        ))
    }
}

fn create_runspace_session(options: SessionOptionsInput<'_>) -> Result<u64, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let runtime = state.runtime.as_ref().cloned().ok_or_else(|| {
        (
            Status::NotInitialized,
            "PowerShell runtime is not initialized".to_owned(),
        )
    })?;
    let session = FfiPowerShellSession::new_for_runtime(
        runtime,
        options.runspace_mode,
        options.initial_configuration,
        options.history_mode,
        options.error_preference,
        options.warning_preference,
        options.verbose_preference,
        options.debug_preference,
        options.information_preference,
        options.execution_policy,
        options.initial_variables,
        options.module_imports_payload,
        options.allowed_module_paths_payload,
        options.working_directory,
        options.environment_payload,
    )
    .map_err(managed_failure)?;
    let handle = state.next_runspace_session_handle;
    state.next_runspace_session_handle = state
        .next_runspace_session_handle
        .checked_add(1)
        .filter(|value| *value != 0)
        .unwrap_or(1_u64 << 61);
    state.runspace_sessions.insert(
        handle,
        Arc::new(RunspaceSession {
            session,
            operation_active: Mutex::new(false),
        }),
    );
    Ok(handle)
}

fn valid_module_import_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn release_runspace_session(handle: u64) -> Result<Status, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state
        .runspace_sessions
        .remove(&handle)
        .map(|_| Status::Success)
        .ok_or_else(|| (Status::InvalidHandle, "PowerShell session handle is invalid".to_owned()))
}

fn with_runspace_session<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiPowerShellSession) -> Result<Status, (Status, String)>,
{
    let session = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .runspace_sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell session handle is invalid".to_owned()))?
    };
    operation(&session.session)
}

fn with_runspace_session_mutation<F>(handle: u64, operation: F) -> Result<Status, (Status, String)>
where
    F: FnOnce(&FfiPowerShellSession) -> Result<Status, (Status, String)>,
{
    reject_active_pipeline_session_mutation()?;
    let session = {
        let state = match state().lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state
            .runspace_sessions
            .get(&handle)
            .cloned()
            .ok_or_else(|| (Status::InvalidHandle, "PowerShell session handle is invalid".to_owned()))?
    };
    reject_active_pipeline_session_mutation()?;
    if runspace_session_has_active_operation(&session) {
        return Err((
            Status::Backpressure,
            "PowerShell session variables cannot be read or changed while an async operation is pending or running."
                .to_owned(),
        ));
    }
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reject_active_pipeline_session_mutation()?;
    if runspace_session_has_active_operation(&session) {
        return Err((
            Status::Backpressure,
            "PowerShell session variables cannot be read or changed while an async operation is pending or running."
                .to_owned(),
        ));
    }
    operation(&session.session)
}

fn create_session_builder(handle: u64) -> Result<u64, (Status, String)> {
    let mut state = match state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let runspace_session = state
        .runspace_sessions
        .get(&handle)
        .cloned()
        .ok_or_else(|| (Status::InvalidHandle, "PowerShell session handle is invalid".to_owned()))?;
    // Keep STATE locked through the managed lease acquisition. Session release
    // removes the public handle under the same lock, so it cannot race this
    // successful builder creation and produce a post-release builder.
    let power_shell = runspace_session.session.create_builder().map_err(managed_failure)?;
    let builder_handle = state.next_handle;
    state.next_handle = state.next_handle.checked_add(1).unwrap_or(1);
    state.sessions.insert(
        builder_handle,
        Arc::new(Session {
            power_shell,
            operation_active: Mutex::new(false),
            runspace_session: Some(runspace_session),
            capability_registration: Mutex::new(None),
            active_capability: Mutex::new(None),
            broker_channel: Mutex::new(None),
            active_broker: Mutex::new(None),
        }),
    );
    Ok(builder_handle)
}

fn write_session_snapshot(
    snapshot: *mut SessionSnapshot,
    value: FfiSessionSnapshot,
) -> Result<Status, (Status, String)> {
    if snapshot.is_null() || unsafe { (*snapshot).size } < mem::size_of::<SessionSnapshot>() as u32 {
        return Err((
            Status::InvalidArgument,
            "PowerShell session snapshot structure is missing or too small".to_owned(),
        ));
    }
    if value.event_count > 32 {
        return Err((
            Status::ManagedFailure,
            "managed PowerShell session event count exceeds its bound".to_owned(),
        ));
    }
    unsafe {
        (*snapshot).state = value.state;
        (*snapshot).runspace_state = value.runspace_state;
        (*snapshot).flags = value.flags;
        (*snapshot).active_pipeline_count = value.active_pipeline_count;
        (*snapshot).event_count = value.event_count;
        (*snapshot).invocation_count = value.invocation_count;
        (*snapshot).history_count = value.history_count;
    }
    Ok(Status::Success)
}

fn validate_pool_options(options: *const SessionPoolOptions) -> Result<(), (Status, String)> {
    if options.is_null() || unsafe { (*options).size } < mem::size_of::<SessionPoolOptions>() as u32 {
        return Err((
            Status::InvalidArgument,
            "PowerShell session pool options structure is missing or too small".to_owned(),
        ));
    }
    let options = unsafe { &*options };
    if options.flags != 0 || options._reserved != 0 || options.maximum_sessions == 0 || options.maximum_sessions > 64 {
        return Err((
            Status::InvalidArgument,
            "PowerShell session pool options are invalid or exceed the bound of 64 sessions".to_owned(),
        ));
    }
    if options.minimum_sessions > options.maximum_sessions {
        return Err((
            Status::InvalidArgument,
            "PowerShell session pool minimum sessions exceeds maximum sessions".to_owned(),
        ));
    }
    Ok(())
}

fn feature_flags() -> u64 {
    FEATURE_STRUCTURED_INVOCATION_ERRORS
        | FEATURE_PER_CALL_DIAGNOSTICS
        | FEATURE_UTF8_SPANS
        | FEATURE_IMMUTABLE_RESULTS
        | FEATURE_TAGGED_VALUES
        | FEATURE_COMMAND_OPTIONS
        | FEATURE_BOUNDED_INPUT
        | FEATURE_INVOCATION_METADATA
        | FEATURE_ASYNC_OPERATIONS
        | FEATURE_SESSIONS
        | FEATURE_SESSION_POLLING
        | FEATURE_SESSION_POOL_REJECTION
        | FEATURE_SNAPSHOT_PROJECTIONS
        | FEATURE_SESSION_CONFIGURATION
        | FEATURE_SESSION_VARIABLES
        | FEATURE_CAPABILITY_RPC
        | FEATURE_LIVE_OBJECT_PROBE
        | FEATURE_LIVE_SESSION_OBJECT_PROBE
        | FEATURE_LIVE_OBJECT_CONTRACTS
        | FEATURE_LIVE_STREAM_POLLING
        | FEATURE_TYPED_RESULT_PAGING
        | FEATURE_OBSERVED_INVOCATION
        | FEATURE_SESSION_PREFLIGHT
        | FEATURE_RUNTIME_DIAGNOSTICS
        | FEATURE_DUPLEX_BROKER_CHANNEL
        | FEATURE_GENERATED_BRIDGE_ATTACHMENT
        | FEATURE_BROKER_TERMINAL_OBSERVATION
        | FEATURE_RELIABLE_BRIDGE_EVENTS
        | FEATURE_OBSERVED_PRESENTATION
        | FEATURE_SECRET_ADAPTERS
        | FEATURE_CREDENTIAL_RESULT
        | FEATURE_REMOTE_PROVIDER_DIAGNOSTICS
}

fn create_live_object_probe(initial_count: i64) -> Result<*mut std::ffi::c_void, (Status, String)> {
    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.runtime.as_ref().cloned().ok_or_else(|| {
            (
                Status::NotInitialized,
                "PowerShell runtime has not been initialized".to_owned(),
            )
        })?
    };
    runtime.create_live_object_probe(initial_count).map_err(managed_failure)
}

fn release_live_object_probe(com_object: *mut std::ffi::c_void) -> Result<Status, (Status, String)> {
    if com_object.is_null() {
        return Err((Status::InvalidArgument, "live object probe pointer is null".to_owned()));
    }

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.runtime.as_ref().cloned().ok_or_else(|| {
            (
                Status::NotInitialized,
                "PowerShell runtime has not been initialized".to_owned(),
            )
        })?
    };
    runtime
        .release_live_object_probe(com_object)
        .map(|_| Status::Success)
        .map_err(managed_failure)
}

fn unregister_live_object_probe(com_object: *mut std::ffi::c_void) -> Result<Status, (Status, String)> {
    if com_object.is_null() {
        return Err((Status::InvalidArgument, "live object probe pointer is null".to_owned()));
    }

    let _operation_lock = SESSION_OPERATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = {
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.runtime.as_ref().cloned().ok_or_else(|| {
            (
                Status::NotInitialized,
                "PowerShell runtime has not been initialized".to_owned(),
            )
        })?
    };
    runtime
        .unregister_live_object_probe(com_object)
        .map(|_| Status::Success)
        .map_err(managed_failure)
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_get_abi_info(info: *mut AbiInfo) -> i32 {
    if info.is_null() || (*info).size < std::mem::size_of::<AbiInfo>() as u32 {
        return Status::InvalidArgument.value();
    }

    (*info).abi_version = ABI_VERSION;
    (*info).feature_flags = feature_flags();
    (*info).minimum_compatible_abi_version = MINIMUM_COMPATIBLE_ABI_VERSION;
    (*info)._reserved = 0;
    Status::Success.value()
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_get_runtime_diagnostics_info(
    info: *mut RuntimeDiagnosticsInfo,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if info.is_null() || (*info).size < mem::size_of::<RuntimeDiagnosticsInfo>() as u32 {
            return Err((
                Status::InvalidArgument,
                "runtime diagnostics info output is invalid".to_owned(),
            ));
        }

        let (diagnostics, contract_pack_identities) = runtime_diagnostics()?;
        if contract_pack_identities.len() > MAX_LIVE_OBJECT_CONTRACT_PACKS {
            return Err((
                Status::ManagedFailure,
                "runtime diagnostics contract-pack count exceeds its bound".to_owned(),
            ));
        }

        (*info).bindings_abi_version = diagnostics.bindings_abi_version;
        (*info).payload_table_size = diagnostics.payload_table_size;
        (*info).payload_table_slot_count = diagnostics.payload_table_slot_count;
        (*info).payload_table_shape = 1;
        (*info).power_shell_file_version_available = u32::from(diagnostics.power_shell_file_version.is_some());
        (*info).contract_pack_count = contract_pack_identities.len() as u32;
        (*info)._reserved = 0;
        (*info).remote_provider_status = diagnostics.devolutions_wsman_provider_status;
        (*info).remote_provider_unavailable_reason = diagnostics.devolutions_wsman_provider_unavailable_reason;
        (*info)._reserved2 = 0;
        (*info).remote_provider_capabilities = diagnostics.devolutions_wsman_provider_capabilities;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_copy_runtime_diagnostics_power_shell_file_version_utf8(
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let (diagnostics, _) = runtime_diagnostics()?;
        let value = diagnostics.power_shell_file_version.unwrap_or_default();
        if value.len() > 128 {
            return Err((
                Status::ManagedFailure,
                "runtime diagnostics PowerShell file version exceeds its bound".to_owned(),
            ));
        }

        Ok(unsafe { write_utf8(buffer, buffer_len, required_len, &value) })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_copy_runtime_diagnostics_contract_pack_identity_utf8(
    index: u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let (_, contract_pack_identities) = runtime_diagnostics()?;
        let value = contract_pack_identities.get(index as usize).ok_or_else(|| {
            (
                Status::InvalidArgument,
                "runtime diagnostics contract-pack index is invalid".to_owned(),
            )
        })?;
        if value.is_empty() || value.len() > MAX_LIVE_OBJECT_CONTRACT_PACK_TYPE_NAME_BYTES {
            return Err((
                Status::ManagedFailure,
                "runtime diagnostics contract-pack identity is invalid".to_owned(),
            ));
        }

        Ok(unsafe { write_utf8(buffer, buffer_len, required_len, value) })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_initialize_utf8(payload_path: Utf8Span, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        let payload_path = utf8_span(payload_path).map_err(|_| {
            (
                Status::InvalidArgument,
                "payload path must be UTF-8 without NUL".to_owned(),
            )
        })?;
        initialize_direct_payload(payload_path)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_initialize_with_contract_packs_utf8(
    payload_path: Utf8Span,
    contract_packs: *const LiveObjectContractPackInput,
    contract_pack_count: usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let payload_path = utf8_span(payload_path).map_err(|_| {
            (
                Status::InvalidArgument,
                "payload path must be UTF-8 without NUL".to_owned(),
            )
        })?;
        let contract_packs = live_object_contract_pack_inputs(contract_packs, contract_pack_count)?;
        initialize_direct_payload_with_contract_packs(payload_path, contract_packs)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_initialize_from_path(result: *mut CallResult) -> i32 {
    v2_call(result, initialize_from_path)
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_initialize_from_path_with_contract_packs(
    contract_packs: *const LiveObjectContractPackInput,
    contract_pack_count: usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let contract_packs = live_object_contract_pack_inputs(contract_packs, contract_pack_count)?;
        initialize_from_path_with_contract_packs(contract_packs)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_get_payload_path_utf8(
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let payload_path = active_payload_path()?;
        let status = write_utf8(buffer, buffer_len, required_len, &payload_path);
        match status {
            Status::Success | Status::BufferTooSmall => Ok(status),
            Status::InvalidArgument => Err((
                Status::InvalidArgument,
                "payload path output buffer arguments are invalid".to_owned(),
            )),
            _ => unreachable!(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_create(handle: *mut u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        if handle.is_null() {
            return Err((Status::InvalidArgument, "handle output pointer is null".to_owned()));
        }

        let value = create_session_result()?;
        *handle = value;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_live_object_probe_create(
    initial_count: i64,
    com_object: *mut *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if com_object.is_null() {
            return Err((
                Status::InvalidArgument,
                "live object probe output pointer is null".to_owned(),
            ));
        }

        *com_object = create_live_object_probe(initial_count)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_live_object_probe_release(
    com_object: *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || release_live_object_probe(com_object))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_live_object_probe_unregister(
    com_object: *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || unregister_live_object_probe(com_object))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_release(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_session_result(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_command_utf8(handle: u64, command: Utf8Span, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        let command = utf8_span(command)
            .map_err(|_| (Status::InvalidArgument, "command must be UTF-8 without NUL".to_owned()))?;
        with_session_result(handle, true, |session| {
            session
                .add_command(command)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_script_utf8(handle: u64, script: Utf8Span, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        let script =
            utf8_span(script).map_err(|_| (Status::InvalidArgument, "script must be UTF-8 without NUL".to_owned()))?;
        with_session_result(handle, true, |session| {
            session
                .add_script(script)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_command_utf8_local(
    handle: u64,
    command: Utf8Span,
    use_local_scope: u32,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if use_local_scope > 1 {
            return Err((Status::InvalidArgument, "local scope must be zero or one".to_owned()));
        }
        let command = utf8_span(command)
            .map_err(|_| (Status::InvalidArgument, "command must be UTF-8 without NUL".to_owned()))?;
        with_session_result(handle, true, |session| {
            session
                .add_command_scoped(command, use_local_scope != 0)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_script_utf8_local(
    handle: u64,
    script: Utf8Span,
    use_local_scope: u32,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if use_local_scope > 1 {
            return Err((Status::InvalidArgument, "local scope must be zero or one".to_owned()));
        }
        let script =
            utf8_span(script).map_err(|_| (Status::InvalidArgument, "script must be UTF-8 without NUL".to_owned()))?;
        with_session_result(handle, true, |session| {
            session
                .add_script_scoped(script, use_local_scope != 0)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_argument_utf8(handle: u64, argument: Utf8Span, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        let argument = utf8_span(argument)
            .map_err(|_| (Status::InvalidArgument, "argument must be UTF-8 without NUL".to_owned()))?;
        with_session_result(handle, true, |session| {
            session
                .add_argument_string(argument)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_argument_value(
    handle: u64,
    value: *const DataValue,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let (kind, payload) = data_value_input(value)?;
        with_session_result(handle, true, |session| {
            session
                .add_argument_value(kind, payload)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_argument_live_object(
    handle: u64,
    com_object: *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if com_object.is_null() {
            return Err((Status::InvalidArgument, "live object probe pointer is null".to_owned()));
        }

        with_session_result(handle, true, |session| unsafe {
            session
                .add_argument_live_object(com_object)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_parameter_string_utf8(
    handle: u64,
    name: Utf8Span,
    value: Utf8Span,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "parameter name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        let value = utf8_span(value).map_err(|_| {
            (
                Status::InvalidArgument,
                "parameter value must be UTF-8 without NUL".to_owned(),
            )
        })?;
        with_session_result(handle, true, |session| {
            session
                .add_parameter_string(name, value)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_parameter_i64(
    handle: u64,
    name: Utf8Span,
    value: i64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "parameter name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        with_session_result(handle, true, |session| {
            session
                .add_parameter_long(name, value)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_parameter_value(
    handle: u64,
    name: Utf8Span,
    value: *const DataValue,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "parameter name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        let (kind, payload) = data_value_input_unvalidated(value)?;
        validate_secret_parameter_value(kind, payload)?;
        with_session_result(handle, true, |session| {
            session
                .add_parameter_value(name, kind, payload)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_parameter_switch(handle: u64, name: Utf8Span, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "parameter name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        with_session_result(handle, true, |session| {
            session
                .add_parameter_switch(name)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_input_value(
    handle: u64,
    value: *const DataValue,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let (kind, payload) = data_value_input(value)?;
        with_session_result(handle, true, |session| {
            session
                .add_input_value(kind, payload)
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_complete_input(handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        with_session_result(handle, true, |session| {
            session
                .complete_input()
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_reset_input(handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        with_session_result(handle, true, |session| {
            session.reset_input().map(|_| Status::Success).map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_add_statement(handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        with_session_result(handle, true, |session| {
            session
                .add_statement()
                .map(|_| Status::Success)
                .map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_clear(handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        with_session_result(handle, true, |session| {
            session.clear().map(|_| Status::Success).map_err(managed_failure)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_invoke_secret_result(
    handle: u64,
    expected_kind: u32,
    user_name_buffer: *mut u8,
    user_name_capacity: usize,
    user_name_length: *mut usize,
    secret_buffer: *mut u16,
    secret_capacity: usize,
    secret_length: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if expected_kind > 2
            || user_name_length.is_null()
            || secret_length.is_null()
            || (user_name_capacity != 0 && user_name_buffer.is_null())
            || (secret_capacity != 0 && secret_buffer.is_null())
        {
            return Err((
                Status::InvalidArgument,
                "secret result arguments are invalid".to_owned(),
            ));
        }

        let user_name = if user_name_capacity == 0 {
            &mut []
        } else {
            slice::from_raw_parts_mut(user_name_buffer, user_name_capacity)
        };
        let secret = if secret_capacity == 0 {
            &mut []
        } else {
            slice::from_raw_parts_mut(secret_buffer, secret_capacity)
        };
        with_session_result(handle, true, |session| {
            let (returned_user_name_length, returned_secret_length) = session
                .invoke_secret_result(expected_kind, user_name, secret)
                .map_err(|_| {
                    (
                        Status::ManagedFailure,
                        "secret-bound PowerShell operation failed".to_owned(),
                    )
                })?;
            *user_name_length = returned_user_name_length;
            *secret_length = returned_secret_length;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_invoke_credential_result(
    handle: u64,
    credential_result: *mut FfiCredentialResult,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if credential_result.is_null() {
            return Err((Status::InvalidArgument, "credential result output is null".to_owned()));
        }

        let credential_result = &mut *credential_result;
        if credential_result.size < std::mem::size_of::<FfiCredentialResult>() as u32
            || credential_result.username_capacity < 0
            || credential_result.username_capacity as usize > MAX_CREDENTIAL_TEXT_UTF8_BYTES
            || credential_result.domain_capacity < 0
            || credential_result.domain_capacity as usize > MAX_CREDENTIAL_TEXT_UTF8_BYTES
            || credential_result.password_capacity < 0
            || credential_result.password_capacity as usize > MAX_SECRET_UTF16_CODE_UNITS
            || credential_result.output_messages_capacity < 0
            || credential_result.output_messages_capacity as usize > MAX_CREDENTIAL_MESSAGE_BUFFER_BYTES
            || credential_result.error_messages_capacity < 0
            || credential_result.error_messages_capacity as usize > MAX_CREDENTIAL_MESSAGE_BUFFER_BYTES
            || credential_result.log_message_capacity < 0
            || credential_result.log_message_capacity as usize > MAX_CREDENTIAL_TEXT_UTF8_BYTES
            || (credential_result.username_capacity != 0 && credential_result.username.is_null())
            || (credential_result.domain_capacity != 0 && credential_result.domain.is_null())
            || (credential_result.password_capacity != 0 && credential_result.password.is_null())
            || (credential_result.output_messages_capacity != 0 && credential_result.output_messages.is_null())
            || (credential_result.error_messages_capacity != 0 && credential_result.error_messages.is_null())
            || (credential_result.log_message_capacity != 0 && credential_result.log_message.is_null())
        {
            return Err((
                Status::InvalidArgument,
                "credential result output is invalid".to_owned(),
            ));
        }

        with_session_result(handle, true, |session| {
            unsafe { session.invoke_credential_result(credential_result) }
                .map(|_| Status::Success)
                .map_err(|_| {
                    (
                        Status::ManagedFailure,
                        "credential result PowerShell operation failed".to_owned(),
                    )
                })
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_invoke_utf8(
    handle: u64,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        with_session_result(handle, true, |session| {
            let output = session.invoke_to_string().map_err(managed_failure)?;
            let status = write_utf8(buffer, buffer_len, required_len, &output);
            match status {
                Status::Success | Status::BufferTooSmall => Ok(status),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "output buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_get_invocation_error_count(
    handle: u64,
    error_count: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if error_count.is_null() {
            return Err((Status::InvalidArgument, "error count output pointer is null".to_owned()));
        }

        with_session_result(handle, true, |session| {
            let count = session.invocation_error_count().map_err(managed_failure)?;
            *error_count = u32::try_from(count).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed error count exceeds the ABI limit".to_owned(),
                )
            })?;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_copy_invocation_error_field_utf8(
    handle: u64,
    error_index: u32,
    field: u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let error_index = i32::try_from(error_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "error index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let field = i32::try_from(field).map_err(|_| {
            (
                Status::InvalidArgument,
                "error field exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_session_result(handle, true, |session| {
            let value = session
                .invocation_error_field(error_index, field)
                .map_err(managed_failure)?;
            let status = write_utf8(buffer, buffer_len, required_len, &value);
            match status {
                Status::Success | Status::BufferTooSmall => Ok(status),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "output buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_stop(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || stop_session_operation(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_capability_register(
    registration: *const CapabilityRegistration,
    capability_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if capability_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "capability registration output pointer is null".to_owned(),
            ));
        }
        *capability_handle = register_capabilities(registration)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_capability_release(capability_handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || release_capabilities(capability_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_set_capabilities(
    handle: u64,
    capability_handle: u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || set_capabilities(handle, capability_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_open(
    options: *const BrokerChannelOptions,
    channel_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || broker_open(options, channel_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_close(channel_handle: u64, result: *mut CallResult) -> i32 {
    v2_broker_call(result, || broker_close(channel_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_wait(
    channel_handle: u64,
    timeout_ms: u32,
    frame_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_wait(channel_handle, timeout_ms, frame_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_frame_get_info(
    frame_handle: u64,
    info: *mut BrokerFrameInfo,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_frame_get_info(frame_handle, info))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_frame_read(
    frame_handle: u64,
    buffer: *mut u8,
    capacity: u32,
    required: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_frame_read(frame_handle, buffer, capacity, required))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_frame_release(frame_handle: u64, result: *mut CallResult) -> i32 {
    v2_broker_call(result, || broker_frame_release(frame_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_observe(
    channel_handle: u64,
    correlation_id: u64,
    observation_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || {
        broker_observe(channel_handle, correlation_id, observation_handle)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_observation_get_info(
    observation_handle: u64,
    info: *mut BrokerTerminalInfo,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_observation_get_info(observation_handle, info))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_observation_wait(
    observation_handle: u64,
    timeout_ms: u32,
    info: *mut BrokerTerminalInfo,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_observation_wait(observation_handle, timeout_ms, info))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_observation_release(
    observation_handle: u64,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || broker_observation_release(observation_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_reply(
    channel_handle: u64,
    correlation_id: u64,
    body: *const u8,
    body_len: u32,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || {
        if body_len > 0 && body.is_null() {
            return Err((
                Status::InvalidArgument,
                "broker reply pointer is null for a non-empty reply".to_owned(),
            ));
        }
        let reply = if body_len == 0 {
            &[][..]
        } else {
            slice::from_raw_parts(body, body_len as usize)
        };
        broker_complete(
            channel_handle,
            correlation_id,
            BROKER_FRAME_STATE_COMPLETED,
            reply,
            String::new(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_reply_error(
    channel_handle: u64,
    correlation_id: u64,
    code: i32,
    message: Utf8Span,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || {
        let text =
            utf8_span(message).map_err(|status| (status, "broker error message is not valid UTF-8".to_owned()))?;
        if text.len() > BROKER_MAX_ERROR_MESSAGE_BYTES {
            return Err((
                Status::InvalidArgument,
                "broker error message exceeds 512 UTF-8 bytes".to_owned(),
            ));
        }
        let diagnostic = if text.is_empty() {
            format!("the broker request failed with code {code}")
        } else {
            format!("{text} (code {code})")
        };
        broker_complete(
            channel_handle,
            correlation_id,
            BROKER_FRAME_STATE_FAILED,
            &[],
            diagnostic,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_broker_cancel(
    channel_handle: u64,
    correlation_id: u64,
    result: *mut CallResult,
) -> i32 {
    v2_broker_call(result, || {
        broker_complete(
            channel_handle,
            correlation_id,
            BROKER_FRAME_STATE_CANCELLED,
            &[],
            String::new(),
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_set_broker(handle: u64, channel_handle: u64, result: *mut CallResult) -> i32 {
    v2_call(result, || set_broker(handle, channel_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_set_bridge(
    handle: u64,
    channel_handle: u64,
    binding_id: u64,
    contract_id_low: u64,
    contract_id_high: u64,
    contract_major_version: u16,
    contract_minor_version: u16,
    maximum_request_bytes: u32,
    maximum_reply_bytes: u32,
    variable_name: Utf8Span,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let variable_name = bridge_variable_name_input(variable_name)?;
        set_bridge(
            handle,
            channel_handle,
            BridgeAttachment {
                binding_id,
                contract_id_low,
                contract_id_high,
                contract_major_version,
                contract_minor_version,
                maximum_request_bytes,
                maximum_reply_bytes,
                variable_name: variable_name.to_owned(),
            },
        )
    })
}

unsafe fn bridge_variable_name_input<'a>(value: Utf8Span) -> Result<&'a str, (Status, String)> {
    if value.len > MAX_BRIDGE_VARIABLE_NAME_BYTES {
        return Err((
            Status::InvalidArgument,
            "bridge variable name exceeds 64 UTF-8 bytes".to_owned(),
        ));
    }

    let value = utf8_span(value).map_err(|status| (status, "bridge variable name is not valid UTF-8".to_owned()))?;
    if !is_valid_bridge_variable_name(value) {
        return Err((
            Status::InvalidArgument,
            "bridge variable name must be a bounded ASCII identifier".to_owned(),
        ));
    }

    Ok(value)
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_invoke(handle: u64, result_handle: *mut u64, result: *mut CallResult) -> i32 {
    v2_call(result, || {
        if result_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation result handle output pointer is null".to_owned(),
            ));
        }

        *result_handle = invoke_result(handle)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_release(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_result(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_info(
    handle: u64,
    flags: *mut u32,
    sequence_count: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if flags.is_null() || sequence_count.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation result info output pointer is null".to_owned(),
            ));
        }

        with_result(handle, |snapshot| {
            let (snapshot_flags, count) = snapshot.info().map_err(managed_failure)?;
            *flags = snapshot_flags;
            *sequence_count = u32::try_from(count).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation sequence count exceeds the ABI limit".to_owned(),
                )
            })?;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_metadata(
    handle: u64,
    state: *mut u32,
    invocation_id: *mut u64,
    had_errors: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if state.is_null() || invocation_id.is_null() || had_errors.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation metadata output pointer is null".to_owned(),
            ));
        }
        with_result(handle, |snapshot| {
            let (metadata_state, metadata_invocation_id, metadata_had_errors) =
                snapshot.metadata().map_err(managed_failure)?;
            *state = metadata_state;
            *invocation_id = metadata_invocation_id;
            *had_errors = u32::from(metadata_had_errors);
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_stream_info(
    handle: u64,
    stream: u32,
    record_count: *mut u32,
    flags: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if record_count.is_null() || flags.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation stream info output pointer is null".to_owned(),
            ));
        }

        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let (count, stream_flags) = snapshot.stream_info(stream).map_err(managed_failure)?;
            *record_count = u32::try_from(count).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation stream count exceeds the ABI limit".to_owned(),
                )
            })?;
            *flags = stream_flags;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_stream_record_info(
    handle: u64,
    stream: u32,
    record_index: u32,
    sequence: *mut u64,
    flags: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if sequence.is_null() || flags.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation stream record info output pointer is null".to_owned(),
            ));
        }

        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let record_index = i32::try_from(record_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream record index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let (record_sequence, record_flags) = snapshot
                .stream_record_info(stream, record_index)
                .map_err(managed_failure)?;
            *sequence = u64::try_from(record_sequence).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation sequence is invalid".to_owned(),
                )
            })?;
            *flags = record_flags;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_copy_stream_record_field_utf8(
    handle: u64,
    stream: u32,
    record_index: u32,
    field: u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let record_index = i32::try_from(record_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream record index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let field = i32::try_from(field).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream field exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let value = snapshot
                .stream_record_field(stream, record_index, field)
                .map_err(managed_failure)?;
            let status = write_utf8(buffer, buffer_len, required_len, &value);
            match status {
                Status::Success | Status::BufferTooSmall => Ok(status),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "invocation stream field buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_stream_totals(
    handle: u64,
    stream: u32,
    total_record_count: *mut u64,
    dropped_record_count: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if total_record_count.is_null() || dropped_record_count.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation stream totals output pointer is null".to_owned(),
            ));
        }
        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let (total, dropped) = snapshot.stream_totals(stream).map_err(managed_failure)?;
            *total_record_count = total;
            *dropped_record_count = dropped;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_stream_record_projection_info(
    handle: u64,
    stream: u32,
    record_index: u32,
    property_entry_count: *mut u32,
    dropped_property_entry_count: *mut u32,
    type_name_count: *mut u32,
    dropped_type_name_count: *mut u32,
    projection_flags: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if property_entry_count.is_null()
            || dropped_property_entry_count.is_null()
            || type_name_count.is_null()
            || dropped_type_name_count.is_null()
            || projection_flags.is_null()
        {
            return Err((
                Status::InvalidArgument,
                "invocation stream projection output pointer is null".to_owned(),
            ));
        }
        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let record_index = i32::try_from(record_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream record index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let info = snapshot
                .stream_record_projection_info(stream, record_index)
                .map_err(managed_failure)?;
            *property_entry_count = info.property_entry_count;
            *dropped_property_entry_count = info.dropped_property_entry_count;
            *type_name_count = info.type_name_count;
            *dropped_type_name_count = info.dropped_type_name_count;
            *projection_flags = info.flags;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_copy_stream_record_value(
    handle: u64,
    stream: u32,
    record_index: u32,
    value_slot: u32,
    kind: *mut u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if kind.is_null() || required_len.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation stream value output pointer is null".to_owned(),
            ));
        }
        let stream = i32::try_from(stream).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let record_index = i32::try_from(record_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream record index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        let value_slot = i32::try_from(value_slot).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation stream value slot exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let value = snapshot
                .stream_record_value(stream, record_index, value_slot)
                .map_err(managed_failure)?;
            *kind = value.kind;
            let status = write_bytes(buffer, buffer_len, required_len, &value.payload);
            match status {
                Status::Success | Status::BufferTooSmall => Ok(status),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "invocation stream value buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_result_get_sequence_record(
    handle: u64,
    sequence_index: u32,
    stream: *mut u32,
    record_index: *mut u32,
    sequence: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if stream.is_null() || record_index.is_null() || sequence.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation sequence record output pointer is null".to_owned(),
            ));
        }

        let sequence_index = i32::try_from(sequence_index).map_err(|_| {
            (
                Status::InvalidArgument,
                "invocation sequence index exceeds the managed ABI limit".to_owned(),
            )
        })?;
        with_result(handle, |snapshot| {
            let (record_stream, record_index_value, record_sequence) =
                snapshot.sequence_record(sequence_index).map_err(managed_failure)?;
            *stream = u32::try_from(record_stream).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation sequence stream is invalid".to_owned(),
                )
            })?;
            *record_index = u32::try_from(record_index_value).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation sequence record index is invalid".to_owned(),
                )
            })?;
            *sequence = u64::try_from(record_sequence).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "managed invocation sequence is invalid".to_owned(),
                )
            })?;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_invoke_async(
    handle: u64,
    operation_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if operation_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell operation handle output pointer is null".to_owned(),
            ));
        }
        *operation_handle = start_operation(handle)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_release(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_operation(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_stop(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || stop_operation(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_poll(
    handle: u64,
    operation_state: *mut u32,
    terminal_status: *mut i32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || poll_operation(handle, operation_state, terminal_status))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_wait(
    handle: u64,
    timeout_milliseconds: u32,
    operation_state: *mut u32,
    terminal_status: *mut i32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        wait_operation(handle, timeout_milliseconds, operation_state, terminal_status)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_get_result(
    handle: u64,
    result_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if result_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "invocation result handle output pointer is null".to_owned(),
            ));
        }
        *result_handle = operation_result(handle)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_read_stream_batch(
    handle: u64,
    after_sequence: u64,
    maximum_records: u32,
    batch_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if batch_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell operation stream batch handle output pointer is null".to_owned(),
            ));
        }
        *batch_handle = read_operation_stream_batch(handle, after_sequence, maximum_records)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_stream_batch_get_info(
    batch_handle: u64,
    info: *mut OperationStreamBatchInfo,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_operation_stream_batch(batch_handle, |batch| write_operation_stream_batch_info(batch, info))
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_stream_batch_get_record_info(
    batch_handle: u64,
    record_index: u32,
    stream: *mut u32,
    sequence: *mut u64,
    flags: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if stream.is_null() || sequence.is_null() || flags.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell operation stream batch record output pointer is null".to_owned(),
            ));
        }
        with_operation_stream_batch(batch_handle, |batch| {
            let record = operation_stream_batch_record(batch, record_index)?;
            *stream = record.stream;
            *sequence = record.sequence;
            *flags = record.flags;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_stream_batch_copy_record_text_utf8(
    batch_handle: u64,
    record_index: u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_operation_stream_batch(batch_handle, |batch| {
            let record = operation_stream_batch_record(batch, record_index)?;
            if record.text.len() > MAX_OPERATION_STREAM_RECORD_BYTES {
                return Err((
                    Status::ManagedFailure,
                    "PowerShell operation stream record exceeds its fixed text bound".to_owned(),
                ));
            }
            match unsafe { write_utf8(buffer, buffer_len, required_len, &record.text) } {
                Status::Success => Ok(Status::Success),
                status => Err((status, "PowerShell operation stream text buffer is invalid".to_owned())),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_operation_stream_batch_release(batch_handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_operation_stream_batch(batch_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_begin_typed_result_invocation(
    handle: u64,
    maximum_buffered_records: u32,
    maximum_page_records: u32,
    typed_result_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if typed_result_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "typed result invocation handle output pointer is null".to_owned(),
            ));
        }
        *typed_result_handle = start_typed_result_operation(handle, maximum_buffered_records, maximum_page_records)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_invocation_read_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
    page_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if page_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "typed result page handle output pointer is null".to_owned(),
            ));
        }
        *page_handle = read_typed_result_page(handle, acknowledged_through, maximum_records)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_invocation_stop(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_typed_result_operation(handle, |operation| {
            operation.request_stop();
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_invocation_release(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_typed_result_operation(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_page_get_info(
    page_handle: u64,
    info: *mut TypedResultPageInfo,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if info.is_null() || (*info).size < mem::size_of::<TypedResultPageInfo>() as u32 {
            return Err((
                Status::InvalidArgument,
                "typed result page info output is null or too small".to_owned(),
            ));
        }
        with_typed_result_page(page_handle, |page| {
            let mut flags = 0_u32;
            if page.is_terminal {
                flags |= 1;
            }
            if page.is_truncated {
                flags |= 1 << 1;
            }
            if page.is_complete {
                flags |= 1 << 2;
            }
            (*info).flags = flags;
            (*info).terminal_status = page.terminal_status;
            (*info)._reserved = 0;
            (*info).acknowledged_sequence = page.acknowledged_sequence;
            (*info).next_sequence = page.next_sequence;
            (*info).total_record_count = page.total_record_count;
            (*info).dropped_record_count = page.dropped_record_count;
            (*info).record_count = u32::try_from(page.records.len()).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "typed result page record count exceeds its bound".to_owned(),
                )
            })?;
            (*info)._reserved2 = 0;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_page_get_record_info(
    page_handle: u64,
    record_index: u32,
    sequence: *mut u64,
    kind: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if sequence.is_null() || kind.is_null() {
            return Err((
                Status::InvalidArgument,
                "typed result page record output pointer is null".to_owned(),
            ));
        }
        with_typed_result_page(page_handle, |page| {
            let record = typed_result_page_record(page, record_index)?;
            *sequence = record.sequence;
            *kind = record.kind;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_page_copy_record_value(
    page_handle: u64,
    record_index: u32,
    kind: *mut u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if kind.is_null() {
            return Err((
                Status::InvalidArgument,
                "typed result value kind output pointer is null".to_owned(),
            ));
        }
        with_typed_result_page(page_handle, |page| {
            let record = typed_result_page_record(page, record_index)?;
            if record.payload.len() > MAX_TYPED_RESULT_PAYLOAD_BYTES {
                return Err((
                    Status::ManagedFailure,
                    "typed result value exceeds its fixed payload bound".to_owned(),
                ));
            }
            *kind = record.kind;
            match write_bytes(buffer, buffer_len, required_len, &record.payload) {
                Status::Success => Ok(Status::Success),
                Status::BufferTooSmall => Ok(Status::BufferTooSmall),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "typed result value buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_typed_result_page_release(page_handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_typed_result_page(page_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_begin_observed_invocation(
    handle: u64,
    maximum_buffered_result_records: u32,
    maximum_result_page_records: u32,
    maximum_buffered_diagnostic_records: u32,
    maximum_diagnostic_page_records: u32,
    observed_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if observed_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "observed invocation handle output pointer is null".to_owned(),
            ));
        }
        *observed_handle = start_observed_invocation_operation(
            handle,
            maximum_buffered_result_records,
            maximum_result_page_records,
            maximum_buffered_diagnostic_records,
            maximum_diagnostic_page_records,
        )?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_invocation_read_result_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
    page_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if page_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "observed result page handle output pointer is null".to_owned(),
            ));
        }
        *page_handle = read_observed_result_page(handle, acknowledged_through, maximum_records)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_invocation_read_diagnostic_page(
    handle: u64,
    acknowledged_through: u64,
    maximum_records: u32,
    page_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if page_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "observed diagnostic page handle output pointer is null".to_owned(),
            ));
        }
        *page_handle = read_observed_diagnostic_page(handle, acknowledged_through, maximum_records)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_invocation_stop(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_observed_operation(handle, |operation| {
            operation.request_stop();
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_invocation_release(handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_observed_operation(handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_diagnostic_page_get_info(
    page_handle: u64,
    info: *mut TypedResultPageInfo,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if info.is_null() || (*info).size < mem::size_of::<TypedResultPageInfo>() as u32 {
            return Err((
                Status::InvalidArgument,
                "observed diagnostic page info output is null or too small".to_owned(),
            ));
        }
        with_observed_diagnostic_page(page_handle, |page| {
            let mut flags = 0_u32;
            if page.is_terminal {
                flags |= 1;
            }
            if page.is_truncated {
                flags |= 1 << 1;
            }
            if page.is_complete {
                flags |= 1 << 2;
            }
            (*info).flags = flags;
            (*info).terminal_status = page.terminal_status;
            (*info)._reserved = 0;
            (*info).acknowledged_sequence = page.acknowledged_sequence;
            (*info).next_sequence = page.next_sequence;
            (*info).total_record_count = page.total_record_count;
            (*info).dropped_record_count = page.dropped_record_count;
            (*info).record_count = u32::try_from(page.records.len()).map_err(|_| {
                (
                    Status::ManagedFailure,
                    "observed diagnostic page record count exceeds its bound".to_owned(),
                )
            })?;
            (*info)._reserved2 = 0;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_diagnostic_page_get_record_info(
    page_handle: u64,
    record_index: u32,
    stream: *mut u32,
    sequence: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if stream.is_null() || sequence.is_null() {
            return Err((
                Status::InvalidArgument,
                "observed diagnostic page record output pointer is null".to_owned(),
            ));
        }
        with_observed_diagnostic_page(page_handle, |page| {
            let record = page.records.get(record_index as usize).ok_or_else(|| {
                (
                    Status::InvalidArgument,
                    "observed diagnostic page record index is invalid".to_owned(),
                )
            })?;
            *stream = record.stream;
            *sequence = record.sequence;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_diagnostic_page_copy_record_text_utf8(
    page_handle: u64,
    record_index: u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_observed_diagnostic_page(page_handle, |page| {
            let record = page.records.get(record_index as usize).ok_or_else(|| {
                (
                    Status::InvalidArgument,
                    "observed diagnostic page record index is invalid".to_owned(),
                )
            })?;
            if record.text.len() > MAX_TYPED_RESULT_PAYLOAD_BYTES {
                return Err((
                    Status::ManagedFailure,
                    "observed diagnostic text exceeds its fixed bound".to_owned(),
                ));
            }
            match unsafe { write_utf8(buffer, buffer_len, required_len, &record.text) } {
                Status::Success => Ok(Status::Success),
                status => Err((status, "observed diagnostic text buffer is invalid".to_owned())),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_diagnostic_page_copy_record_value(
    page_handle: u64,
    record_index: u32,
    kind: *mut u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if kind.is_null() {
            return Err((
                Status::InvalidArgument,
                "observed diagnostic value kind output pointer is null".to_owned(),
            ));
        }
        with_observed_diagnostic_page(page_handle, |page| {
            let record = page.records.get(record_index as usize).ok_or_else(|| {
                (
                    Status::InvalidArgument,
                    "observed diagnostic page record index is invalid".to_owned(),
                )
            })?;
            let value = record.value.as_ref().ok_or_else(|| {
                (
                    Status::UnsupportedValue,
                    "observed diagnostic record has no copied value".to_owned(),
                )
            })?;
            if value.kind != VALUE_KIND_PROPERTY_BAG || value.payload.len() > MAX_TYPED_RESULT_PAYLOAD_BYTES {
                return Err((
                    Status::ManagedFailure,
                    "observed progress value exceeds its fixed bounds".to_owned(),
                ));
            }
            *kind = value.kind;
            match write_bytes(buffer, buffer_len, required_len, &value.payload) {
                Status::Success => Ok(Status::Success),
                Status::BufferTooSmall => Ok(Status::BufferTooSmall),
                Status::InvalidArgument => Err((
                    Status::InvalidArgument,
                    "observed progress value buffer arguments are invalid".to_owned(),
                )),
                _ => unreachable!(),
            }
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_observed_diagnostic_page_release(page_handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_observed_diagnostic_page(page_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_preflight(
    options: *const SessionOptions,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let options = session_options_input_with_current_runspace_rejection(options, false)?;
        let runtime = {
            let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.runtime.as_ref().cloned().ok_or_else(|| {
                (
                    Status::NotInitialized,
                    "PowerShell runtime is not initialized".to_owned(),
                )
            })?
        };
        let report = FfiPowerShellSession::preflight_configured(
            runtime,
            options.runspace_mode,
            options.initial_configuration,
            options.history_mode,
            options.error_preference,
            options.warning_preference,
            options.verbose_preference,
            options.debug_preference,
            options.information_preference,
            options.execution_policy,
            options.initial_variables,
            options.module_imports_payload,
            options.allowed_module_paths_payload,
            options.working_directory,
            options.environment_payload,
        )
        .map_err(managed_failure)?;
        match write_bytes(buffer, buffer_len, required_len, &report) {
            Status::Success => Ok(Status::Success),
            status => Err((status, "session preflight report buffer is invalid".to_owned())),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_create(
    options: *const SessionOptions,
    session_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if session_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session handle output pointer is null".to_owned(),
            ));
        }
        *session_handle = create_runspace_session(session_options_input(options)?)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_release(session_handle: u64, result: *mut CallResult) -> i32 {
    v2_call_allow_active_pipeline(result, || release_runspace_session(session_handle))
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_create_builder(
    session_handle: u64,
    builder_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if builder_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session builder handle output pointer is null".to_owned(),
            ));
        }
        *builder_handle = create_session_builder(session_handle)?;
        Ok(Status::Success)
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_get_snapshot(
    session_handle: u64,
    snapshot: *mut SessionSnapshot,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        with_runspace_session(session_handle, |session| {
            let snapshot_value = session.snapshot().map_err(managed_failure)?;
            write_session_snapshot(snapshot, snapshot_value)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_get_event_info(
    session_handle: u64,
    event_index: u32,
    sequence: *mut u64,
    event_state: *mut u32,
    flags: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call_allow_active_pipeline(result, || {
        if sequence.is_null() || event_state.is_null() || flags.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session event output pointer is null".to_owned(),
            ));
        }
        with_runspace_session(session_handle, |session| {
            let FfiSessionEvent {
                sequence: event_sequence,
                state,
                flags: event_flags,
            } = session.event(event_index).map_err(managed_failure)?;
            *sequence = event_sequence;
            *event_state = state;
            *flags = event_flags;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_set_variable(
    session_handle: u64,
    name: Utf8Span,
    value: *const DataValue,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session variable name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if !valid_session_name(name) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable name must be a bounded ASCII identifier".to_owned(),
            ));
        }
        let (kind, payload) = data_value_input(value)?;
        with_runspace_session_mutation(session_handle, |session| {
            session.set_variable(name, kind, payload).map_err(managed_failure)?;
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_set_live_object_variable(
    session_handle: u64,
    name: Utf8Span,
    com_object: *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if com_object.is_null() {
            return Err((
                Status::InvalidArgument,
                "live session object probe pointer is null".to_owned(),
            ));
        }
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session variable name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if !valid_session_name(name) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable name must be a bounded ASCII identifier".to_owned(),
            ));
        }
        with_runspace_session_mutation(session_handle, |session| {
            unsafe {
                session
                    .set_live_object_variable(name, com_object)
                    .map_err(managed_failure)?;
            }
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_set_live_object_contract_variable(
    session_handle: u64,
    name: Utf8Span,
    contract: *const FfiLiveObjectContractDescriptor,
    com_object: *mut std::ffi::c_void,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if com_object.is_null() {
            return Err((Status::InvalidArgument, "live object pointer is null".to_owned()));
        }
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session variable name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if !valid_session_name(name) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable name must be a bounded ASCII identifier".to_owned(),
            ));
        }
        let contract = live_object_contract_input(contract)?;
        with_runspace_session_mutation(session_handle, |session| {
            unsafe {
                session
                    .set_live_object_contract_variable(name, contract, com_object)
                    .map_err(managed_failure)?;
            }
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_remove_variable(
    session_handle: u64,
    name: Utf8Span,
    removed: *mut u32,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if removed.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable removal output pointer is null".to_owned(),
            ));
        }
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session variable name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if !valid_session_name(name) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable name must be a bounded ASCII identifier".to_owned(),
            ));
        }
        with_runspace_session_mutation(session_handle, |session| {
            *removed = u32::from(session.remove_variable(name).map_err(managed_failure)?);
            Ok(Status::Success)
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_get_variable_snapshot(
    session_handle: u64,
    name: Utf8Span,
    found: *mut u32,
    kind: *mut u32,
    buffer: *mut u8,
    buffer_len: usize,
    required_len: *mut usize,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if found.is_null() || kind.is_null() || required_len.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable snapshot output pointer is null".to_owned(),
            ));
        }
        let name = utf8_span(name).map_err(|_| {
            (
                Status::InvalidArgument,
                "PowerShell session variable name must be UTF-8 without NUL".to_owned(),
            )
        })?;
        if !valid_session_name(name) {
            return Err((
                Status::InvalidArgument,
                "PowerShell session variable name must be a bounded ASCII identifier".to_owned(),
            ));
        }
        *found = 0;
        *kind = 0;
        *required_len = 0;
        with_runspace_session_mutation(session_handle, |session| {
            let Some(FfiSnapshotValue {
                kind: value_kind,
                payload,
            }) = session.variable_snapshot(name).map_err(managed_failure)?
            else {
                return Ok(Status::Success);
            };
            *found = 1;
            *kind = value_kind;
            Ok(write_bytes(buffer, buffer_len, required_len, &payload))
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn multi_pwsh_session_pool_create(
    options: *const SessionPoolOptions,
    pool_handle: *mut u64,
    result: *mut CallResult,
) -> i32 {
    v2_call(result, || {
        if pool_handle.is_null() {
            return Err((
                Status::InvalidArgument,
                "PowerShell session pool handle output pointer is null".to_owned(),
            ));
        }
        validate_pool_options(options)?;
        *pool_handle = 0;
        Err((
            Status::UnsupportedCapability,
            "PowerShell session pools are intentionally unsupported: a shared in-process runspace cannot safely provide concurrent pool semantics."
                .to_owned(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static EMPTY_VALUE_CONTAINER: [u8; 4] = [0; 4];

    fn empty_data_value(kind: u32) -> DataValue {
        DataValue {
            size: mem::size_of::<DataValue>() as u32,
            kind,
            flags: 0,
            _reserved: 0,
            data: EMPTY_VALUE_CONTAINER.as_ptr(),
            data_len: EMPTY_VALUE_CONTAINER.len(),
        }
    }

    fn empty_session_options() -> SessionOptions {
        SessionOptions {
            size: mem::size_of::<SessionOptions>() as u32,
            runspace_mode: 1,
            initial_configuration: 0,
            history_mode: 0,
            error_preference: 0,
            warning_preference: 0,
            verbose_preference: 0,
            debug_preference: 0,
            information_preference: 0,
            flags: 0,
            _reserved: 0,
            allowed_module_path: Utf8Span {
                data: std::ptr::null(),
                len: 0,
            },
            execution_policy: 0,
            configuration_flags: 0,
            initial_variables: empty_data_value(VALUE_KIND_PROPERTY_BAG),
            module_imports: empty_data_value(VALUE_KIND_ARRAY),
            allowed_module_paths: empty_data_value(VALUE_KIND_ARRAY),
            working_directory: Utf8Span {
                data: std::ptr::null(),
                len: 0,
            },
            environment: empty_data_value(VALUE_KIND_PROPERTY_BAG),
        }
    }

    #[test]
    fn live_stream_batches_enforce_cursor_limit_release_and_terminal_metadata() {
        let mut stream = OperationStreamState {
            records: VecDeque::with_capacity(MAX_OPERATION_STREAM_RECORDS),
            next_sequence: 1,
            total_record_count: 0,
            dropped_record_count: 0,
            source_dropped_record_count: 0,
        };
        for source_sequence in 1..=33_u64 {
            stream
                .capture_batch(FfiLiveStreamBatch {
                    next_sequence: source_sequence,
                    total_record_count: source_sequence,
                    lost_record_count: 0,
                    records: vec![FfiLiveStreamRecord {
                        stream: 0,
                        sequence: source_sequence,
                        text: source_sequence.to_string(),
                        flags: 0,
                    }],
                })
                .unwrap();
        }

        let batch = stream
            .read_batch(OperationState::Cancelled, Status::OperationCancelled, 0, 2)
            .unwrap();
        assert_eq!(batch.records.len(), 2);
        assert_eq!(batch.records[0].sequence, 2);
        assert_eq!(batch.records[1].sequence, 3);
        assert_eq!(batch.next_sequence, 3);
        assert_eq!(batch.lost_record_count, 1);
        assert_eq!(batch.dropped_record_count, 1);
        assert_eq!(batch.state, OperationState::Cancelled);
        assert_eq!(batch.terminal_status, Status::OperationCancelled);
        assert!(matches!(
            stream.read_batch(OperationState::Running, Status::Success, 3, 0),
            Err((Status::InvalidArgument, _))
        ));
        assert!(matches!(
            stream.read_batch(OperationState::Running, Status::Success, 34, 1),
            Err((Status::InvalidArgument, _))
        ));

        let _scope = TEST_PIPELINE_SCOPE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let handle = u64::MAX - 91;
        state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .operation_stream_batches
            .insert(handle, Arc::new(batch));
        assert_eq!(release_operation_stream_batch(handle), Ok(Status::Success));
        assert_eq!(
            release_operation_stream_batch(handle),
            Err((
                Status::InvalidHandle,
                "PowerShell operation stream batch handle is invalid".to_owned()
            ))
        );
    }

    #[test]
    fn live_stream_record_text_is_utf8_bounded() {
        let mut flags = 0;
        let text = bound_operation_stream_text("é".repeat(MAX_OPERATION_STREAM_RECORD_BYTES), &mut flags);

        assert!(text.len() <= MAX_OPERATION_STREAM_RECORD_BYTES);
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
        assert_ne!(flags & OPERATION_STREAM_RECORD_TEXT_TRUNCATED, 0);
    }

    #[test]
    fn typed_result_page_exports_preserve_metadata_and_release_handles() {
        let _scope = TEST_PIPELINE_SCOPE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let handle = u64::MAX - 92;
        let page = FfiTypedResultPage {
            acknowledged_sequence: 3,
            next_sequence: 5,
            total_record_count: 5,
            dropped_record_count: 0,
            terminal_status: 0,
            is_terminal: true,
            is_truncated: false,
            is_complete: true,
            records: vec![
                FfiTypedResultRecord {
                    sequence: 4,
                    kind: 4,
                    payload: 42_i32.to_le_bytes().to_vec(),
                },
                FfiTypedResultRecord {
                    sequence: 5,
                    kind: 3,
                    payload: vec![1],
                },
            ],
        };
        state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .typed_result_pages
            .insert(handle, Arc::new(page));

        let mut diagnostic = [0_u8; 256];
        let mut result = v2_call_result(&mut diagnostic);
        let mut info = TypedResultPageInfo {
            size: mem::size_of::<TypedResultPageInfo>() as u32,
            flags: 0,
            terminal_status: 0,
            _reserved: 0,
            acknowledged_sequence: 0,
            next_sequence: 0,
            total_record_count: 0,
            dropped_record_count: 0,
            record_count: 0,
            _reserved2: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_typed_result_page_get_info(handle, &mut info, &mut result) },
            Status::Success.value()
        );
        assert_eq!(info.flags, 0b101);
        assert_eq!(info.acknowledged_sequence, 3);
        assert_eq!(info.next_sequence, 5);
        assert_eq!(info.total_record_count, 5);
        assert_eq!(info.dropped_record_count, 0);
        assert_eq!(info.record_count, 2);

        let mut sequence = 0;
        let mut kind = 0;
        result = v2_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_typed_result_page_get_record_info(handle, 0, &mut sequence, &mut kind, &mut result) },
            Status::Success.value()
        );
        assert_eq!(sequence, 4);
        assert_eq!(kind, 4);

        let mut required_len = 0;
        result = v2_call_result(&mut diagnostic);
        assert_eq!(
            unsafe {
                multi_pwsh_typed_result_page_copy_record_value(
                    handle,
                    0,
                    &mut kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required_len,
                    &mut result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!(required_len, mem::size_of::<i32>());

        result = v2_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_typed_result_page_release(handle, &mut result) },
            Status::Success.value()
        );
        result = v2_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_typed_result_page_release(handle, &mut result) },
            Status::InvalidHandle.value()
        );
    }

    fn append_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn encode_nested(output: &mut Vec<u8>, kind: u32, value: &[u8]) {
        append_u32(output, kind);
        append_u32(output, value.len() as u32);
        output.extend_from_slice(value);
    }

    fn encode_array(values: Vec<(u32, Vec<u8>)>) -> Vec<u8> {
        let mut output = Vec::new();
        append_u32(&mut output, values.len() as u32);
        for (kind, value) in values {
            encode_nested(&mut output, kind, &value);
        }
        output
    }

    fn encode_property_bag(values: Vec<(&str, u32, Vec<u8>)>) -> Vec<u8> {
        let mut output = Vec::new();
        append_u32(&mut output, values.len() as u32);
        for (name, kind, value) in values {
            append_u32(&mut output, name.len() as u32);
            output.extend_from_slice(name.as_bytes());
            encode_nested(&mut output, kind, &value);
        }
        output
    }

    fn encode_unsigned(value: u64) -> Vec<u8> {
        value.to_le_bytes().to_vec()
    }

    fn encode_capability_definition(name: &str) -> Vec<u8> {
        encode_property_bag(vec![
            ("name", VALUE_KIND_STRING, name.as_bytes().to_vec()),
            ("permissions", VALUE_KIND_UNSIGNED_INTEGER, encode_unsigned(1)),
            ("maximumInputBytes", VALUE_KIND_UNSIGNED_INTEGER, encode_unsigned(64)),
            ("maximumOutputBytes", VALUE_KIND_UNSIGNED_INTEGER, encode_unsigned(64)),
            (
                "deadlineMilliseconds",
                VALUE_KIND_UNSIGNED_INTEGER,
                encode_unsigned(100),
            ),
            ("arguments", VALUE_KIND_ARRAY, encode_array(vec![])),
            (
                "responseKinds",
                VALUE_KIND_ARRAY,
                encode_array(vec![(
                    VALUE_KIND_UNSIGNED_INTEGER,
                    encode_unsigned(VALUE_KIND_STRING as u64),
                )]),
            ),
        ])
    }

    fn encode_capability_registration(names: &[&str]) -> Vec<u8> {
        encode_property_bag(vec![
            (
                "protocol",
                VALUE_KIND_UNSIGNED_INTEGER,
                encode_unsigned(CAPABILITY_REGISTRATION_VERSION as u64),
            ),
            (
                "capabilities",
                VALUE_KIND_ARRAY,
                encode_array(
                    names
                        .iter()
                        .map(|name| (VALUE_KIND_PROPERTY_BAG, encode_capability_definition(name)))
                        .collect(),
                ),
            ),
        ])
    }

    #[test]
    fn write_utf8_reports_required_length_before_copying() {
        let mut required = 0;
        let status = unsafe { write_utf8(std::ptr::null_mut(), 0, &mut required, "héllo") };
        assert!(matches!(status, Status::BufferTooSmall));
        assert_eq!(required, "héllo".len());
    }

    #[test]
    fn write_utf8_copies_complete_utf8_values() {
        let mut output = [0_u8; 8];
        let mut required = 0;
        let status = unsafe { write_utf8(output.as_mut_ptr(), output.len(), &mut required, "héllo") };
        assert!(matches!(status, Status::Success));
        assert_eq!(&output[..required], "héllo".as_bytes());
    }

    #[test]
    fn capability_schema_parser_rejects_noncanonical_and_duplicate_definitions() {
        let valid = encode_capability_registration(&["example.get-label"]);
        let definitions = parse_capability_definitions(VALUE_KIND_PROPERTY_BAG, &valid).unwrap();
        assert_eq!(definitions.len(), 1);
        assert!(definitions.contains_key("example.get-label"));

        let duplicate = encode_capability_registration(&["example.get-label", "example.get-label"]);
        assert!(matches!(
            parse_capability_definitions(VALUE_KIND_PROPERTY_BAG, &duplicate),
            Err((Status::InvalidArgument, _))
        ));

        let noncanonical = encode_capability_registration(&["example"]);
        assert!(matches!(
            parse_capability_definitions(VALUE_KIND_PROPERTY_BAG, &noncanonical),
            Err((Status::InvalidArgument, _))
        ));
        assert!(matches!(
            parse_capability_definitions(VALUE_KIND_STRING, b"not-a-registration"),
            Err((Status::InvalidArgument, _))
        ));
    }

    #[test]
    fn callback_scope_rejects_reentrant_ffi_calls_before_runtime_access() {
        let _callback_scope = CapabilityCallbackScope::enter();
        let mut diagnostic = [0_u8; 64];
        let mut result = CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: diagnostic.as_mut_ptr(),
            diagnostic_capacity: diagnostic.len(),
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        let mut handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut handle, &mut result) },
            Status::Backpressure.value()
        );
        assert_eq!(result.status, Status::Backpressure.value());
        assert_eq!(handle, 0);
    }

    // -----------------------------------------------------------------
    // Duplex Broker Channel acceptance tests
    // -----------------------------------------------------------------

    fn broker_call_result(diagnostic: &mut [u8]) -> CallResult {
        CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: diagnostic.as_mut_ptr(),
            diagnostic_capacity: diagnostic.len(),
            diagnostic_required: 0,
            diagnostic_written: 0,
        }
    }

    fn broker_default_options() -> BrokerChannelOptions {
        BrokerChannelOptions {
            size: mem::size_of::<BrokerChannelOptions>() as u32,
            abi_version: BROKER_ABI_V1,
            max_inflight: 4,
            max_body_bytes: 1024,
            default_deadline_ms: 5_000,
            flags: 0,
        }
    }

    fn bridge_close_body() -> Vec<u8> {
        let mut close = vec![0_u8; BRIDGE_ROUTE_HEADER_SIZE + BRIDGE_REQUEST_HEADER_SIZE];
        close[0] = 1;
        close[BRIDGE_ROUTE_HEADER_SIZE] = BRIDGE_PROTOCOL_VERSION;
        close[BRIDGE_ROUTE_HEADER_SIZE + 1] = BRIDGE_CLOSE_FRAME_KIND;
        close[BRIDGE_ROUTE_HEADER_SIZE + 16..BRIDGE_ROUTE_HEADER_SIZE + 24].copy_from_slice(&1_u64.to_le_bytes());
        close[BRIDGE_ROUTE_HEADER_SIZE + 24..BRIDGE_ROUTE_HEADER_SIZE + 28].copy_from_slice(&1_u32.to_le_bytes());
        close
    }

    fn open_broker_channel(options: &BrokerChannelOptions) -> u64 {
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut channel = 0_u64;
        assert_eq!(
            unsafe { multi_pwsh_broker_open(options, &mut channel, &mut result) },
            Status::Success.value()
        );
        assert_ne!(channel, 0);
        channel
    }

    /// Drives the payload half directly, exactly as the trampoline does.
    fn broker_enqueue(channel: u64, generation: u64, kind: u32, body: &[u8]) -> (i32, Vec<u8>, String) {
        let mut diagnostic = [0_u8; 512];
        let mut result = broker_call_result(&mut diagnostic);
        let mut reply = [0_u8; 1024];
        let mut reply_len = 0_u32;
        let mut correlation = 0_u64;
        let status = unsafe {
            broker_enqueue_and_wait(
                channel,
                generation,
                kind,
                0,
                0,
                0,
                body.as_ptr(),
                body.len() as u32,
                &mut correlation,
                reply.as_mut_ptr(),
                reply.len() as u32,
                &mut reply_len,
                &mut result,
            )
        };
        let text = String::from_utf8_lossy(&diagnostic[..result.diagnostic_written]).to_string();
        (status, reply[..reply_len as usize].to_vec(), text)
    }

    /// Registers an active attachment so the trampolines resolve, without
    /// requiring a real payload runtime or session.
    fn broker_test_attachment(channel_handle: u64) -> Arc<BrokerAttachment> {
        let channel = broker_channel_for(channel_handle).expect("channel");
        let attachment = Arc::new(BrokerAttachment {
            channel_handle,
            channel,
            generation: NEXT_BROKER_GENERATION.fetch_add(1, Ordering::AcqRel),
            active: AtomicBool::new(true),
            cancelled: AtomicBool::new(false),
            bridge: None,
        });
        broker_register_attachment(&attachment);
        attachment
    }

    fn broker_release_test_attachment(attachment: &Arc<BrokerAttachment>) {
        attachment.active.store(false, Ordering::Release);
        broker_unregister_attachment(attachment.generation);
    }

    fn broker_wait_for_frame(channel: u64, timeout_ms: u32) -> u64 {
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut frame = 0_u64;
        let status = unsafe { multi_pwsh_broker_wait(channel, timeout_ms, &mut frame, &mut result) };
        assert_eq!(status, Status::Success.value());
        frame
    }

    fn broker_attach_consumer(channel: u64) {
        assert_eq!(broker_wait_for_frame(channel, 0), 0);
    }

    fn broker_close_channel(channel: u64) {
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        unsafe { multi_pwsh_broker_close(channel, &mut result) };
    }

    #[test]
    fn broker_open_rejects_invalid_options() {
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut channel = 0_u64;

        let mut options = broker_default_options();
        options.abi_version = 99;
        assert_eq!(
            unsafe { multi_pwsh_broker_open(&options, &mut channel, &mut result) },
            Status::InvalidArgument.value()
        );

        #[repr(C)]
        struct ShortBrokerOptions {
            size: u32,
        }

        let mut options = ShortBrokerOptions { size: 4 };
        assert_eq!(
            unsafe {
                multi_pwsh_broker_open(
                    (&mut options as *mut ShortBrokerOptions).cast::<BrokerChannelOptions>(),
                    &mut channel,
                    &mut result,
                )
            },
            Status::InvalidArgument.value()
        );

        #[repr(C, align(8))]
        struct ShortBrokerFrameInfo {
            size: u32,
            abi_version: u32,
        }

        let mut info = ShortBrokerFrameInfo {
            size: 8,
            abi_version: BROKER_ABI_V1,
        };
        assert_eq!(
            unsafe {
                multi_pwsh_broker_frame_get_info(
                    0,
                    (&mut info as *mut ShortBrokerFrameInfo).cast::<BrokerFrameInfo>(),
                    &mut result,
                )
            },
            Status::InvalidArgument.value()
        );

        let mut options = broker_default_options();
        options.max_inflight = BROKER_MAX_INFLIGHT_LIMIT + 1;
        assert_eq!(
            unsafe { multi_pwsh_broker_open(&options, &mut channel, &mut result) },
            Status::InvalidArgument.value()
        );

        let mut options = broker_default_options();
        options.max_body_bytes = BROKER_MAX_BODY_BYTES_LIMIT + 1;
        assert_eq!(
            unsafe { multi_pwsh_broker_open(&options, &mut channel, &mut result) },
            Status::InvalidArgument.value()
        );

        let mut options = broker_default_options();
        options.flags = 1;
        assert_eq!(
            unsafe { multi_pwsh_broker_open(&options, &mut channel, &mut result) },
            Status::InvalidArgument.value()
        );
        assert_eq!(channel, 0);
    }

    #[test]
    fn broker_wire_structures_have_the_documented_fixed_layout() {
        assert_eq!(mem::size_of::<BrokerChannelOptions>(), 24);
        assert_eq!(mem::size_of::<BrokerFrameInfo>(), 56);
        assert_eq!(mem::align_of::<BrokerFrameInfo>(), 8);
    }

    #[test]
    fn broker_request_reaches_a_pump_and_the_reply_resumes_the_payload() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 7, b"ping"));

        let frame = broker_wait_for_frame(channel, 5_000);
        assert_ne!(frame, 0);

        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) },
            Status::Success.value()
        );
        assert_eq!(info.kind, 7);
        assert_eq!(info.body_length, 4);
        assert_eq!(info.state, BROKER_FRAME_STATE_DISPATCHED);
        assert!(info.flags & BROKER_FRAME_FLAG_MUTATING != 0);

        let mut body = [0_u8; 16];
        let mut required = 0_u32;
        assert_eq!(
            unsafe {
                multi_pwsh_broker_frame_read(frame, body.as_mut_ptr(), body.len() as u32, &mut required, &mut result)
            },
            Status::Success.value()
        );
        assert_eq!(&body[..required as usize], b"ping");

        let correlation = info.correlation_id;
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );

        // Releasing the delivery handle must NOT be abandonment: the reply
        // still resumes the blocked payload.
        assert_eq!(
            unsafe { multi_pwsh_broker_reply(channel, correlation, b"pong".as_ptr(), 4, &mut result) },
            Status::Success.value()
        );

        let (status, reply, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::Success.value());
        assert_eq!(reply, b"pong");

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_terminal_observation_survives_delivery_release_and_reports_cancellation() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 1, b"observe"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut frame_info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_get_info(frame, &mut frame_info, &mut result) },
            Status::Success.value()
        );

        let mut observation = 0_u64;
        assert_eq!(
            unsafe { multi_pwsh_broker_observe(channel, frame_info.correlation_id, &mut observation, &mut result,) },
            Status::Success.value()
        );
        assert_ne!(observation, 0);

        let mut terminal = BrokerTerminalInfo {
            size: mem::size_of::<BrokerTerminalInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerTerminalInfo::default()
        };
        assert_eq!(
            unsafe { multi_pwsh_broker_observation_get_info(observation, &mut terminal, &mut result) },
            Status::Success.value()
        );
        assert_eq!(terminal.state, BROKER_FRAME_STATE_DISPATCHED);
        assert_eq!(terminal.terminal_status, Status::OperationNotTerminal.value());
        assert_eq!(terminal.terminal_epoch_ms, 0);

        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_broker_cancel(channel, frame_info.correlation_id, &mut result) },
            Status::Success.value()
        );

        let observed_from_worker = std::thread::spawn(move || {
            let mut diagnostic = [0_u8; 256];
            let mut result = broker_call_result(&mut diagnostic);
            let mut terminal = BrokerTerminalInfo {
                size: mem::size_of::<BrokerTerminalInfo>() as u32,
                abi_version: BROKER_ABI_V1,
                ..BrokerTerminalInfo::default()
            };
            let status = unsafe { multi_pwsh_broker_observation_wait(observation, 1_000, &mut terminal, &mut result) };
            (status, terminal)
        })
        .join()
        .expect("observation worker");
        assert_eq!(observed_from_worker.0, Status::Success.value());
        assert_eq!(observed_from_worker.1.state, BROKER_FRAME_STATE_CANCELLED);
        assert_eq!(
            observed_from_worker.1.terminal_status,
            Status::OperationCancelled.value()
        );

        assert_eq!(
            unsafe { multi_pwsh_broker_observation_release(observation, &mut result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_broker_observation_release(observation, &mut result) },
            Status::InvalidHandle.value()
        );
        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::OperationCancelled.value());

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_terminal_observation_is_bounded_and_remains_readable_after_close() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 1, b"close"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut frame_info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_get_info(frame, &mut frame_info, &mut result) },
            Status::Success.value()
        );

        let mut observations = Vec::new();
        for _ in 0..BROKER_MAX_OBSERVERS_PER_FRAME {
            let mut observation = 0_u64;
            assert_eq!(
                unsafe { multi_pwsh_broker_observe(channel, frame_info.correlation_id, &mut observation, &mut result) },
                Status::Success.value()
            );
            observations.push(observation);
        }
        let mut overflow = 0_u64;
        assert_eq!(
            unsafe { multi_pwsh_broker_observe(channel, frame_info.correlation_id, &mut overflow, &mut result) },
            Status::Backpressure.value()
        );
        assert_eq!(overflow, 0);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );

        broker_close_channel(channel);
        for observation in observations {
            let mut terminal = BrokerTerminalInfo {
                size: mem::size_of::<BrokerTerminalInfo>() as u32,
                abi_version: BROKER_ABI_V1,
                ..BrokerTerminalInfo::default()
            };
            assert_eq!(
                unsafe { multi_pwsh_broker_observation_get_info(observation, &mut terminal, &mut result) },
                Status::Success.value()
            );
            assert_eq!(terminal.state, BROKER_FRAME_STATE_ABORTED);
            assert_eq!(terminal.terminal_status, Status::BrokerClosed.value());
            assert_eq!(
                unsafe { multi_pwsh_broker_observation_release(observation, &mut result) },
                Status::Success.value()
            );
        }

        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::BrokerClosed.value());
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_duplicate_and_late_replies_are_rejected() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 1, b"x"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) };
        let correlation = info.correlation_id;
        unsafe { multi_pwsh_broker_frame_release(frame, &mut result) };

        assert_eq!(
            unsafe { multi_pwsh_broker_reply(channel, correlation, std::ptr::null(), 0, &mut result) },
            Status::Success.value()
        );
        // Second reply must not resurrect or leak into another frame.
        assert_eq!(
            unsafe { multi_pwsh_broker_reply(channel, correlation, std::ptr::null(), 0, &mut result) },
            Status::BrokerInvalidTerminalState.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_broker_cancel(channel, correlation, &mut result) },
            Status::BrokerInvalidTerminalState.value()
        );
        // Unknown correlation is equally deterministic.
        assert_eq!(
            unsafe { multi_pwsh_broker_reply(channel, correlation + 4_242, std::ptr::null(), 0, &mut result) },
            Status::BrokerInvalidTerminalState.value()
        );

        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::Success.value());
        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_cancellation_after_dispatch_wakes_the_payload() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 2, b"y"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) };
        unsafe { multi_pwsh_broker_frame_release(frame, &mut result) };
        assert_eq!(
            unsafe { multi_pwsh_broker_cancel(channel, info.correlation_id, &mut result) },
            Status::Success.value()
        );

        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::OperationCancelled.value());
        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_reply_error_surfaces_a_bounded_diagnostic() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 3, b"z"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) };
        unsafe { multi_pwsh_broker_frame_release(frame, &mut result) };
        let message = b"handler refused";
        assert_eq!(
            unsafe {
                multi_pwsh_broker_reply_error(
                    channel,
                    info.correlation_id,
                    9,
                    Utf8Span {
                        data: message.as_ptr(),
                        len: message.len(),
                    },
                    &mut result,
                )
            },
            Status::Success.value()
        );

        let (status, _, text) = payload.join().expect("payload thread");
        assert_eq!(status, Status::ManagedFailure.value());
        assert!(text.contains("handler refused"), "diagnostic was {}", text);
        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_request_times_out_before_its_absolute_deadline_elapses_twice() {
        let mut options = broker_default_options();
        options.default_deadline_ms = 150;
        let channel = open_broker_channel(&options);
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        // Attach a pump so the request is queued rather than refused.
        let frame = broker_wait_for_frame(channel, 50);
        assert_eq!(frame, 0);

        let started = Instant::now();
        let (status, _, _) = broker_enqueue(channel, generation, 4, b"t");
        assert_eq!(status, Status::BrokerTimeout.value());
        assert!(started.elapsed() < Duration::from_secs(5), "timeout was not bounded");

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_request_without_a_pump_fails_fast_rather_than_hanging() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        let started = Instant::now();
        let (status, _, _) = broker_enqueue(channel, generation, 5, b"n");
        assert_eq!(status, Status::BrokerNoConsumer.value());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "no-consumer did not fail fast"
        );
        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_close_wakes_every_blocked_payload_request() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);

        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 6, b"c"));
        // Let the request reach the queue, then close.
        std::thread::sleep(Duration::from_millis(100));
        broker_close_channel(channel);

        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::BrokerClosed.value());
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_saturated_channel_reports_busy_and_serializes_one_mutating_key() {
        let mut options = broker_default_options();
        options.max_inflight = 1;
        let channel = open_broker_channel(&options);
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        assert_eq!(broker_wait_for_frame(channel, 50), 0);

        let first = std::thread::spawn(move || broker_enqueue(channel, generation, 8, b"1"));
        std::thread::sleep(Duration::from_millis(100));
        // A second request cannot be admitted: the bound counts every
        // non-terminal correlation, dispatched or not.
        let (status, _, _) = broker_enqueue(channel, generation, 8, b"2");
        assert_eq!(status, Status::BrokerBusy.value());

        broker_close_channel(channel);
        let (first_status, _, _) = first.join().expect("payload thread");
        assert_eq!(first_status, Status::BrokerClosed.value());
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_rejects_a_stale_generation_and_an_unknown_channel() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        let (status, _, _) = broker_enqueue(channel, generation + 1, 9, b"s");
        assert_eq!(status, Status::InvalidHandle.value());

        let (status, _, _) = broker_enqueue(channel + 9_999, generation, 9, b"s");
        assert_eq!(status, Status::InvalidHandle.value());

        // Deactivating the attachment revokes a captured $DpsBroker.
        broker_release_test_attachment(&attachment);
        let (status, _, _) = broker_enqueue(channel, generation, 9, b"s");
        assert_eq!(status, Status::InvalidHandle.value());

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_frame_handles_are_channel_scoped_and_single_release() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 10, b"h"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );
        // A released handle is stale forever; it never resolves to another frame.
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::InvalidHandle.value()
        );
        let mut required = 0_u32;
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_read(frame, std::ptr::null_mut(), 0, &mut required, &mut result) },
            Status::InvalidHandle.value()
        );

        broker_close_channel(channel);
        let _ = payload.join();
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_dispatch_violation_rejects_non_broker_calls_while_a_frame_is_held() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 11, b"d"));

        let frame = broker_wait_for_frame(channel, 5_000);
        assert_ne!(frame, 0);

        // While this thread holds a delivery handle, a normal FFI export must
        // fail deterministically instead of deadlocking or being allowed.
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut builder = 0_u64;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut builder, &mut result) },
            Status::BrokerDispatchViolation.value()
        );
        assert_eq!(builder, 0);

        // Broker data-plane calls remain legal from the same thread.
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );
        // After release the restriction is lifted.
        assert_ne!(
            unsafe { multi_pwsh_create(&mut builder, &mut result) },
            Status::BrokerDispatchViolation.value()
        );
        if builder != 0 {
            unsafe { multi_pwsh_release(builder, &mut result) };
        }

        broker_close_channel(channel);
        let _ = payload.join();
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_release_from_another_thread_is_a_dispatch_violation() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_attach_consumer(channel);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 12, b"o"));

        let frame = broker_wait_for_frame(channel, 5_000);
        let foreign = std::thread::spawn(move || {
            let mut diagnostic = [0_u8; 256];
            let mut result = broker_call_result(&mut diagnostic);
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) }
        })
        .join()
        .expect("foreign thread");
        assert_eq!(foreign, Status::BrokerDispatchViolation.value());

        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );

        broker_close_channel(channel);
        let _ = payload.join();
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_one_way_events_never_block_a_producer() {
        let mut options = broker_default_options();
        options.max_inflight = 2;
        let channel = open_broker_channel(&options);
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        assert_eq!(broker_wait_for_frame(channel, 50), 0);

        let started = Instant::now();
        for index in 0..12_u32 {
            let mut diagnostic = [0_u8; 256];
            let mut result = broker_call_result(&mut diagnostic);
            let body = index.to_le_bytes();
            let status =
                unsafe { broker_post(channel, generation, 1, 0, body.as_ptr(), body.len() as u32, &mut result) };
            assert_eq!(status, Status::Success.value(), "post {} applied backpressure", index);
        }
        assert!(started.elapsed() < Duration::from_secs(1), "posting blocked");

        // The loss is reported rather than hidden.
        let frame = broker_wait_for_frame(channel, 1_000);
        assert_ne!(frame, 0);
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) };
        assert!(info.flags & BROKER_FRAME_FLAG_ONE_WAY != 0);
        assert!(info.dropped_before > 0, "coalesced events were not reported");
        unsafe { multi_pwsh_broker_frame_release(frame, &mut result) };

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_cancellation_closes_payload_admission_before_queued_frames_finish() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        broker_cancel_attachment(&attachment);

        let (status, _, diagnostic) = broker_enqueue(channel, generation, 31, b"request");
        assert_eq!(status, Status::OperationCancelled.value());
        assert!(diagnostic.contains("cancelled"));

        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let status = unsafe {
            broker_post(
                channel,
                generation,
                32,
                0,
                b"event".as_ptr(),
                b"event".len() as u32,
                &mut result,
            )
        };
        assert_eq!(status, Status::OperationCancelled.value());
        assert!(String::from_utf8_lossy(&diagnostic[..result.diagnostic_written]).contains("cancelled"));
        assert_eq!(broker_wait_for_frame(channel, 50), 0);

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_cancellation_preserves_an_admitted_bridge_close_frame() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        assert_eq!(broker_wait_for_frame(channel, 50), 0);

        let close = bridge_close_body();
        let payload = std::thread::spawn({
            let close = close.clone();
            move || broker_enqueue(channel, generation, BRIDGE_BROKER_REQUEST_KIND, &close)
        });

        for _ in 0..100 {
            if attachment
                .channel
                .lock()
                .frames
                .values()
                .any(|frame| frame.generation == generation)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(attachment
            .channel
            .lock()
            .frames
            .values()
            .any(|frame| frame.generation == generation));

        broker_cancel_attachment(&attachment);
        let frame = broker_wait_for_frame(channel, 5_000);
        assert_ne!(frame, 0, "cancellation must preserve generated bridge Close");
        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );

        broker_close_channel(channel);
        let (status, _, _) = payload.join().expect("payload");
        assert_eq!(status, Status::BrokerClosed.value());
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_cancellation_admits_a_fixed_cleanup_close_frame() {
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;
        assert_eq!(broker_wait_for_frame(channel, 50), 0);
        broker_cancel_attachment(&attachment);

        let payload = std::thread::spawn({
            let close = bridge_close_body();
            move || broker_enqueue(channel, generation, BRIDGE_BROKER_REQUEST_KIND, &close)
        });
        let frame = broker_wait_for_frame(channel, 5_000);
        assert_ne!(frame, 0, "cancellation must admit generated bridge Close");

        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        let mut info = BrokerFrameInfo {
            size: mem::size_of::<BrokerFrameInfo>() as u32,
            abi_version: BROKER_ABI_V1,
            ..BrokerFrameInfo::default()
        };
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) },
            Status::Success.value()
        );
        assert_eq!(info.kind, BRIDGE_BROKER_REQUEST_KIND);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_broker_reply(channel, info.correlation_id, std::ptr::null(), 0, &mut result) },
            Status::Success.value()
        );
        let (status, reply, _) = payload.join().expect("payload");
        assert_eq!(status, Status::Success.value());
        assert!(reply.is_empty());

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_generation_is_invalidated_when_its_invocation_terminates() {
        // Regression: the generation must be revoked on the real terminal
        // paths, not only on pre-invocation failure paths. Otherwise script
        // that captured `$DpsBroker` reaches the channel in a later invocation
        // and BROKER_ATTACHMENTS grows without bound.
        let channel = open_broker_channel(&broker_default_options());
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        assert!(
            broker_attachment_for(channel, generation).is_some(),
            "the attachment should resolve while its invocation is active"
        );

        // This is exactly what every terminal invocation path now performs.
        broker_deactivate_attachment(&attachment);

        assert!(
            broker_attachment_for(channel, generation).is_none(),
            "the broker generation survived its invocation"
        );
        let (status, _, _) = broker_enqueue(channel, generation, 1, b"stale");
        assert_eq!(
            status,
            Status::InvalidHandle.value(),
            "a captured $DpsBroker reached the channel after its invocation ended"
        );
        assert!(
            BROKER_ATTACHMENTS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .map(|map| !map.contains_key(&generation))
                .unwrap_or(true),
            "the attachment registry retained a terminated generation"
        );

        broker_close_channel(channel);
    }

    #[test]
    fn broker_expired_one_way_events_do_not_accumulate_without_a_pump() {
        // Regression: a one-way frame has no blocked raiser to collect it, so
        // an expired event nobody received must be dropped by the expiry sweep
        // rather than retained for the life of the channel.
        let mut options = broker_default_options();
        options.max_inflight = 8;
        options.default_deadline_ms = 60;
        let channel = open_broker_channel(&options);
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        // Attach a pump once so posting is accepted, then stop consuming.
        assert_eq!(broker_wait_for_frame(channel, 10), 0);

        for round in 0..4_u32 {
            for index in 0..6_u32 {
                let mut diagnostic = [0_u8; 256];
                let mut result = broker_call_result(&mut diagnostic);
                let body = [round as u8, index as u8];
                let status = unsafe {
                    broker_post(
                        channel,
                        generation,
                        index,
                        0,
                        body.as_ptr(),
                        body.len() as u32,
                        &mut result,
                    )
                };
                assert_eq!(status, Status::Success.value());
            }
            // Let this round expire, then drive a sweep.
            std::thread::sleep(Duration::from_millis(90));
            assert_eq!(broker_wait_for_frame(channel, 10), 0);
        }

        let retained = {
            let owner = broker_channel_for(channel).expect("channel");
            let inner = owner.lock();
            inner.frames.len()
        };
        assert!(
            retained <= options.max_inflight as usize,
            "expired one-way frames accumulated: {} retained",
            retained
        );

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_release_after_deadline_does_not_poison_the_pump_thread() {
        // Regression: the payload thread removes its frame when it wakes on a
        // terminal state. A pump still holding the delivery handle must still be
        // able to release it, or its thread-local owner token leaks and every
        // later non-broker call from that thread fails forever.
        let mut options = broker_default_options();
        options.default_deadline_ms = 150;
        let channel = open_broker_channel(&options);
        let attachment = broker_test_attachment(channel);
        let generation = attachment.generation;

        assert_eq!(broker_wait_for_frame(channel, 20), 0);
        let payload = std::thread::spawn(move || broker_enqueue(channel, generation, 42, b"late"));

        let frame = broker_wait_for_frame(channel, 5_000);
        assert_ne!(frame, 0);

        // Let the request pass its deadline while the pump still holds the frame.
        let (status, _, _) = payload.join().expect("payload thread");
        assert_eq!(status, Status::BrokerTimeout.value());

        let mut diagnostic = [0_u8; 256];
        let mut result = broker_call_result(&mut diagnostic);
        assert_eq!(
            unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
            Status::Success.value(),
            "releasing a delivery handle after its request terminated must succeed"
        );

        // The pump thread must not stay poisoned for ordinary FFI calls.
        let mut builder = 0_u64;
        let create_status = unsafe { multi_pwsh_create(&mut builder, &mut result) };
        assert_ne!(
            create_status,
            Status::BrokerDispatchViolation.value(),
            "the pump thread remained blocked from non-broker calls after release"
        );
        if builder != 0 {
            unsafe { multi_pwsh_release(builder, &mut result) };
        }

        broker_close_channel(channel);
        broker_release_test_attachment(&attachment);
    }

    #[test]
    fn broker_randomized_soak_is_bounded_and_leak_free() {
        // Deterministic LCG: reproducible in CI, no extra dependency.
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self) -> u64 {
                self.0 = self
                    .0
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                self.0 >> 33
            }
            fn below(&mut self, bound: u64) -> u64 {
                self.next() % bound
            }
        }

        let mut random = Lcg(0x5EED_1234_ABCD_0001);
        let mut seen_delivery_handles = HashSet::new();
        let mut seen_channel_handles = HashSet::new();
        let soak_started = Instant::now();

        for iteration in 0..24_u32 {
            let mut options = broker_default_options();
            options.max_inflight = 1 + random.below(4) as u32;
            options.max_body_bytes = 64 + random.below(256) as u32;
            // Short deadlines keep abandoned frames bounded without a long test.
            options.default_deadline_ms = 150 + random.below(150) as u32;

            let channel = open_broker_channel(&options);
            assert!(
                seen_channel_handles.insert(channel),
                "channel handle {} was reused",
                channel
            );
            let attachment = broker_test_attachment(channel);
            let generation = attachment.generation;

            // Attach a pump before producing so requests are queued, not refused.
            assert_eq!(broker_wait_for_frame(channel, 20), 0);

            let request_count = 1 + random.below(4) as u32;
            let mut payloads = Vec::new();
            for index in 0..request_count {
                let body = vec![(index % 251) as u8; 1 + random.below(16) as usize];
                payloads.push(std::thread::spawn(move || {
                    broker_enqueue(channel, generation, index, &body)
                }));
            }

            let event_count = random.below(6) as u32;
            for index in 0..event_count {
                let mut diagnostic = [0_u8; 256];
                let mut result = broker_call_result(&mut diagnostic);
                let body = [index as u8];
                let status =
                    unsafe { broker_post(channel, generation, 1, 0, body.as_ptr(), body.len() as u32, &mut result) };
                assert_eq!(status, Status::Success.value(), "one-way post applied backpressure");
            }

            // Service a random subset with a random terminal decision. Anything
            // left over must still terminate on its deadline or at close.
            let service_budget = request_count + event_count;
            for _ in 0..service_budget {
                let frame = broker_wait_for_frame(channel, 200);
                if frame == 0 {
                    continue;
                }
                assert!(
                    seen_delivery_handles.insert(frame),
                    "delivery handle {} was reused",
                    frame
                );

                let mut diagnostic = [0_u8; 256];
                let mut result = broker_call_result(&mut diagnostic);
                let mut info = BrokerFrameInfo {
                    size: mem::size_of::<BrokerFrameInfo>() as u32,
                    abi_version: BROKER_ABI_V1,
                    ..BrokerFrameInfo::default()
                };
                assert_eq!(
                    unsafe { multi_pwsh_broker_frame_get_info(frame, &mut info, &mut result) },
                    Status::Success.value()
                );
                let mut body = vec![0_u8; info.body_length as usize + 1];
                let mut required = 0_u32;
                assert_eq!(
                    unsafe {
                        multi_pwsh_broker_frame_read(
                            frame,
                            body.as_mut_ptr(),
                            body.len() as u32,
                            &mut required,
                            &mut result,
                        )
                    },
                    Status::Success.value()
                );
                assert_eq!(required, info.body_length);

                let correlation = info.correlation_id;
                let one_way = info.flags & BROKER_FRAME_FLAG_ONE_WAY != 0;
                assert_eq!(
                    unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
                    Status::Success.value()
                );
                // Releasing twice is always stale, never a second frame.
                assert_eq!(
                    unsafe { multi_pwsh_broker_frame_release(frame, &mut result) },
                    Status::InvalidHandle.value()
                );

                if one_way {
                    continue;
                }

                match random.below(4) {
                    0 => {
                        let reply = [7_u8, 8, 9];
                        unsafe {
                            multi_pwsh_broker_reply(channel, correlation, reply.as_ptr(), 3, &mut result);
                        }
                    }
                    1 => {
                        let message = b"soak";
                        unsafe {
                            multi_pwsh_broker_reply_error(
                                channel,
                                correlation,
                                1,
                                Utf8Span {
                                    data: message.as_ptr(),
                                    len: message.len(),
                                },
                                &mut result,
                            );
                        }
                    }
                    2 => unsafe {
                        multi_pwsh_broker_cancel(channel, correlation, &mut result);
                    },
                    // 3: deliberately abandon. Release is not abandonment, so
                    // this frame must still terminate on its deadline.
                    _ => {}
                }
            }

            broker_close_channel(channel);

            for payload in payloads {
                let (status, _, _) = payload.join().expect("payload thread");
                // Every path must reach a defined terminal status.
                assert!(
                    status == Status::Success.value()
                        || status == Status::ManagedFailure.value()
                        || status == Status::OperationCancelled.value()
                        || status == Status::BrokerTimeout.value()
                        || status == Status::BrokerClosed.value()
                        || status == Status::BrokerBusy.value(),
                    "iteration {} produced undefined status {}",
                    iteration,
                    status
                );
            }

            broker_release_test_attachment(&attachment);

            // Closing must leave no delivery handles or channel state behind.
            let registry = broker_frame_registry();
            let leaked = registry
                .as_ref()
                .map(|map| {
                    map.values()
                        .filter(|entry| Arc::strong_count(&entry.channel) > 0)
                        .count()
                })
                .unwrap_or(0);
            drop(registry);
            assert!(
                leaked <= seen_delivery_handles.len(),
                "delivery-handle registry grew without bound"
            );
            let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            assert!(
                !state.broker_channels.contains_key(&channel),
                "a closed channel remained registered"
            );
        }

        assert!(
            soak_started.elapsed() < Duration::from_secs(60),
            "the broker soak exceeded its bounded runtime"
        );

        // No channel state and no delivery handles survive the soak.
        let state = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for channel in &seen_channel_handles {
            assert!(!state.broker_channels.contains_key(channel));
        }
        drop(state);
        let registry = broker_frame_registry();
        if let Some(map) = registry.as_ref() {
            for handle in &seen_delivery_handles {
                assert!(!map.contains_key(handle), "delivery handle {} leaked", handle);
            }
        }
    }

    #[test]
    fn invocation_execution_scope_tracks_nesting_and_unwind_cleanup() {
        assert_eq!(pipeline_execution_depth(), 0);

        {
            let _outer = InvocationExecutionScope::enter();
            assert_eq!(pipeline_execution_depth(), 1);
            assert_eq!(active_pipeline_count(), 1);

            {
                let _inner = InvocationExecutionScope::enter();
                assert_eq!(pipeline_execution_depth(), 2);
                assert_eq!(active_pipeline_count(), 2);
            }

            assert_eq!(pipeline_execution_depth(), 1);
            assert_eq!(active_pipeline_count(), 1);
        }

        assert_eq!(pipeline_execution_depth(), 0);
        assert_eq!(active_pipeline_count(), 0);

        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let _scope = InvocationExecutionScope::enter();
            assert_eq!(pipeline_execution_depth(), 1);
            panic!("test pipeline unwind");
        }));
        assert!(unwind.is_err());
        assert_eq!(pipeline_execution_depth(), 0);
        assert_eq!(active_pipeline_count(), 0);
    }

    #[test]
    fn execution_scope_rejects_nested_ffi_calls_before_runtime_access() {
        let _scope = InvocationExecutionScope::enter();
        let mut diagnostic = [0_u8; 128];
        let mut result = CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: diagnostic.as_mut_ptr(),
            diagnostic_capacity: diagnostic.len(),
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        let mut handle = 0;

        assert_eq!(
            unsafe { multi_pwsh_create(&mut handle, &mut result) },
            Status::Backpressure.value()
        );
        assert_eq!(result.status, Status::Backpressure.value());
        assert_eq!(handle, 0);
        assert_eq!(
            std::str::from_utf8(&diagnostic[..result.diagnostic_written]).unwrap(),
            "PowerShell FFI calls are not permitted from code invoked by an active PowerShell pipeline."
        );
    }

    #[test]
    fn execution_scope_rejects_cross_thread_ffi_calls_before_runtime_access() {
        let _scope = InvocationExecutionScope::enter();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let error = reject_active_pipeline_ffi_call().unwrap_err();
                assert_eq!(error.0, Status::Backpressure);
                assert_eq!(
                    error.1,
                    "PowerShell FFI calls are not permitted while any PowerShell pipeline is running."
                );
            });
        });
    }

    #[test]
    fn explicitly_permitted_v2_call_remains_allowed_without_execution_scope() {
        let mut result = CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: std::ptr::null_mut(),
            diagnostic_capacity: 0,
            diagnostic_required: 0,
            diagnostic_written: 0,
        };

        assert_eq!(
            unsafe { v2_call_allow_active_pipeline(&mut result, || Ok(Status::Success)) },
            Status::Success.value()
        );
    }

    #[test]
    fn active_pipeline_rejects_session_mutation_before_operation_locking() {
        let _scope = InvocationExecutionScope::enter();
        let error = reject_active_pipeline_session_mutation().unwrap_err();

        assert_eq!(error.0, Status::Backpressure);
        assert_eq!(
            error.1,
            "PowerShell session variables cannot be read or changed while any PowerShell pipeline is running."
        );
    }

    #[test]
    fn live_object_contract_directions_accept_a_paired_bridge_marker_and_reject_a_lone_one() {
        fn descriptor(directions: u32) -> FfiLiveObjectContractDescriptor {
            FfiLiveObjectContractDescriptor {
                size: mem::size_of::<FfiLiveObjectContractDescriptor>() as u32,
                directions,
                interface_id_low: 1,
                interface_id_high: 2,
                major_version: 1,
                minor_version: 0,
                reserved: 0,
            }
        }

        // Accepted: the existing directions, and the bridge marker paired with
        // the consumer-to-session surface it augments.
        for directions in [0x01u32, 0x02, 0x03, 0x06, 0x07] {
            let value = descriptor(directions);
            assert!(
                unsafe { live_object_contract_input(&value) }.is_ok(),
                "directions {:#x} should be accepted",
                directions
            );
        }

        // Rejected: a bridge marker on its own, which would leave every existing
        // consumer-to-session check reading false for a contract that is one.
        // Rejected: anything outside the defined set.
        for directions in [0x00u32, 0x04, 0x05, 0x08, 0x10, 0xFFFF_FFFF] {
            let value = descriptor(directions);
            assert!(
                unsafe { live_object_contract_input(&value) }.is_err(),
                "directions {:#x} should be rejected",
                directions
            );
        }
    }

    #[test]
    fn abi_helpers_and_sized_calls_enforce_the_contract_without_a_payload() {
        const REQUIRED_FEATURES: u64 = FEATURE_STRUCTURED_INVOCATION_ERRORS
            | FEATURE_PER_CALL_DIAGNOSTICS
            | FEATURE_UTF8_SPANS
            | FEATURE_IMMUTABLE_RESULTS
            | FEATURE_TAGGED_VALUES
            | FEATURE_COMMAND_OPTIONS
            | FEATURE_BOUNDED_INPUT
            | FEATURE_INVOCATION_METADATA
            | FEATURE_ASYNC_OPERATIONS
            | FEATURE_SESSIONS
            | FEATURE_SESSION_POLLING
            | FEATURE_SESSION_POOL_REJECTION
            | FEATURE_SNAPSHOT_PROJECTIONS
            | FEATURE_SESSION_CONFIGURATION
            | FEATURE_SESSION_VARIABLES
            | FEATURE_CAPABILITY_RPC
            | FEATURE_LIVE_OBJECT_PROBE
            | FEATURE_LIVE_SESSION_OBJECT_PROBE
            | FEATURE_LIVE_OBJECT_CONTRACTS
            | FEATURE_LIVE_STREAM_POLLING
            | FEATURE_TYPED_RESULT_PAGING
            | FEATURE_OBSERVED_INVOCATION
            | FEATURE_SESSION_PREFLIGHT
            | FEATURE_RUNTIME_DIAGNOSTICS
            | FEATURE_DUPLEX_BROKER_CHANNEL
            | FEATURE_GENERATED_BRIDGE_ATTACHMENT
            | FEATURE_BROKER_TERMINAL_OBSERVATION
            | FEATURE_RELIABLE_BRIDGE_EVENTS
            | FEATURE_OBSERVED_PRESENTATION
            | FEATURE_SECRET_ADAPTERS
            | FEATURE_CREDENTIAL_RESULT
            | FEATURE_REMOTE_PROVIDER_DIAGNOSTICS;

        assert_eq!(ABI_VERSION, 2);
        assert_eq!(MINIMUM_COMPATIBLE_ABI_VERSION, 2);
        assert_eq!(feature_flags(), REQUIRED_FEATURES);

        let mut abi_info = AbiInfo {
            size: mem::size_of::<AbiInfo>() as u32,
            abi_version: 0,
            feature_flags: 0,
            minimum_compatible_abi_version: 0,
            _reserved: u32::MAX,
        };
        assert_eq!(
            unsafe { multi_pwsh_get_abi_info(&mut abi_info) },
            Status::Success.value()
        );
        assert_eq!(abi_info.abi_version, ABI_VERSION);
        assert_eq!(abi_info.minimum_compatible_abi_version, MINIMUM_COMPATIBLE_ABI_VERSION);
        assert_eq!(abi_info.feature_flags, REQUIRED_FEATURES);
        assert_eq!(abi_info._reserved, 0);

        let mut undersized_abi_info = AbiInfo {
            size: (mem::size_of::<AbiInfo>() - 1) as u32,
            abi_version: 0,
            feature_flags: 0,
            minimum_compatible_abi_version: 0,
            _reserved: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_get_abi_info(&mut undersized_abi_info) },
            Status::InvalidArgument.value()
        );

        let mut undersized_call_result = CallResult {
            size: (mem::size_of::<CallResult>() - 1) as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: std::ptr::null_mut(),
            diagnostic_capacity: 0,
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_create(std::ptr::null_mut(), &mut undersized_call_result) },
            Status::InvalidArgument.value()
        );
    }

    #[test]
    fn v2_negative_matrix_rejects_defined_malformed_inputs_without_a_payload() {
        let mut diagnostic = [0_u8; 8];
        let mut call_result = v2_call_result(&mut diagnostic);
        let invalid_utf8 = [0xff];
        let nul_utf8 = [b'a', 0];

        assert_eq!(
            unsafe { multi_pwsh_release(u64::MAX, &mut call_result) },
            Status::InvalidHandle.value()
        );
        assert_eq!(call_result.status, Status::InvalidHandle.value());
        assert_eq!(call_result.diagnostic_written, diagnostic.len());
        assert!(call_result.diagnostic_required > call_result.diagnostic_written);
        assert_ne!(call_result.flags & CALL_RESULT_DIAGNOSTIC_TRUNCATED, 0);

        let mut no_diagnostic_storage = CallResult {
            size: mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: std::ptr::null_mut(),
            diagnostic_capacity: 1,
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_release(u64::MAX, &mut no_diagnostic_storage) },
            Status::InvalidArgument.value()
        );

        let mut undersized_call_result = v2_call_result(&mut diagnostic);
        undersized_call_result.size = (mem::size_of::<CallResult>() - 1) as u32;
        assert_eq!(
            unsafe { multi_pwsh_release(u64::MAX, &mut undersized_call_result) },
            Status::InvalidArgument.value()
        );

        let empty = Utf8Span {
            data: std::ptr::null(),
            len: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_add_script_utf8(u64::MAX, empty, &mut call_result) },
            Status::InvalidHandle.value()
        );
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    u64::MAX,
                    Utf8Span {
                        data: std::ptr::null(),
                        len: 1,
                    },
                    &mut call_result,
                )
            },
            Status::InvalidArgument.value()
        );
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    u64::MAX,
                    Utf8Span {
                        data: invalid_utf8.as_ptr(),
                        len: invalid_utf8.len(),
                    },
                    &mut call_result,
                )
            },
            Status::InvalidArgument.value()
        );
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    u64::MAX,
                    Utf8Span {
                        data: nul_utf8.as_ptr(),
                        len: nul_utf8.len(),
                    },
                    &mut call_result,
                )
            },
            Status::InvalidArgument.value()
        );

        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, std::ptr::null(), &mut call_result) },
            Status::InvalidArgument.value()
        );
        let mut data_value = DataValue {
            size: (mem::size_of::<DataValue>() - 1) as u32,
            kind: 1,
            flags: 0,
            _reserved: 0,
            data: invalid_utf8.as_ptr(),
            data_len: invalid_utf8.len(),
        };
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, &data_value, &mut call_result) },
            Status::InvalidArgument.value()
        );
        data_value.size = mem::size_of::<DataValue>() as u32;
        data_value.flags = 1;
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, &data_value, &mut call_result) },
            Status::InvalidArgument.value()
        );
        data_value.flags = 0;
        data_value.data = std::ptr::null();
        data_value.data_len = 1;
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, &data_value, &mut call_result) },
            Status::InvalidArgument.value()
        );
        data_value.data = invalid_utf8.as_ptr();
        data_value.data_len = invalid_utf8.len();
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, &data_value, &mut call_result) },
            Status::InvalidArgument.value()
        );
        data_value.data = nul_utf8.as_ptr();
        data_value.data_len = nul_utf8.len();
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(u64::MAX, &data_value, &mut call_result) },
            Status::InvalidArgument.value()
        );

        let mut session_options = SessionOptions {
            size: (mem::size_of::<SessionOptions>() - 1) as u32,
            allowed_module_path: empty,
            ..empty_session_options()
        };
        let mut session_handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create(&session_options, &mut session_handle, &mut call_result) },
            Status::InvalidArgument.value()
        );
        session_options.size = mem::size_of::<SessionOptions>() as u32;
        session_options._reserved = 1;
        assert_eq!(
            unsafe { multi_pwsh_session_create(&session_options, &mut session_handle, &mut call_result) },
            Status::InvalidArgument.value()
        );
        session_options._reserved = 0;
        session_options.allowed_module_path = Utf8Span {
            data: std::ptr::null(),
            len: 1,
        };
        assert_eq!(
            unsafe { multi_pwsh_session_create(&session_options, &mut session_handle, &mut call_result) },
            Status::InvalidArgument.value()
        );

        let mut pool_options = SessionPoolOptions {
            size: (mem::size_of::<SessionPoolOptions>() - 1) as u32,
            minimum_sessions: 0,
            maximum_sessions: 1,
            flags: 0,
            _reserved: 0,
        };
        let mut pool_handle = u64::MAX;
        assert_eq!(
            unsafe { multi_pwsh_session_pool_create(&pool_options, &mut pool_handle, &mut call_result) },
            Status::InvalidArgument.value()
        );
        pool_options.size = mem::size_of::<SessionPoolOptions>() as u32;
        assert_eq!(
            unsafe { multi_pwsh_session_pool_create(&pool_options, &mut pool_handle, &mut call_result) },
            Status::UnsupportedCapability.value()
        );
        assert_eq!(pool_handle, 0);

        let mut required = 0;
        assert!(matches!(
            unsafe { write_utf8(std::ptr::null_mut(), 0, &mut required, "é") },
            Status::BufferTooSmall
        ));
        assert_eq!(required, "é".len());
        assert!(matches!(
            unsafe { write_utf8(std::ptr::null_mut(), required, &mut required, "é") },
            Status::InvalidArgument
        ));
    }

    #[test]
    #[ignore = "requires PWSH_FFI_PAYLOAD to be an explicit PowerShell payload directory"]
    fn explicit_payload_round_trip_uses_the_exported_abi() {
        let payload = std::env::var("PWSH_FFI_PAYLOAD")
            .expect("PWSH_FFI_PAYLOAD must name an explicit PowerShell payload directory");
        let mut abi_info = AbiInfo {
            size: std::mem::size_of::<AbiInfo>() as u32,
            abi_version: 0,
            feature_flags: 0,
            minimum_compatible_abi_version: 0,
            _reserved: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_get_abi_info(&mut abi_info) },
            Status::Success.value()
        );
        assert_eq!(abi_info.abi_version, ABI_VERSION);
        assert_eq!(abi_info.minimum_compatible_abi_version, MINIMUM_COMPATIBLE_ABI_VERSION);
        assert_ne!(abi_info.feature_flags & FEATURE_PER_CALL_DIAGNOSTICS, 0);
        assert_ne!(abi_info.feature_flags & FEATURE_UTF8_SPANS, 0);
        let payload_span = Utf8Span {
            data: payload.as_ptr(),
            len: payload.len(),
        };
        let mut diagnostic = [0_u8; 64];
        let mut call_result = CallResult {
            size: std::mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: diagnostic.as_mut_ptr(),
            diagnostic_capacity: diagnostic.len(),
            diagnostic_required: 0,
            diagnostic_written: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_initialize_utf8(payload_span, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(call_result.status, Status::Success.value());

        let mut v2_handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut v2_handle, &mut call_result) },
            Status::Success.value()
        );
        let empty = Utf8Span {
            data: std::ptr::null(),
            len: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_add_script_utf8(v2_handle, empty, &mut call_result) },
            Status::Success.value()
        );
        let mut required = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_utf8(v2_handle, std::ptr::null_mut(), 0, &mut required, &mut call_result,) },
            Status::Success.value()
        );
        assert_eq!(required, 0);
        assert_eq!(
            unsafe { multi_pwsh_release(v2_handle, &mut call_result) },
            Status::Success.value()
        );

        let mut immutable_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut immutable_builder, &mut call_result) },
            Status::Success.value()
        );
        let immutable_script = r#"
            Write-Output 'immutable-output'
            Write-Output ([pscustomobject]@{ Name = 'projection'; Count = 2; Nested = @{ Value = 1 }; Items = 1, 2 })
            Write-Error -Message 'immutable-error' -Category InvalidOperation -TargetObject 42
            Write-Warning 'immutable-warning'
            Write-Verbose 'immutable-verbose' -Verbose
            Write-Debug 'immutable-debug' -Debug
            Write-Information 'immutable-information' -InformationAction Continue
            Write-Progress -Activity 'immutable-progress' -Status 'running' -PercentComplete 50
        "#;
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    immutable_builder,
                    Utf8Span {
                        data: immutable_script.as_ptr(),
                        len: immutable_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut immutable_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(immutable_builder, &mut immutable_result, &mut call_result) },
            Status::Success.value()
        );
        assert_ne!(immutable_result, 0);
        let mut immutable_flags = 0;
        let mut immutable_sequence_count = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_info(
                    immutable_result,
                    &mut immutable_flags,
                    &mut immutable_sequence_count,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(immutable_flags, 0);
        assert!(
            immutable_sequence_count >= 7,
            "expected all stream records, got {}",
            immutable_sequence_count
        );
        for stream in 0..7 {
            let mut count = 0;
            let mut flags = 0;
            assert_eq!(
                unsafe {
                    multi_pwsh_result_get_stream_info(
                        immutable_result,
                        stream,
                        &mut count,
                        &mut flags,
                        &mut call_result,
                    )
                },
                Status::Success.value()
            );
            assert_eq!(
                count,
                if stream == 0 { 2 } else { 1 },
                "stream {} did not retain its record",
                stream
            );
            assert_eq!(flags, 0);
        }
        let mut required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_field_utf8(
                    immutable_result,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        let mut immutable_output = vec![0_u8; required];
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_field_utf8(
                    immutable_result,
                    0,
                    0,
                    0,
                    immutable_output.as_mut_ptr(),
                    immutable_output.len(),
                    &mut required,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(String::from_utf8(immutable_output).unwrap(), "immutable-output");
        let mut output_total = 0;
        let mut output_dropped = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_totals(
                    immutable_result,
                    0,
                    &mut output_total,
                    &mut output_dropped,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!((output_total, output_dropped), (2, 0));

        let mut scalar_property_entries = 0;
        let mut scalar_dropped_properties = 0;
        let mut scalar_type_names = 0;
        let mut scalar_dropped_type_names = 0;
        let mut scalar_projection_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_record_projection_info(
                    immutable_result,
                    0,
                    0,
                    &mut scalar_property_entries,
                    &mut scalar_dropped_properties,
                    &mut scalar_type_names,
                    &mut scalar_dropped_type_names,
                    &mut scalar_projection_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_ne!(scalar_projection_flags & RESULT_RECORD_SCALAR_VALUE_PRESENT, 0);
        let mut scalar_kind = 0;
        required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_value(
                    immutable_result,
                    0,
                    0,
                    0,
                    &mut scalar_kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!(scalar_kind, 1);
        let mut scalar_payload = vec![0_u8; required];
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_value(
                    immutable_result,
                    0,
                    0,
                    0,
                    &mut scalar_kind,
                    scalar_payload.as_mut_ptr(),
                    scalar_payload.len(),
                    &mut required,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(String::from_utf8(scalar_payload).unwrap(), "immutable-output");

        let mut property_entries = 0;
        let mut dropped_properties = 0;
        let mut type_names = 0;
        let mut dropped_type_names = 0;
        let mut property_projection_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_record_projection_info(
                    immutable_result,
                    0,
                    1,
                    &mut property_entries,
                    &mut dropped_properties,
                    &mut type_names,
                    &mut dropped_type_names,
                    &mut property_projection_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!((property_entries, dropped_properties), (2, 2));
        assert_ne!(property_projection_flags & RESULT_RECORD_PROPERTY_BAG_PRESENT, 0);
        assert_ne!(type_names, 0);
        let mut property_bag_kind = 0;
        required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_value(
                    immutable_result,
                    0,
                    1,
                    1,
                    &mut property_bag_kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!(property_bag_kind, 14);
        assert!(required > 4);

        let mut error_total = 0;
        let mut error_dropped = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_totals(
                    immutable_result,
                    1,
                    &mut error_total,
                    &mut error_dropped,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!((error_total, error_dropped), (1, 0));
        let mut error_projection_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_record_projection_info(
                    immutable_result,
                    1,
                    0,
                    &mut property_entries,
                    &mut dropped_properties,
                    &mut type_names,
                    &mut dropped_type_names,
                    &mut error_projection_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_ne!(error_projection_flags & RESULT_RECORD_ERROR_TARGET_VALUE_PRESENT, 0);
        let mut target_kind = 0;
        required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_value(
                    immutable_result,
                    1,
                    0,
                    2,
                    &mut target_kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!((target_kind, required), (4, 8));
        assert_eq!(
            unsafe { multi_pwsh_clear(immutable_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(immutable_builder, &mut call_result) },
            Status::Success.value()
        );
        let mut output_count_after_builder_release = 0;
        let mut output_flags_after_builder_release = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_info(
                    immutable_result,
                    0,
                    &mut output_count_after_builder_release,
                    &mut output_flags_after_builder_release,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(output_count_after_builder_release, 2);
        let mut retained_property_bag_kind = 0;
        required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_value(
                    immutable_result,
                    0,
                    1,
                    1,
                    &mut retained_property_bag_kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!(retained_property_bag_kind, 14);
        assert!(required > 4);
        required = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_field_utf8(
                    immutable_result,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        let mut immutable_output_after_builder_release = vec![0_u8; required];
        assert_eq!(
            unsafe {
                multi_pwsh_result_copy_stream_record_field_utf8(
                    immutable_result,
                    0,
                    0,
                    0,
                    immutable_output_after_builder_release.as_mut_ptr(),
                    immutable_output_after_builder_release.len(),
                    &mut required,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(
            String::from_utf8(immutable_output_after_builder_release).unwrap(),
            "immutable-output"
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(immutable_result, &mut call_result) },
            Status::Success.value()
        );

        let mut bounded_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut bounded_builder, &mut call_result) },
            Status::Success.value()
        );
        let bounded_script = "1..40 | ForEach-Object { Write-Output $_; Write-Warning $_ }";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    bounded_builder,
                    Utf8Span {
                        data: bounded_script.as_ptr(),
                        len: bounded_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut bounded_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(bounded_builder, &mut bounded_result, &mut call_result) },
            Status::Success.value()
        );
        let mut bounded_warning_count = 0;
        let mut bounded_warning_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_info(
                    bounded_result,
                    2,
                    &mut bounded_warning_count,
                    &mut bounded_warning_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(bounded_warning_count, 32);
        assert_ne!(bounded_warning_flags, 0);
        let mut bounded_output_count = 0;
        let mut bounded_output_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_info(
                    bounded_result,
                    0,
                    &mut bounded_output_count,
                    &mut bounded_output_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(bounded_output_count, 32);
        assert_ne!(bounded_output_flags, 0);
        assert_eq!(
            unsafe { multi_pwsh_result_release(bounded_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_clear(bounded_builder, &mut call_result) },
            Status::Success.value()
        );
        let replacement_script = "Write-Output 'stream-buffers-replaced'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    bounded_builder,
                    Utf8Span {
                        data: replacement_script.as_ptr(),
                        len: replacement_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut replacement_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(bounded_builder, &mut replacement_result, &mut call_result) },
            Status::Success.value()
        );
        let mut replacement_warning_count = 0;
        let mut replacement_warning_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_stream_info(
                    replacement_result,
                    2,
                    &mut replacement_warning_count,
                    &mut replacement_warning_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(replacement_warning_count, 0);
        assert_eq!(replacement_warning_flags, 0);
        assert_eq!(
            unsafe { multi_pwsh_result_release(replacement_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(bounded_builder, &mut call_result) },
            Status::Success.value()
        );

        let mut invalid_state_handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut invalid_state_handle, &mut call_result) },
            Status::Success.value()
        );
        let mut error_count = 0;
        assert_eq!(
            unsafe { multi_pwsh_get_invocation_error_count(invalid_state_handle, &mut error_count, &mut call_result) },
            Status::ManagedFailure.value()
        );
        assert_eq!(call_result.status, Status::ManagedFailure.value());
        let diagnostic = std::str::from_utf8(&diagnostic[..call_result.diagnostic_written]).unwrap();
        assert!(
            diagnostic.contains("No invocation"),
            "unexpected diagnostic: {}",
            diagnostic
        );
        assert_ne!(call_result.flags & CALL_RESULT_DIAGNOSTIC_TRUNCATED, 0);
        assert!(call_result.diagnostic_required > call_result.diagnostic_written);
        assert_eq!(
            unsafe { multi_pwsh_release(invalid_state_handle, &mut call_result) },
            Status::Success.value()
        );
    }

    #[test]
    #[ignore = "requires PWSH_FFI_PAYLOAD to be an explicit PowerShell payload directory"]
    fn explicit_payload_async_operations_are_terminal_and_lifetime_safe() {
        let payload = std::env::var("PWSH_FFI_PAYLOAD")
            .expect("PWSH_FFI_PAYLOAD must name an explicit PowerShell payload directory");
        let mut diagnostic = [0_u8; 512];
        let mut call_result = v2_call_result(&mut diagnostic);
        initialize_v2(&payload, &mut call_result);
        assert_ne!(feature_flags() & FEATURE_ASYNC_OPERATIONS, 0);

        let success_builder = v2_create_session(&mut call_result);
        let success_script = "$input | ForEach-Object { Start-Sleep -Milliseconds 250; $_ * 2 }";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    success_builder,
                    Utf8Span {
                        data: success_script.as_ptr(),
                        len: success_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        add_v2_input_value(success_builder, 4, &3_i64.to_le_bytes(), &mut call_result);
        assert_eq!(
            unsafe { multi_pwsh_complete_input(success_builder, &mut call_result) },
            Status::Success.value()
        );
        let mut success_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(success_builder, &mut success_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_complete_input(success_builder, &mut call_result) },
            Status::Backpressure.value()
        );
        let mut operation_state = 0;
        let mut terminal_status = 0;
        assert_eq!(
            unsafe { multi_pwsh_operation_get_result(success_operation, std::ptr::null_mut(), &mut call_result) },
            Status::InvalidArgument.value()
        );
        let mut result_before_terminal = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_operation_get_result(success_operation, &mut result_before_terminal, &mut call_result)
            },
            Status::OperationNotTerminal.value()
        );
        assert_eq!(call_result.status, Status::OperationNotTerminal.value());
        assert_eq!(
            unsafe {
                multi_pwsh_operation_wait(
                    success_operation,
                    5_000,
                    &mut operation_state,
                    &mut terminal_status,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(operation_state, OperationState::Completed as u32);
        assert_eq!(terminal_status, Status::Success.value());
        let mut success_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_operation_get_result(success_operation, &mut success_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(v2_result_output(success_result, &mut call_result), vec!["6".to_owned()]);
        assert_eq!(
            unsafe { multi_pwsh_result_release(success_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_release(success_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(success_builder, &mut call_result) },
            Status::Success.value()
        );

        let cancellation_builder = v2_create_session(&mut call_result);
        let cancellation_script = "Start-Sleep -Seconds 30; 'unexpected completion'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    cancellation_builder,
                    Utf8Span {
                        data: cancellation_script.as_ptr(),
                        len: cancellation_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut cancellation_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(cancellation_builder, &mut cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            unsafe { multi_pwsh_operation_stop(cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_stop(cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        operation_state = 0;
        terminal_status = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_operation_wait(
                    cancellation_operation,
                    5_000,
                    &mut operation_state,
                    &mut terminal_status,
                    &mut call_result,
                )
            },
            Status::OperationCancelled.value()
        );
        assert_eq!(operation_state, OperationState::Cancelled as u32);
        assert_eq!(terminal_status, Status::OperationCancelled.value());
        assert!(std::str::from_utf8(&diagnostic[..call_result.diagnostic_written])
            .unwrap()
            .contains("cancelled"));
        let mut cancelled_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_operation_get_result(cancellation_operation, &mut cancelled_result, &mut call_result) },
            Status::OperationCancelled.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_release(cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(cancellation_builder, &mut call_result) },
            Status::Success.value()
        );

        let race_builder = v2_create_session(&mut call_result);
        let race_script = "Start-Sleep -Seconds 30; 'release-race'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    race_builder,
                    Utf8Span {
                        data: race_script.as_ptr(),
                        len: race_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut race_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(race_builder, &mut race_operation, &mut call_result) },
            Status::Success.value()
        );
        let poller = std::thread::spawn(move || {
            for _ in 0..32 {
                let mut state = 0;
                let mut status = 0;
                let mut call_result = CallResult {
                    size: std::mem::size_of::<CallResult>() as u32,
                    status: 0,
                    flags: 0,
                    _reserved: 0,
                    diagnostic: std::ptr::null_mut(),
                    diagnostic_capacity: 0,
                    diagnostic_required: 0,
                    diagnostic_written: 0,
                };
                let poll_status =
                    unsafe { multi_pwsh_operation_poll(race_operation, &mut state, &mut status, &mut call_result) };
                assert!(
                    poll_status == Status::Success.value()
                        || poll_status == Status::OperationCancelled.value()
                        || poll_status == Status::InvalidHandle.value(),
                    "unexpected release-race poll status {}",
                    poll_status
                );
            }
        });
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            unsafe { multi_pwsh_operation_release(race_operation, &mut call_result) },
            Status::Success.value()
        );
        poller.join().unwrap();
        assert_eq!(
            unsafe { multi_pwsh_release(race_builder, &mut call_result) },
            Status::Success.value()
        );
    }

    #[test]
    #[ignore = "requires PWSH_FFI_PAYLOAD to be an explicit PowerShell payload directory"]
    fn explicit_payload_lifecycle_stress_enforces_serialization_and_lifetime_contracts() {
        let payload = std::env::var("PWSH_FFI_PAYLOAD")
            .expect("PWSH_FFI_PAYLOAD must name an explicit PowerShell payload directory");
        let mut diagnostic = [0_u8; 512];
        let mut call_result = v2_call_result(&mut diagnostic);
        initialize_v2(&payload, &mut call_result);

        let stale_builder = v2_create_session(&mut call_result);
        assert_eq!(
            unsafe { multi_pwsh_release(stale_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(stale_builder, &mut call_result) },
            Status::InvalidHandle.value()
        );
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    stale_builder,
                    Utf8Span {
                        data: b"'stale'".as_ptr(),
                        len: b"'stale'".len(),
                    },
                    &mut call_result,
                )
            },
            Status::InvalidHandle.value()
        );
        let fresh_builder = v2_create_session(&mut call_result);
        assert_ne!(
            fresh_builder, stale_builder,
            "released builder handles must not be reused"
        );
        assert_eq!(
            unsafe { multi_pwsh_release(fresh_builder, &mut call_result) },
            Status::Success.value()
        );

        let cancellation_builder = v2_create_session(&mut call_result);
        let cancellation_script = "1..50 | ForEach-Object { Start-Sleep -Milliseconds 100; Write-Output $_ }";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    cancellation_builder,
                    Utf8Span {
                        data: cancellation_script.as_ptr(),
                        len: cancellation_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut cancellation_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(cancellation_builder, &mut cancellation_operation, &mut call_result,) },
            Status::Success.value()
        );

        let barrier = Arc::new(std::sync::Barrier::new(4));
        let operation_stop = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut result = v2_call_result_without_diagnostic();
                barrier.wait();
                unsafe { multi_pwsh_operation_stop(cancellation_operation, &mut result) }
            })
        };
        let repeated_operation_stop = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut result = v2_call_result_without_diagnostic();
                barrier.wait();
                unsafe { multi_pwsh_operation_stop(cancellation_operation, &mut result) }
            })
        };
        let builder_stop = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut result = v2_call_result_without_diagnostic();
                barrier.wait();
                unsafe { multi_pwsh_stop(cancellation_builder, &mut result) }
            })
        };
        let builder_release = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut result = v2_call_result_without_diagnostic();
                barrier.wait();
                unsafe { multi_pwsh_release(cancellation_builder, &mut result) }
            })
        };
        assert_eq!(operation_stop.join().unwrap(), Status::Success.value());
        assert_eq!(repeated_operation_stop.join().unwrap(), Status::Success.value());
        assert!(
            matches!(
                builder_stop.join().unwrap(),
                value if value == Status::Success.value() || value == Status::InvalidHandle.value()
            ),
            "builder stop may race the releasing owner, but must remain defined"
        );
        assert_eq!(builder_release.join().unwrap(), Status::Success.value());

        let mut operation_state = 0;
        let mut terminal_status = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_operation_wait(
                    cancellation_operation,
                    5_000,
                    &mut operation_state,
                    &mut terminal_status,
                    &mut call_result,
                )
            },
            Status::OperationCancelled.value()
        );
        assert_eq!(operation_state, OperationState::Cancelled as u32);
        assert_eq!(terminal_status, Status::OperationCancelled.value());
        assert_eq!(
            unsafe { multi_pwsh_operation_stop(cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        let mut cancelled_result = u64::MAX;
        assert_eq!(
            unsafe { multi_pwsh_operation_get_result(cancellation_operation, &mut cancelled_result, &mut call_result) },
            Status::OperationCancelled.value()
        );
        assert_eq!(
            cancelled_result,
            u64::MAX,
            "cancelled operations must not return successful partial output"
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_release(cancellation_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_release(cancellation_operation, &mut call_result) },
            Status::InvalidHandle.value()
        );
        assert_eq!(
            unsafe {
                multi_pwsh_operation_poll(
                    cancellation_operation,
                    &mut operation_state,
                    &mut terminal_status,
                    &mut call_result,
                )
            },
            Status::InvalidHandle.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(cancellation_builder, &mut call_result) },
            Status::InvalidHandle.value()
        );

        let result_builder = v2_create_session(&mut call_result);
        let result_script = "'result-outlives-builder'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    result_builder,
                    Utf8Span {
                        data: result_script.as_ptr(),
                        len: result_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let result_handle = v2_invoke(result_builder, &mut call_result);
        assert_eq!(
            unsafe { multi_pwsh_release(result_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            v2_result_output(result_handle, &mut call_result),
            vec!["result-outlives-builder".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(result_handle, &mut call_result) },
            Status::Success.value()
        );
        let mut result_flags = 0;
        let mut sequence_count = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_info(result_handle, &mut result_flags, &mut sequence_count, &mut call_result)
            },
            Status::InvalidHandle.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(result_handle, &mut call_result) },
            Status::InvalidHandle.value()
        );

        let session_options = empty_session_options();
        let mut session_handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create(&session_options, &mut session_handle, &mut call_result) },
            Status::Success.value()
        );
        let mut session_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create_builder(session_handle, &mut session_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_session_release(session_handle, &mut call_result) },
            Status::Success.value()
        );
        let mut rejected_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create_builder(session_handle, &mut rejected_builder, &mut call_result) },
            Status::InvalidHandle.value()
        );
        let mut stale_snapshot = SessionSnapshot {
            size: mem::size_of::<SessionSnapshot>() as u32,
            state: 0,
            runspace_state: 0,
            flags: 0,
            active_pipeline_count: 0,
            event_count: 0,
            invocation_count: 0,
            history_count: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_session_get_snapshot(session_handle, &mut stale_snapshot, &mut call_result) },
            Status::InvalidHandle.value()
        );
        let session_script = "'builder-outlives-session'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    session_builder,
                    Utf8Span {
                        data: session_script.as_ptr(),
                        len: session_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let session_result = v2_invoke(session_builder, &mut call_result);
        assert_eq!(
            v2_result_output(session_result, &mut call_result),
            vec!["builder-outlives-session".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(session_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(session_builder, &mut call_result) },
            Status::Success.value()
        );

        let first_builder = v2_create_session(&mut call_result);
        let second_builder = v2_create_session(&mut call_result);
        let serialized_script = "Start-Sleep -Milliseconds 300; 'serialized'";
        for builder in [first_builder, second_builder] {
            assert_eq!(
                unsafe {
                    multi_pwsh_add_script_utf8(
                        builder,
                        Utf8Span {
                            data: serialized_script.as_ptr(),
                            len: serialized_script.len(),
                        },
                        &mut call_result,
                    )
                },
                Status::Success.value()
            );
        }
        let started = Instant::now();
        let mut first_operation = 0;
        let mut second_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(first_builder, &mut first_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(second_builder, &mut second_operation, &mut call_result) },
            Status::Success.value()
        );
        for operation in [first_operation, second_operation] {
            operation_state = 0;
            terminal_status = 0;
            assert_eq!(
                unsafe {
                    multi_pwsh_operation_wait(
                        operation,
                        5_000,
                        &mut operation_state,
                        &mut terminal_status,
                        &mut call_result,
                    )
                },
                Status::Success.value()
            );
            assert_eq!(operation_state, OperationState::Completed as u32);
            assert_eq!(terminal_status, Status::Success.value());
            let mut result = 0;
            assert_eq!(
                unsafe { multi_pwsh_operation_get_result(operation, &mut result, &mut call_result) },
                Status::Success.value()
            );
            assert_eq!(
                unsafe { multi_pwsh_result_release(result, &mut call_result) },
                Status::Success.value()
            );
            assert_eq!(
                unsafe { multi_pwsh_operation_release(operation, &mut call_result) },
                Status::Success.value()
            );
        }
        assert!(
            started.elapsed() >= Duration::from_millis(500),
            "process-global normal-operation serialization must make two 300 ms invocations sequential"
        );
        for builder in [first_builder, second_builder] {
            assert_eq!(
                unsafe { multi_pwsh_release(builder, &mut call_result) },
                Status::Success.value()
            );
        }
    }

    #[test]
    #[ignore = "requires PWSH_FFI_PAYLOAD to be an explicit PowerShell payload directory"]
    fn explicit_payload_increment_6_sessions_are_bounded_and_lifetime_safe() {
        let payload = std::env::var("PWSH_FFI_PAYLOAD")
            .expect("PWSH_FFI_PAYLOAD must name an explicit PowerShell payload directory");
        let mut diagnostic = [0_u8; 512];
        let mut call_result = v2_call_result(&mut diagnostic);
        initialize_v2(&payload, &mut call_result);
        assert_ne!(feature_flags() & FEATURE_SESSIONS, 0);
        assert_ne!(feature_flags() & FEATURE_SESSION_POLLING, 0);
        assert_ne!(feature_flags() & FEATURE_SESSION_POOL_REJECTION, 0);
        assert_ne!(feature_flags() & FEATURE_SESSION_VARIABLES, 0);

        let configured_options = SessionOptions {
            history_mode: 1,
            error_preference: 3,
            warning_preference: 1,
            verbose_preference: 1,
            information_preference: 1,
            ..empty_session_options()
        };
        let mut configured_session = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create(&configured_options, &mut configured_session, &mut call_result) },
            Status::Success.value()
        );
        assert_ne!(configured_session, 0);
        let mut snapshot = SessionSnapshot {
            size: mem::size_of::<SessionSnapshot>() as u32,
            state: 0,
            runspace_state: 0,
            flags: 0,
            active_pipeline_count: 0,
            event_count: 0,
            invocation_count: 0,
            history_count: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_session_get_snapshot(configured_session, &mut snapshot, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(snapshot.state, 1);
        assert_eq!(snapshot.active_pipeline_count, 0);
        assert!(snapshot.event_count >= 1);

        let variable_name = "FfiCopied";
        let variable_payload = b"copied-session-variable";
        let variable_value = DataValue {
            size: mem::size_of::<DataValue>() as u32,
            kind: VALUE_KIND_STRING,
            flags: 0,
            _reserved: 0,
            data: variable_payload.as_ptr(),
            data_len: variable_payload.len(),
        };
        assert_eq!(
            unsafe {
                multi_pwsh_session_set_variable(
                    configured_session,
                    Utf8Span {
                        data: variable_name.as_ptr(),
                        len: variable_name.len(),
                    },
                    &variable_value,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let mut found = 0;
        let mut variable_kind = 0;
        let mut required_length = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_session_get_variable_snapshot(
                    configured_session,
                    Utf8Span {
                        data: variable_name.as_ptr(),
                        len: variable_name.len(),
                    },
                    &mut found,
                    &mut variable_kind,
                    std::ptr::null_mut(),
                    0,
                    &mut required_length,
                    &mut call_result,
                )
            },
            Status::BufferTooSmall.value()
        );
        assert_eq!(found, 1);
        assert_eq!(variable_kind, VALUE_KIND_STRING);
        assert_eq!(required_length, variable_payload.len());
        let mut variable_snapshot = vec![0_u8; required_length];
        assert_eq!(
            unsafe {
                multi_pwsh_session_get_variable_snapshot(
                    configured_session,
                    Utf8Span {
                        data: variable_name.as_ptr(),
                        len: variable_name.len(),
                    },
                    &mut found,
                    &mut variable_kind,
                    variable_snapshot.as_mut_ptr(),
                    variable_snapshot.len(),
                    &mut required_length,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(found, 1);
        assert_eq!(variable_kind, VALUE_KIND_STRING);
        assert_eq!(variable_snapshot, variable_payload);
        let mut removed = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_session_remove_variable(
                    configured_session,
                    Utf8Span {
                        data: variable_name.as_ptr(),
                        len: variable_name.len(),
                    },
                    &mut removed,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(removed, 1);
        assert_eq!(
            unsafe {
                multi_pwsh_session_remove_variable(
                    configured_session,
                    Utf8Span {
                        data: variable_name.as_ptr(),
                        len: variable_name.len(),
                    },
                    &mut removed,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(removed, 0);

        let mut configured_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create_builder(configured_session, &mut configured_builder, &mut call_result) },
            Status::Success.value()
        );
        let configured_script = "Write-Output \"$ErrorActionPreference|$WarningPreference|$VerbosePreference\"";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    configured_builder,
                    Utf8Span {
                        data: configured_script.as_ptr(),
                        len: configured_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let configured_result = v2_invoke(configured_builder, &mut call_result);
        assert_eq!(
            v2_result_output(configured_result, &mut call_result),
            vec!["Stop|Continue|Continue".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(configured_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_session_get_snapshot(configured_session, &mut snapshot, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(snapshot.active_pipeline_count, 0);
        assert_eq!(snapshot.invocation_count, 1);
        assert_eq!(snapshot.history_count, 1);
        let mut event_sequence = 0;
        let mut event_state = 0;
        let mut event_flags = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_session_get_event_info(
                    configured_session,
                    0,
                    &mut event_sequence,
                    &mut event_state,
                    &mut event_flags,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_ne!(event_sequence, 0);
        assert_eq!(event_state, 1);

        let first_script = "Start-Sleep -Milliseconds 100; 'session-first'";
        let second_script = "Start-Sleep -Milliseconds 100; 'session-second'";
        let mut first_builder = 0;
        let mut second_builder = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create_builder(configured_session, &mut first_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_session_create_builder(configured_session, &mut second_builder, &mut call_result) },
            Status::Success.value()
        );
        for (builder, script) in [(first_builder, first_script), (second_builder, second_script)] {
            assert_eq!(
                unsafe {
                    multi_pwsh_add_script_utf8(
                        builder,
                        Utf8Span {
                            data: script.as_ptr(),
                            len: script.len(),
                        },
                        &mut call_result,
                    )
                },
                Status::Success.value()
            );
        }
        let mut first_operation = 0;
        let mut second_operation = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(first_builder, &mut first_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(second_builder, &mut second_operation, &mut call_result) },
            Status::Backpressure.value()
        );
        assert_eq!(second_operation, 0);
        for operation in [first_operation] {
            let mut operation_state = 0;
            let mut terminal_status = 0;
            assert_eq!(
                unsafe {
                    multi_pwsh_operation_wait(
                        operation,
                        5_000,
                        &mut operation_state,
                        &mut terminal_status,
                        &mut call_result,
                    )
                },
                Status::Success.value()
            );
            assert_eq!(operation_state, OperationState::Completed as u32);
            assert_eq!(terminal_status, Status::Success.value());
            let mut result = 0;
            assert_eq!(
                unsafe { multi_pwsh_operation_get_result(operation, &mut result, &mut call_result) },
                Status::Success.value()
            );
            assert_eq!(
                unsafe { multi_pwsh_result_release(result, &mut call_result) },
                Status::Success.value()
            );
            assert_eq!(
                unsafe { multi_pwsh_operation_release(operation, &mut call_result) },
                Status::Success.value()
            );
        }
        assert_eq!(
            unsafe { multi_pwsh_invoke_async(second_builder, &mut second_operation, &mut call_result) },
            Status::Success.value()
        );
        let mut operation_state = 0;
        let mut terminal_status = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_operation_wait(
                    second_operation,
                    5_000,
                    &mut operation_state,
                    &mut terminal_status,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(operation_state, OperationState::Completed as u32);
        assert_eq!(terminal_status, Status::Success.value());
        let mut second_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_operation_get_result(second_operation, &mut second_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(second_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_operation_release(second_operation, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(first_builder, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(second_builder, &mut call_result) },
            Status::Success.value()
        );
        for _ in 0..13 {
            let event_result = v2_invoke(configured_builder, &mut call_result);
            assert_eq!(
                unsafe { multi_pwsh_result_release(event_result, &mut call_result) },
                Status::Success.value()
            );
        }
        assert_eq!(
            unsafe { multi_pwsh_session_get_snapshot(configured_session, &mut snapshot, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(snapshot.event_count, 32);
        assert_ne!(snapshot.flags & 1, 0);

        assert_eq!(
            unsafe { multi_pwsh_session_release(configured_session, &mut call_result) },
            Status::Success.value()
        );
        let mut lifetime_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(configured_builder, &mut lifetime_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            v2_result_output(lifetime_result, &mut call_result),
            vec!["Stop|Continue|Continue".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(lifetime_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(configured_builder, &mut call_result) },
            Status::Success.value()
        );

        let rejected_current_options = SessionOptions {
            runspace_mode: 0,
            history_mode: 1,
            ..empty_session_options()
        };
        let mut rejected_session = 0;
        assert_eq!(
            unsafe { multi_pwsh_session_create(&rejected_current_options, &mut rejected_session, &mut call_result) },
            Status::UnsupportedCapability.value()
        );
        assert_eq!(rejected_session, 0);
        assert!(std::str::from_utf8(&diagnostic[..call_result.diagnostic_written])
            .unwrap()
            .contains("current-runspace"));

        let pool_options = SessionPoolOptions {
            size: mem::size_of::<SessionPoolOptions>() as u32,
            minimum_sessions: 0,
            maximum_sessions: 2,
            flags: 0,
            _reserved: 0,
        };
        let mut pool_handle = u64::MAX;
        assert_eq!(
            unsafe { multi_pwsh_session_pool_create(&pool_options, &mut pool_handle, &mut call_result) },
            Status::UnsupportedCapability.value()
        );
        assert_eq!(pool_handle, 0);
    }

    #[test]
    fn tagged_value_validation_rejects_unknown_and_malformed_payloads() {
        let unknown = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind: u32::MAX,
            flags: 0,
            _reserved: 0,
            data: std::ptr::null(),
            data_len: 0,
        };
        assert!(matches!(
            unsafe { data_value_input(&unknown) },
            Err((Status::UnsupportedValue, _))
        ));

        let malformed = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind: 3,
            flags: 0,
            _reserved: 0,
            data: [2_u8].as_ptr(),
            data_len: 1,
        };
        assert!(matches!(
            unsafe { data_value_input(&malformed) },
            Err((Status::InvalidArgument, _))
        ));
    }

    #[test]
    fn bridge_variable_name_is_validated_before_attachment_copy() {
        let oversized = [b'R'; MAX_BRIDGE_VARIABLE_NAME_BYTES + 1];
        assert!(matches!(
            unsafe {
                bridge_variable_name_input(Utf8Span {
                    data: oversized.as_ptr(),
                    len: oversized.len(),
                })
            },
            Err((Status::InvalidArgument, _))
        ));

        let maximum = [b'R'; MAX_BRIDGE_VARIABLE_NAME_BYTES];
        assert!(unsafe {
            bridge_variable_name_input(Utf8Span {
                data: maximum.as_ptr(),
                len: maximum.len(),
            })
        }
        .is_ok());

        let malformed = b"R-DM";
        assert!(matches!(
            unsafe {
                bridge_variable_name_input(Utf8Span {
                    data: malformed.as_ptr(),
                    len: malformed.len(),
                })
            },
            Err((Status::InvalidArgument, _))
        ));

        let valid = b"RDM";
        assert_eq!(
            unsafe {
                bridge_variable_name_input(Utf8Span {
                    data: valid.as_ptr(),
                    len: valid.len(),
                })
            }
            .unwrap(),
            "RDM"
        );
    }

    #[test]
    fn session_options_and_pool_bounds_reject_unsupported_configurations() {
        let legacy_prefix = SessionOptionsPrefix {
            size: SESSION_OPTIONS_PREFIX_SIZE,
            runspace_mode: 1,
            initial_configuration: 0,
            history_mode: 0,
            error_preference: 0,
            warning_preference: 0,
            verbose_preference: 0,
            debug_preference: 0,
            information_preference: 0,
            flags: 0,
            _reserved: 0,
            allowed_module_path: Utf8Span {
                data: std::ptr::null(),
                len: 0,
            },
        };
        assert!(unsafe {
            session_options_input(&legacy_prefix as *const SessionOptionsPrefix as *const SessionOptions)
        }
        .is_ok());

        let current_without_configuration = SessionOptions {
            runspace_mode: 0,
            ..empty_session_options()
        };
        assert!(unsafe { session_options_input(&current_without_configuration) }.is_ok());

        let current_with_history = SessionOptions {
            runspace_mode: 0,
            history_mode: 1,
            ..empty_session_options()
        };
        assert!(matches!(
            unsafe { session_options_input(&current_with_history) },
            Err((Status::UnsupportedCapability, _))
        ));

        let unsupported_preference = SessionOptions {
            error_preference: 4,
            ..current_with_history
        };
        assert!(matches!(
            unsafe { session_options_input(&unsupported_preference) },
            Err((Status::InvalidArgument, _))
        ));

        let invalid_pool = SessionPoolOptions {
            size: mem::size_of::<SessionPoolOptions>() as u32,
            minimum_sessions: 0,
            maximum_sessions: 0,
            flags: 0,
            _reserved: 0,
        };
        assert!(matches!(
            validate_pool_options(&invalid_pool),
            Err((Status::InvalidArgument, _))
        ));

        let bounded_pool = SessionPoolOptions {
            maximum_sessions: 64,
            ..invalid_pool
        };
        assert!(validate_pool_options(&bounded_pool).is_ok());
    }

    unsafe extern "C" fn test_capability_dispatch(
        _handle: u64,
        _invocation_id: u64,
        _name: Utf8Span,
        _arguments: *const DataValue,
        _argument_count: u32,
        _deadline_milliseconds: u32,
        _response_kind: *mut u32,
        _response: *mut u8,
        _response_capacity: usize,
        _response_required: *mut usize,
        _result: *mut CallResult,
    ) -> i32 {
        Status::Success.value()
    }

    static TEST_CAPABILITY_CANCEL_COUNT: AtomicU64 = AtomicU64::new(0);

    unsafe extern "C" fn test_capability_cancel(_handle: u64, _invocation_id: u64) {
        TEST_CAPABILITY_CANCEL_COUNT.fetch_add(1, Ordering::AcqRel);
    }

    #[test]
    fn capability_cancellation_releases_pending_invocation() {
        TEST_CAPABILITY_CANCEL_COUNT.store(0, Ordering::Release);
        let registration = Arc::new(CapabilityRegistrationState {
            handle: 1,
            definitions: HashMap::new(),
            dispatch: test_capability_dispatch,
            cancel: test_capability_cancel,
            active: AtomicBool::new(true),
            invocations: Mutex::new(HashMap::new()),
        });
        let capability = registration.begin_invocation(42).unwrap();

        cancel_and_finish_capability(Some(&capability));

        assert_eq!(TEST_CAPABILITY_CANCEL_COUNT.load(Ordering::Acquire), 1);
        assert!(!registration
            .invocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(&42));
    }

    #[test]
    fn calls_contain_native_panics() {
        let mut call_result = v2_call_result_without_diagnostic();
        assert_eq!(
            unsafe { v2_call_allow_active_pipeline(&mut call_result, || panic!("test panic containment")) },
            Status::Panic.value()
        );
        assert_eq!(call_result.status, Status::Panic.value());
    }

    #[test]
    #[ignore = "requires PWSH_FFI_PAYLOAD to be an explicit PowerShell payload directory"]
    fn explicit_payload_increment_3_tagged_values_commands_and_input_are_bounded() {
        let payload = std::env::var("PWSH_FFI_PAYLOAD")
            .expect("PWSH_FFI_PAYLOAD must name an explicit PowerShell payload directory");
        let mut diagnostic = [0_u8; 512];
        let mut call_result = v2_call_result(&mut diagnostic);
        initialize_v2(&payload, &mut call_result);
        assert_ne!(feature_flags() & FEATURE_TAGGED_VALUES, 0);
        assert_ne!(feature_flags() & FEATURE_COMMAND_OPTIONS, 0);
        assert_ne!(feature_flags() & FEATURE_BOUNDED_INPUT, 0);

        let value_handle = v2_create_session(&mut call_result);
        let value_script = r#"
            param($Nothing, $String, $Boolean, $Signed, $Unsigned, $Double, $Decimal, $Bytes, $When, $Offset, $Guid, $Uri, $Array, $Bag, $Switch)
            @(
                ($null -eq $Nothing),
                ($String -eq 'tagged'),
                ($Boolean -eq $true),
                ($Signed -eq -7),
                ($Unsigned -eq 9),
                ($Double -eq 2.5),
                ($Decimal -eq [decimal]42.5),
                ($Bytes.Length -eq 3),
                ($When.Ticks -eq 0),
                ($Offset.Ticks -eq 621355968000000000),
                ($Guid -eq [guid]'01234567-89ab-cdef-0123-456789abcdef'),
                ($Uri.Scheme -eq 'https'),
                ($Array.Count -eq 2 -and $Array[0] -eq 1 -and $Array[1] -eq 'two'),
                ($Bag.Name -eq 'snapshot' -and $Bag.Count -eq 3),
                ($Switch.IsPresent)
            ) -join '|'
        "#;
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    value_handle,
                    Utf8Span {
                        data: value_script.as_ptr(),
                        len: value_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        add_v2_parameter_value(value_handle, "Nothing", 0, &[], &mut call_result);
        add_v2_parameter_value(value_handle, "String", 1, b"tagged", &mut call_result);
        add_v2_parameter_value(value_handle, "Boolean", 3, &[1], &mut call_result);
        add_v2_parameter_value(value_handle, "Signed", 4, &(-7_i64).to_le_bytes(), &mut call_result);
        add_v2_parameter_value(value_handle, "Unsigned", 5, &(9_u64).to_le_bytes(), &mut call_result);
        add_v2_parameter_value(
            value_handle,
            "Double",
            6,
            &2.5_f64.to_bits().to_le_bytes(),
            &mut call_result,
        );
        add_v2_parameter_value(value_handle, "Decimal", 7, b"42.5", &mut call_result);
        add_v2_parameter_value(value_handle, "Bytes", 8, b"abc", &mut call_result);
        add_v2_parameter_value(value_handle, "When", 9, &0_i64.to_le_bytes(), &mut call_result);
        let mut offset = Vec::from(621355968000000000_i64.to_le_bytes());
        offset.extend_from_slice(&0_i16.to_le_bytes());
        add_v2_parameter_value(value_handle, "Offset", 10, &offset, &mut call_result);
        add_v2_parameter_value(
            value_handle,
            "Guid",
            11,
            b"01234567-89ab-cdef-0123-456789abcdef",
            &mut call_result,
        );
        add_v2_parameter_value(value_handle, "Uri", 12, b"https://example.test/path", &mut call_result);
        let first_array_item = 1_i64.to_le_bytes();
        let array = array_payload(&[(4, first_array_item.as_slice()), (1, b"two".as_slice())]);
        add_v2_parameter_value(value_handle, "Array", 13, &array, &mut call_result);
        let bag = property_bag_payload(&[("Name", 1, b"snapshot"), ("Count", 4, &3_i64.to_le_bytes())]);
        add_v2_parameter_value(value_handle, "Bag", 14, &bag, &mut call_result);
        add_v2_parameter_value(value_handle, "Switch", 2, &[1], &mut call_result);
        let value_result = v2_invoke(value_handle, &mut call_result);
        let mut state = 0;
        let mut invocation_id = 0;
        let mut had_errors = 0;
        assert_eq!(
            unsafe {
                multi_pwsh_result_get_metadata(
                    value_result,
                    &mut state,
                    &mut invocation_id,
                    &mut had_errors,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(state, 1);
        assert_ne!(invocation_id, 0);
        assert_eq!(had_errors, 0);
        assert_eq!(
            v2_result_output(value_result, &mut call_result),
            vec!["True|True|True|True|True|True|True|True|True|True|True|True|True|True|True".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(value_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(value_handle, &mut call_result) },
            Status::Success.value()
        );

        let command_handle = v2_create_session(&mut call_result);
        let switch_script = "param([switch] $Flag) if ($Flag) { 'switch' }";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8_local(
                    command_handle,
                    Utf8Span {
                        data: switch_script.as_ptr(),
                        len: switch_script.len(),
                    },
                    1,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let flag = "Flag";
        assert_eq!(
            unsafe {
                multi_pwsh_add_parameter_switch(
                    command_handle,
                    Utf8Span {
                        data: flag.as_ptr(),
                        len: flag.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_add_statement(command_handle, &mut call_result) },
            Status::Success.value()
        );
        let second_statement = "'multiple-statements'";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8_local(
                    command_handle,
                    Utf8Span {
                        data: second_statement.as_ptr(),
                        len: second_statement.len(),
                    },
                    0,
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        let command_result = v2_invoke(command_handle, &mut call_result);
        assert_eq!(
            v2_result_output(command_result, &mut call_result),
            vec!["switch".to_owned(), "multiple-statements".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(command_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(command_handle, &mut call_result) },
            Status::Success.value()
        );

        let input_handle = v2_create_session(&mut call_result);
        let input_script = "$input | ForEach-Object { $_ * 2 }";
        assert_eq!(
            unsafe {
                multi_pwsh_add_script_utf8(
                    input_handle,
                    Utf8Span {
                        data: input_script.as_ptr(),
                        len: input_script.len(),
                    },
                    &mut call_result,
                )
            },
            Status::Success.value()
        );
        add_v2_input_value(input_handle, 4, &3_i64.to_le_bytes(), &mut call_result);
        let mut incomplete_result = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(input_handle, &mut incomplete_result, &mut call_result) },
            Status::InputNotCompleted.value()
        );
        assert_eq!(call_result.status, Status::InputNotCompleted.value());
        assert!(std::str::from_utf8(&diagnostic[..call_result.diagnostic_written])
            .unwrap()
            .contains("CompleteInput"));
        assert_eq!(
            unsafe { multi_pwsh_reset_input(input_handle, &mut call_result) },
            Status::Success.value()
        );
        add_v2_input_value(input_handle, 4, &3_i64.to_le_bytes(), &mut call_result);
        add_v2_input_value(input_handle, 4, &4_i64.to_le_bytes(), &mut call_result);
        assert_eq!(
            unsafe { multi_pwsh_complete_input(input_handle, &mut call_result) },
            Status::Success.value()
        );
        let input_result = v2_invoke(input_handle, &mut call_result);
        assert_eq!(
            v2_result_output(input_result, &mut call_result),
            vec!["6".to_owned(), "8".to_owned()]
        );
        assert_eq!(
            unsafe { multi_pwsh_result_release(input_result, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(input_handle, &mut call_result) },
            Status::Success.value()
        );

        let backpressure_handle = v2_create_session(&mut call_result);
        for _ in 0..64 {
            add_v2_input_value(backpressure_handle, 4, &1_i64.to_le_bytes(), &mut call_result);
        }
        let extra_payload = 1_i64.to_le_bytes();
        let value = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind: 4,
            flags: 0,
            _reserved: 0,
            data: extra_payload.as_ptr(),
            data_len: 8,
        };
        assert_eq!(
            unsafe { multi_pwsh_add_input_value(backpressure_handle, &value, &mut call_result) },
            Status::Backpressure.value()
        );
        assert_eq!(call_result.status, Status::Backpressure.value());
        assert_eq!(
            unsafe { multi_pwsh_reset_input(backpressure_handle, &mut call_result) },
            Status::Success.value()
        );
        assert_eq!(
            unsafe { multi_pwsh_release(backpressure_handle, &mut call_result) },
            Status::Success.value()
        );

        let rejection_handle = v2_create_session(&mut call_result);
        let unsupported = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind: u32::MAX,
            flags: 0,
            _reserved: 0,
            data: std::ptr::null(),
            data_len: 0,
        };
        assert_eq!(
            unsafe { multi_pwsh_add_argument_value(rejection_handle, &unsupported, &mut call_result) },
            Status::UnsupportedValue.value()
        );
        assert_eq!(call_result.status, Status::UnsupportedValue.value());
        assert_eq!(
            unsafe { multi_pwsh_release(rejection_handle, &mut call_result) },
            Status::Success.value()
        );
    }

    fn v2_call_result(diagnostic: &mut [u8]) -> CallResult {
        CallResult {
            size: std::mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: diagnostic.as_mut_ptr(),
            diagnostic_capacity: diagnostic.len(),
            diagnostic_required: 0,
            diagnostic_written: 0,
        }
    }

    fn v2_call_result_without_diagnostic() -> CallResult {
        CallResult {
            size: std::mem::size_of::<CallResult>() as u32,
            status: 0,
            flags: 0,
            _reserved: 0,
            diagnostic: std::ptr::null_mut(),
            diagnostic_capacity: 0,
            diagnostic_required: 0,
            diagnostic_written: 0,
        }
    }

    fn initialize_v2(payload: &str, call_result: &mut CallResult) {
        assert_eq!(
            unsafe {
                multi_pwsh_initialize_utf8(
                    Utf8Span {
                        data: payload.as_ptr(),
                        len: payload.len(),
                    },
                    call_result,
                )
            },
            Status::Success.value()
        );
        assert_eq!(call_result.status, Status::Success.value());
    }

    fn v2_create_session(call_result: &mut CallResult) -> u64 {
        let mut handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_create(&mut handle, call_result) },
            Status::Success.value()
        );
        handle
    }

    fn add_v2_parameter_value(handle: u64, name: &str, kind: u32, payload: &[u8], call_result: &mut CallResult) {
        let value = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind,
            flags: 0,
            _reserved: 0,
            data: payload.as_ptr(),
            data_len: payload.len(),
        };
        assert_eq!(
            unsafe {
                multi_pwsh_add_parameter_value(
                    handle,
                    Utf8Span {
                        data: name.as_ptr(),
                        len: name.len(),
                    },
                    &value,
                    call_result,
                )
            },
            Status::Success.value()
        );
    }

    fn add_v2_input_value(handle: u64, kind: u32, payload: &[u8], call_result: &mut CallResult) {
        let value = DataValue {
            size: std::mem::size_of::<DataValue>() as u32,
            kind,
            flags: 0,
            _reserved: 0,
            data: payload.as_ptr(),
            data_len: payload.len(),
        };
        assert_eq!(
            unsafe { multi_pwsh_add_input_value(handle, &value, call_result) },
            Status::Success.value()
        );
    }

    fn v2_invoke(handle: u64, call_result: &mut CallResult) -> u64 {
        let mut result_handle = 0;
        assert_eq!(
            unsafe { multi_pwsh_invoke(handle, &mut result_handle, call_result) },
            Status::Success.value()
        );
        result_handle
    }

    fn v2_result_output(result_handle: u64, call_result: &mut CallResult) -> Vec<String> {
        let mut count = 0;
        let mut flags = 0;
        assert_eq!(
            unsafe { multi_pwsh_result_get_stream_info(result_handle, 0, &mut count, &mut flags, call_result) },
            Status::Success.value()
        );
        let mut values = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut required = 0;
            let status = unsafe {
                multi_pwsh_result_copy_stream_record_field_utf8(
                    result_handle,
                    0,
                    index,
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut required,
                    call_result,
                )
            };
            assert!(status == Status::Success.value() || status == Status::BufferTooSmall.value());
            let mut value = vec![0_u8; required];
            assert_eq!(
                unsafe {
                    multi_pwsh_result_copy_stream_record_field_utf8(
                        result_handle,
                        0,
                        index,
                        0,
                        value.as_mut_ptr(),
                        value.len(),
                        &mut required,
                        call_result,
                    )
                },
                Status::Success.value()
            );
            values.push(String::from_utf8(value).unwrap());
        }
        values
    }

    #[test]
    fn secret_parameter_values_are_private_to_parameter_binding() {
        let secret_utf16 = [0x41_u8, 0, 0x42, 0];
        assert!(validate_secret_parameter_value(SECRET_VALUE_KIND_UTF16, &secret_utf16).is_ok());
        assert_eq!(
            validate_value_payload(SECRET_VALUE_KIND_UTF16, &secret_utf16, 0)
                .expect_err("ordinary tagged values must reject secret kinds")
                .0,
            Status::UnsupportedValue
        );

        let mut credential = Vec::new();
        credential.extend_from_slice(&4_u32.to_ne_bytes());
        credential.extend_from_slice(b"user");
        credential.extend_from_slice(&secret_utf16);
        assert!(validate_secret_parameter_value(SECRET_VALUE_KIND_CREDENTIAL, &credential).is_ok());
    }

    #[test]
    fn time_span_tagged_values_require_eight_little_endian_tick_bytes() {
        assert!(validate_value_payload(VALUE_KIND_TIME_SPAN, &(-1_i64).to_le_bytes(), 0).is_ok());
        assert_eq!(
            validate_value_payload(VALUE_KIND_TIME_SPAN, &[0_u8; 7], 0)
                .expect_err("short TimeSpan payload must fail")
                .0,
            Status::InvalidArgument
        );
        assert_eq!(
            validate_value_payload(VALUE_KIND_TIME_SPAN, &[0_u8; 9], 0)
                .expect_err("long TimeSpan payload must fail")
                .0,
            Status::InvalidArgument
        );
    }

    #[test]
    fn secret_parameter_values_reject_malformed_payloads() {
        assert!(validate_secret_parameter_value(SECRET_VALUE_KIND_UTF16, &[0x41]).is_err());
        assert!(validate_secret_parameter_value(SECRET_VALUE_KIND_UTF16, &[0, 0]).is_err());
        assert!(validate_secret_parameter_value(SECRET_VALUE_KIND_CREDENTIAL, &[0, 0, 0, 0]).is_err());
    }

    fn property_bag_payload(entries: &[(&str, u32, &[u8])]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (name, kind, value) in entries {
            payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
            payload.extend_from_slice(name.as_bytes());
            payload.extend_from_slice(&kind.to_le_bytes());
            payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
            payload.extend_from_slice(value);
        }
        payload
    }

    fn array_payload(entries: &[(u32, &[u8])]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (kind, value) in entries {
            payload.extend_from_slice(&kind.to_le_bytes());
            payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
            payload.extend_from_slice(value);
        }
        payload
    }
}
