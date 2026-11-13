# In-process PowerShell FFI experiment

`pwsh-host` can be exposed through the `pwsh-sdk-ffi` Rust `cdylib`.
The library receives a caller-selected PowerShell payload directory, or resolves
`pwsh` from `PATH`, then loads that payload's `hostfxr`, initializes `pwsh.dll`, injects
`Devolutions.PowerShell.SDK.Bindings`, and invokes its unmanaged function table.

The `dotnet/sdk-ffi` facade uses only `LibraryImport`. It has no
`System.Management.Automation` or `Microsoft.PowerShell.*` dependency.

## Experimental runtime boundary

The `dotnet/nativeaot-sample` NativeAOT executable has been exercised on Windows x64
with PowerShell 7.4 and 7.7 payloads in separate runs, each in the **same
process** as the hosted runtime. It creates a pipeline and receives script
output through the Rust `cdylib`.

This is an acceptance experiment, not a claim that NativeAOT plus a dynamically
hosted CoreCLR is a supported general .NET topology. The runtime host rejects
incompatible initialization. Do not rely on dynamic runtime switching, multiple
PowerShell runtime versions, or runtime unloading in the same process.

### Experimental live-object probe

`PowerShellRuntime.CreateLiveObjectProbe` is a deliberately narrow,
non-production `IUnknown` experiment. It creates a payload-local
`PSCustomObject` with one numeric `Count` property and projects only
`GetCount` and `Increment` through a source-generated, fixed-GUID COM
interface. It is portable userland `ComWrappers` use: it does not use Windows
COM activation, registry metadata, BSTR/automation marshalling, or classic
runtime COM interop.

The `net10.0` facade receives an opaque interface pointer, creates a unique
generated RCW, and immediately releases the producer's transit reference. The
RCW owns its reference until `PowerShellLiveObjectProbe.Dispose`, which calls
`ComObject.FinalRelease` after unregistering the payload identity entry; using
the wrapper afterward throws
`ObjectDisposedException`. `PowerShell.AddArgument(PowerShellLiveObjectProbe)`
returns the original pointer to the payload binding, which accepts only
pointers it created and restores the original payload object rather than
accepting arbitrary foreign `IUnknown` values.

This is currently validated only by the Windows x64 NativeAOT sample with an
explicit PowerShell 7.4 or 7.7 payload. It must be used only after the creating
invocation has completed and never from a capability callback or while a
pipeline is active. The existing process-global operation lock does not make
arbitrary direct proxy calls, reentrancy, runspace affinity, arbitrary member
invocation, or cross-platform payload activation supported. The probe is not a
general mechanism for exposing `PSObject`, SMA objects, delegates, hosts,
credentials, reflection, or ordinary managed object references to the
consumer runtime.

### Experimental reverse session-object probe

`PowerShellSessionObjectProbe` proves the inverse direction. A .NET 10-owned
broker exports an `IUnknown` transfer reference to
`PowerShellSession.SetLiveObjectVariable`; the payload creates a unique
generated RCW, wraps it in a small PowerShell-visible payload proxy, and
places that proxy in the session's `SessionStateProxy`. PowerShell can invoke
only the proxy's fixed `Count` and `Increment` members, whose bodies call the
generated interface and run in the .NET 10 broker.

The payload retains each projected RCW by variable name and calls
`ComObject.FinalRelease` when its value is replaced through either session
setter, removed with `RemoveVariable`, or the session owner is released. The
.NET 10 facade releases its transfer reference immediately after a successful
or failed assignment, but the managed probe itself must outlive its
session-variable binding. Remove the variable before disposing the probe.

This is a narrow Windows x64 / PowerShell 7.4 and 7.7 acceptance experiment, not
support for arbitrary .NET 10 objects in PowerShell session state. PowerShell
scripts can remove, overwrite, or alias the variable independently. The
payload reconciles its tracked bindings when each pipeline ends: it releases a
proxy once no session variable references it and retains it while an alias
still exists. The session mutation lock still rejects changes during an active
pipeline, and the proxy must not call back into the FFI API.

### Preview contract packs

`PowerShellLiveObjectContract` and `PowerShellLiveObject<TContract>` provide
the preview path for application-owned, consumer-to-session live objects.
Applications compile one source-generated `IUnknown` interface into their
`net10.0` consumer and their trusted `net8.0` payload adapter. The contract
contains a non-empty interface GUID, major/minor version, and direction flags;
methods must use preserved HRESULT returns and explicitly ABI-safe values.

The consumer activates with
`PowerShellRuntime.Activate(contractPacks)` or
`PowerShellRuntime.Activate(payloadDirectory, contractPacks)`. Each
`PowerShellLiveObjectContractPack` names an absolute payload-adapter assembly
and its assembly-qualified static export type. During activation, the host
loads the adapter into the selected payload runtime and invokes its
`GetLiveObjectContractPackV1` unmanaged export. That export supplies a bounded
contract descriptor list plus create/release callbacks for PowerShell-visible
payload proxies. The payload registry rejects unknown, duplicate,
direction-incompatible, or metadata-incompatible contracts before it projects
an `IUnknown`.

The `dotnet/live-object-test-pack` project is an acceptance fixture, not a
shipping application contract. It proves that a separately compiled net8
adapter can project a .NET 10 broker into a `PowerShellSession`, expose
ordinary scalar members, nested broker members, and a C# indexer as
PowerShell-visible properties and `Children[index]` access. The acceptance
broker mutates a nested child through both `Primary` and `Children[0]` to
verify that separately projected RCWs retain the same consumer-owned child
identity. It retains live state across invocations and preserves a
session-variable alias.

Contract packs are deliberately explicit and trusted application code. They
cannot expose raw pointers through PowerShell or turn arbitrary CLR objects
into contracts. The current external-pack registry supports only the proven
consumer-to-session direction. Payload-to-consumer object contracts,
cross-session identity, compatibility relaxation, and SDK-owned
application-specific schema generation remain future work.

#### Contract packs are a coordinated breaking release

There is no contract-pack version negotiation, compatibility manifest, accepted
version range, or reused-identifier policy, and none is planned. A contract's
interface identifier, version tuple, direction flags, operation shape, and the
`GetLiveObjectContractPackV1` pack ABI are one indivisible agreement between a
consumer and its payload adapter. Changing any of them is a breaking change:
build and ship both sides together, and re-run the consuming application's own
acceptance tests against the new pair.

The registry fails activation loudly rather than degrading. Each rejection below
is exercised end to end by the self-contained Win-x64 NativeAOT sample using the
fixtures in `dotnet/live-object-incompatible-test-pack`, run as
`NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:<fixture>`:

| Fixture | Declared defect | Rejection |
| --- | --- | --- |
| `duplicate-across-packs` | Two packs in one activation declare the same interface identifier | `duplicate interface identifiers` |
| `duplicate-within-pack` | One pack declares the same interface identifier twice | `duplicate interface identifiers` |
| `direction-violation` | Contract omits `ConsumerToSession` | `unsupported direction` |
| `reserved-identifier` | Pack re-declares an identifier the payload already owns | `has already been registered` |
| `unsupported-pack-abi` | Pack reports an unimplemented `AbiVersion` | `contract pack API is invalid` |
| `bridge-marker-without-direction` | Raw descriptor sets the Bridge Contract v2 marker without `ConsumerToSession` | `must be declared together with ConsumerToSession` |

Because activation is all-or-nothing, a rejected pack never leaves a partially
registered contract behind, and there is no path by which a stale consumer
silently binds a newer payload contract.

The transfer is `IUnknown` only: the NativeAOT consumer creates a
source-generated CCW, Rust retains no object graph and forwards its transfer
reference, and the trusted payload adapter creates source-generated RCWs before
wrapping them in ordinary fixed managed proxy classes. PowerShell binds those
managed proxy members; it does not bind a raw COM object and requires neither
`IDispatch` nor a dynamic proxy. This proves direct in-process generated-COM
calls on the serialized local-pipeline path, not apartment marshalling. Contract
brokers must therefore be thread-safe and treated as MTA/agile-only until an
application supplies and validates an explicit apartment dispatcher or free
threaded marshaler strategy.

### Live-object execution policy

Direct calls from a PowerShell-visible payload proxy to a consumer broker are
allowed only when the broker records bounded consumer-owned intent. Such a
call must not invoke the native FFI, start another pipeline, access payload
session state, call capability RPC, or mutate UI/runspace-affine state.

Contract packs must treat the source-generated interface as the complete
allowlist: interface IIDs, member ordinals, parameter/return types, numeric
ranges, collection bounds, HRESULTs, and any authorization decision are
application-owned static contract data. The SDK does not inspect a COM vtable
for a per-member schema, redact secrets, or supply a cancellation token to a
direct COM call. Do not include `SecureString`, credential, arbitrary object,
or secret-bearing string members. Reads must complete synchronously within
application-defined bounds; writes should only stage validated intent and
commit it after the PowerShell invocation reaches a successful terminal state.

The native bridge tracks active pipeline execution process-wide. General FFI
calls fail with `Backpressure` while any pipeline is active, including calls
forwarded to another thread by a broker; calls from a capability callback also
fail before they acquire runtime or operation locks. Cancellation, operation
polling/waiting, session snapshots/events, and immutable-result reads remain
available while a pipeline runs. Session-variable reads and mutations fail
with `Backpressure` before lock acquisition. These rules prevent deadlock;
they do not make a live PowerShell object or runspace callable from a broker.
A session-affine dispatcher is required before supporting that category.

## Lifecycle and ABI

- The direct activation exports are process-global and accept one canonical
  payload directory. Repeating the same activation succeeds; selecting a
  different payload returns `MULTI_PWSH_INCOMPATIBLE_PAYLOAD`.
- The native product ABI reports its compatible version and feature flags through
  `multi_pwsh_get_abi_info`; the managed package and native asset ship together
  and use the unversioned `multi_pwsh_*` exports. The injected managed function
  table independently reports its compatibility version.
- The public native ABI remains v2. `ReadStreamBatch` is enabled only when the
  `LIVE_STREAM_POLLING` feature bit is present, so consumers can reject a native
  asset that lacks polling before calling its additive export.
- Generated Bridge Contract v2 attachment requires feature bit 26, bounded
  broker terminal observation requires bit 27, and retained generated bridge
  events require bit 28. Structured observed presentation requires bit 29.
  Bit 28 adds no payload callback slot; bit 29 appends the one
  `ObservedDiagnosticPage_CopyRecordValue` slot. Both make the V1 payload
  table fail closed unless the payload and native host recognize the current
  shape. A current facade therefore rejects an old payload before it creates a
  bridge channel or reads a typed progress presentation record; ABI v2 field
  ordering and existing exports remain unchanged.
- The managed payload and Rust host use one jointly shipped **V1 payload binding
  table**. It includes the core, live-stream, typed-result, and bounded runtime
  diagnostics slots, including the appended observed-progress value-copy slot.
  This is
  separate from the public native ABI, which remains v2. Rust validates the
  fixed V1 header (size, version, features) before reading an extended table
  field, then requires the full current table size.
- The supported ABI uses sized, caller-owned `multi_pwsh_call_result` structures. Each operation
  returns its own bounded UTF-8 diagnostic and truncation metadata, so concurrent
  calls never read a process-global error slot. `multi_pwsh_utf8_span` permits
  `(NULL, 0)` for an empty value; the library never retains caller memory.
- The caller owns all input and output buffers. UTF-8 fields use a two-pass
  required-length query and never retain caller memory.
- `multi_pwsh_create` returns an opaque builder handle. `multi_pwsh_release`
  disposes its managed `GCHandle`; handles are never reused by the current
  process. `multi_pwsh_invoke` instead returns a distinct immutable result
  handle. Release it explicitly with `multi_pwsh_result_release`; it remains
  readable after the builder is cleared, mutated, or released.
  - `multi_pwsh_result_get_metadata` reports the immutable invocation ID,
    terminal synchronous state (`Completed` or `Terminated`), and `had_errors`
    bit alongside the retained stream snapshot.
- Exports contain Rust panics and return fixed status codes without unwinding
  into the caller.
- Normal pipeline execution and builder-mutation calls are serialized by the
  cdylib's **process-global** operation lock, including calls made through
  otherwise independent builders and sessions. This ABI version does not
  support parallel independent-session execution. `Stop` is the exception:
  builder and operation stop requests may run concurrently with the active
  pipeline so cancellation is not blocked behind `Invoke`.
- A v2 session is a separate opaque handle, not a builder alias. A session owns
  (or narrowly borrows) one local runspace; builders created by
  `multi_pwsh_session_create_builder` keep an internal managed lease, so they
  remain valid if the public session handle is released first. Session release
  prevents new builders and closes an owned runspace after the final builder
  lease is released. Immutable results and operations remain independently
  owned as before.
- Explicit native release consumes its raw handle: a second release or any
  later use deterministically returns `MULTI_PWSH_INVALID_HANDLE`. In contrast,
  the facade's `IDisposable` implementations are idempotent and their
  `SafeHandle` leases keep an in-flight facade call alive until that call
  returns.

## Payload selection and activation

`PowerShellRuntime.Activate()` resolves `pwsh` from `PATH` and activates its
containing payload directory. `PowerShellRuntime.Activate(payloadDirectory)`
activates exactly the caller-selected existing directory. The native host
canonicalizes that directory before loading its local `hostfxr`.

The SDK deliberately does not read, package, generate, or validate a payload
metadata file. It does not pin a PowerShell, hostfxr, CoreCLR, module, or file
version. The application selects the PowerShell installation and is responsible
for any provenance or integrity controls its deployment requires.

One payload is process-global. Repeating activation for the same canonical
directory succeeds; selecting a different directory after initialization fails
with `IncompatiblePayload`. Runtime switching and unloading are unsupported.

### Async operations and cancellation

`multi_pwsh_invoke_async` returns an opaque operation handle. It is a genuine
native-owned invocation: the Rust bridge retains the builder and its managed
pipeline until the operation reaches a terminal state. It is not a managed
`Task.Run` wrapper around synchronous `Invoke`.

- `multi_pwsh_operation_poll` and `multi_pwsh_operation_wait` report
  `Pending`, `Running`, `Completed`, `Cancelled`, or `Failed`, together with
  the terminal status. A wait timeout reports the current non-terminal state;
  `UINT32_MAX` waits indefinitely.
- `multi_pwsh_operation_stop` is idempotent. The first stop request wins over
  any concurrent successful completion: the operation becomes `Cancelled`,
  returns `MULTI_PWSH_OPERATION_CANCELLED_STATUS`, and never exposes an
  immutable result. This prevents a caller from accepting partial script
  output as a successful result. Repeated stop calls and stop after a terminal
  state succeed.
- A completed operation exposes immutable result snapshots through
  `multi_pwsh_operation_get_result`. Retrieval before terminal completion
  returns `MULTI_PWSH_OPERATION_NOT_TERMINAL`; cancelled and failed operations
  return their terminal status plus the operation's bounded (4 KiB) diagnostic.
  Result handles are independently released with
  `multi_pwsh_result_release` and can outlive the operation handle.
- `multi_pwsh_operation_release` is explicit single-owner release. Releasing an
  active operation first requests cancellation, then detaches the caller's
  handle. Existing native calls and the worker retain internal references, so
  no release race can free a pipeline while it is executing. The raw operation
  handle is then stale and a repeated release deterministically returns
  `MULTI_PWSH_INVALID_HANDLE`; `PowerShellInvocationOperation.Dispose()` remains
  idempotent. Releasing a builder with an active operation likewise requests
  cancellation.
- A builder is immutable while its async operation is active. Builder mutation,
  synchronous invocation, and input calls return `MULTI_PWSH_BACKPRESSURE`
  until terminal completion. Builder `Stop` targets its active operation and
  has the same cancellation-wins semantics.

### Live stream polling

`PowerShellInvocationOperation.ReadStreamBatch(afterSequence, maximumRecords)`
is a polling-only, copied-record view of a running operation. `afterSequence`
is a record-sequence cursor where `0` starts at the beginning; a cursor beyond
the newest sequence or a limit outside `1..32` is rejected. Each returned
record contains only its stream kind, a native-owned monotonic nonzero
sequence, a fixed-bounded copied display-text field, and a truncation bit. It
never carries `PSObject`, `ErrorRecord`, SMA state, CLR objects, credentials,
callbacks, or pipeline input.

The batch is immutable and includes `NextSequence`, operation state and
terminal status, cursor-loss metadata, total/dropped/source-dropped counters,
and truncation metadata. The native operation owns a 32-record ring; reading a
cursor older than its retained range succeeds with `IsCursorLost` and
`LostRecordCount` rather than silently skipping data. The payload-to-native
capture path is separately bounded, and source eviction is reported through
`SourceDroppedRecordCount`. Stream records can be read while an operation is
running and remain readable after completion or cancellation, but cancellation
still wins: `GetResult` never converts partial live output into a successful
result.

The facade projects this model as `PowerShell.BeginInvoke()` and
`PowerShell.InvokeAsync(CancellationToken)`. `PowerShellInvocationOperation`
offers `Poll`, bounded `Wait`, `Stop`, and `GetResult`. Token cancellation
calls the native stop primitive, then waits for the native terminal state
before completing the task as cancelled; it never abandons or prematurely
releases the operation handle. The facade uses `LibraryImport` and
`SafeHandle` leases only—no SMA objects, native callbacks, or collectible
managed delegates cross the boundary.

### Typed result paging

`PowerShell.BeginTypedResultInvocation(options)` is the lossless result-data
counterpart to live stream polling. It is part of the required V1 payload
binding table; the public native ABI remains v2. The caller chooses bounded
record and page limits (1 through 64), then calls
`Read(acknowledgedThrough, maximumRecords)`. Supplying a newer cursor
explicitly acknowledges the previous page and releases its records; re-reading
an earlier cursor does not acknowledge or discard anything.

The payload producer blocks when the bounded queue is full. It never drops
values to make room, and it returns `UnsupportedValue` when output cannot be
represented as a documented tagged value rather than converting it to display
text. Terminal pages carry cancellation, truncation, dropped-record, and
completion state. `IsComplete` is true only after successful completion and
acknowledgement of every produced record. This remains copied data only:
arrays and property bags are bounded `PowerShellValue` values, never `PSObject`
or arbitrary CLR-object transfer.

### Observed presentation paging

`PowerShell.BeginObservedInvocation(options)` has independent result and
diagnostic acknowledgement cursors. `ReadResults` returns copied tagged output
values. For presentation, a host chooses exactly one reader for the diagnostic
cursor sequence:

- `ReadDiagnostics` returns the existing copied stream kind, sequence, and
  bounded display text.
- `ReadPresentation` returns the same cursor/page metadata and display text,
  plus a fixed `PowerShellProgressUpdate` for a `Progress` record.

The readers cannot be interleaved for one cursor: passing a later cursor
acknowledges and releases the previous page regardless of which representation
read it. A page can be retried before acknowledgement. `IsComplete` remains
true only after both channels have successfully terminated and all their
records were acknowledged.

`CreateTranscript()` provides a commit-acknowledged pull wrapper for consumers
that persist or render records incrementally. `ReadResults()` and
`ReadPresentation()` retain the same immutable page until
`CommitResults(page)` or `CommitPresentation(page)` succeeds. The two commits
advance independent result and presentation cursors, so a consumer can commit
only after its own durable/UI-model update completes. Each returned page
retains total/dropped/truncated counts and terminal status; transcript callers
must not issue direct observed reads for that invocation.

The progress projection is a closed copied property bag with only
`ActivityId`, `ParentActivityId`, `Activity`, optional `StatusDescription` and
`CurrentOperation`, `PercentComplete`, `SecondsRemaining`, and `IsCompleted`.
It rejects non-progress values, unsupported fields, invalid ranges, text over
512/1024 characters, malformed tagged values, and payloads over 64 KiB rather
than parsing `ProgressRecord.ToString()` or falling back to a dynamic object.
All other stream kinds remain bounded display text. This is a pull-only
presentation path for `Write-Information`/stream output and `Write-Progress`;
it is not a `PSHost`, RawUI, prompt-response API, arbitrary callback, or
secret transport.

### Sessions, bounded settings, and polling

`PowerShellRuntime.CreateSession(PowerShellSessionOptions)` creates a reusable
`PowerShellSession`. It deliberately offers only copied, declarative settings:

- `NewRunspace` creates and owns a local runspace. Its initial configuration is
  either PowerShell's default local state or `ConstrainedLanguage`. Its
  `PowerShellSessionConfiguration` can set copied tagged initial variables,
  approved module paths/imports, one approved working directory, allowlisted
  environment values, and the `Default`/`Restricted` execution-policy subset.
  It is not an arbitrary `InitialSessionState` or module-loading API.
- `CurrentRunspace` is a narrow ambient-runspace opt-in. It requires an already
  opened default runspace and rejects all configuration, history, preferences,
  and module-path changes, so this boundary cannot mutate the embedding
  application's current runspace.
- History is a Boolean invocation setting. Error, warning, verbose, debug, and
  information preferences are limited to inherit, continue, silently continue,
  or stop. Interactive preference modes are rejected because the ABI has no
  prompt channel. Error preference is also passed through a bounded
  `PSInvocationSettings`; no settings object leaves managed code.

Session configuration is supplied directly by the application. Requested module
paths and working directories must be absolute existing directories; module
imports are bounded names resolved only beneath the supplied module paths. The
payload-local authorization manager permits external scripts only beneath those
module roots. This is not a general authorization bypass or an additional
module search path. `CurrentRunspace` rejects all configuration so the ABI does
not mutate ambient application state.

`GetSnapshot()` and `GetEvents()` are polling APIs. A snapshot reports copied
session/runspace state, active pipeline count, invocation and history counts.
Events are numeric state records only—no callback function pointer, delegate,
or managed object crosses the ABI. At most 32 events are retained; overflow is
reported by the snapshot's `AreEventsTruncated` flag. Pipelines on the same
session are serialized by the in-process host's operation boundary and the
process-global normal-operation lock. A snapshot does not authorize concurrent
live runspace use, including across separate sessions.

The lifecycle is explicit even though configuration/import are synchronous:
construction validates configuration and approved imports before returning an
`Opened` session; invocation reports `Running`; stop/cancellation returns to
`Opened` after terminal cleanup; owner release is `Closed`; an unrecoverable
runspace failure reports `Faulted`. A closed public session cannot create
builders, while an already-created builder retains its managed lease until it
is released. There is no concurrent session mutator model: normal operations
are process-globally serialized and copied-variable access rejects an active
async session operation.

`PowerShellSessionPoolOptions` and `CreateSessionPool` form an explicit
rejection boundary. `multi_pwsh_session_pool_create` validates a bounded
maximum of 1–64 sessions (with a minimum no greater than that) then always returns
`MULTI_PWSH_UNSUPPORTED_CAPABILITY`: the current single CoreCLR, local-runspace
model has no safe pool lifecycle/concurrency implementation. It does not
pretend that serializing one runspace is a pool.

#### Session snapshots are not durable restores

`PowerShellSessionSnapshot` and session events are copied telemetry, not a
serialized runspace image. The SDK intentionally exposes no variable
enumeration, module inventory, restore operation, or general runspace
serialization. A preflight report validates the caller's current roots and
static import declarations, but it does not seal module identity, hash content,
or atomically bind preflight to a later session creation. Treating it as a
durable approved-import manifest would create a filesystem TOCTOU and module
provenance policy that the generic SDK cannot own.

An application that needs continuity must explicitly rehydrate its own
reviewed, non-secret DTOs into a new `PowerShellSessionConfiguration` and
copied session variables, then make its own authorization and module-integrity
decision. This SDK provides neither persistence nor a claim that a local
module is trusted across process lifetimes.

The proxy surface is `Create`, `AddCommand`/`AddScript` (including
`useLocalScope`), `AddArgument`, typed and switch parameters, `AddStatement`,
`Clear`, bounded input feeding, result-oriented `Invoke`, `Stop`, and
`Dispose`. `AddArguments` and `AddParameters` are convenience bulk builders;
each item is validated and appended in order, so callers should `Clear` to
discard a partially built pipeline after an item fails.

## Tagged values and input

`multi_pwsh_data_value` is the only native value-transfer envelope. Its `kind`,
bounded caller-owned payload, and zero flags are copied synchronously by the
call. It never carries a CLR object, `PSObject`, delegate, handle, reflection
request, or JSON document. The facade exposes the same contract as immutable
`PowerShellValue` instances:

- null, string, switch, Boolean, signed/unsigned 64-bit integer, double,
  invariant decimal text, bytes, `DateTime`, `DateTimeOffset`, `Guid`, and
  absolute `Uri`;
- arrays of at most 64 tagged values;
- property bags of at most 64 unique string keys. The payload is copied to a
  fresh managed `PSObject` with note properties, so it is a
  PSCustomObject-style snapshot rather than a live caller object.

Payloads are capped at 64 KiB and containers nest at most eight levels. Fixed
numeric payloads are little-endian; `Guid` uses canonical UTF-8 `D` text,
decimal uses invariant UTF-8 text, `DateTime` is `DateTime.ToBinary()`, and
`DateTimeOffset` is ticks followed by a signed little-endian offset-minute
count. Array payloads are `u32 item-count` followed by `u32 kind`, `u32
length`, and payload for each item. Property bags use `u32 item-count`,
`u32 key-length`, UTF-8 key, then that same nested value record. These binary
envelopes are deliberately not JSON.

`PowerShellValue.From(object?)` converts only that documented set and nested
property bags/arrays. It throws `PowerShellValueConversionException` for
delegates and unsupported CLR objects; no facade builder overload accepts a
raw `object`.

`AddInput` copies one tagged value into a synchronous per-builder collection.
The collection accepts at most 64 values and 64 KiB of payload; exceeding
either bound returns `MULTI_PWSH_BACKPRESSURE`. Call `CompleteInput` before
`Invoke` after starting input. Invoking an uncompleted collection returns
`MULTI_PWSH_INPUT_NOT_COMPLETED` and retains the input so it can be completed or
`ResetInput` can discard it. Invoking without ever starting input retains the
ordinary no-input behavior. Completion, reset, clear, release, and invocation
all discard the managed input collection after its defined use, preventing
reuse from retaining unreported input.

Async input intentionally supports only that bounded producer model: add up to
64 tagged values, call `CompleteInput`, then start the operation. Streaming a
producer concurrently with `InvokeAsync`, adding input after start, and
reopening completed input are intentionally unsupported. This preserves an
explicit copied-input lifetime and deterministic cancellation boundary.

`multi_pwsh_result_get_stream_info` and indexed record APIs expose output,
error, warning, verbose, debug, information, and progress. Retained records
carry a global capture sequence available through
`multi_pwsh_result_get_sequence_record`. Each stream retains at most 32
records; each field retains at most 4,096 UTF-16 code units.
`multi_pwsh_result_get_stream_totals` reports all observed and dropped
records, not merely the retained ring. Result, stream, and record flags report
terminating invocation, dropped sequence/stream records, and field truncation.
Managed stream buffers are cleared before and after every result invocation,
including streams the caller does not read.

The facade projects this into immutable `PowerShellInvocationResult` snapshots:
output is a `PowerShellObjectSnapshot` with display text, up to eight copied
type-label strings, and an optional copied tagged scalar. Type labels are
declarative text only; they do not expose, bind, or retain a CLR type or object
identity. The optional property bag is built only from `PSNoteProperty` values
that convert to documented tagged scalars. It retains at most 16 ordinal-sorted
keys of at most 128 UTF-16 code units, a 1 KiB scalar payload per value, and a
16 KiB total envelope. Complex values and enumerables are neither traversed nor
enumerated; they are dropped and the retained/dropped entry counts and
truncation flag make that loss explicit.

Errors add copied category reason/activity/target labels, command and source
location text, pipeline coordinates, details/recommended-action text, target
display text, and an optional scalar target projection. These are bounded
strings or tagged copies, never `ErrorRecord` or `InvocationInfo` references.
There is intentionally no per-record terminal flag: PowerShell does not expose
one reliably for every captured error. `IsTerminatingFailure` is the reliable
result-level terminal indicator. A terminating invocation throws
`PowerShellInvocationException`, whose `InvocationResult` retains that bounded
snapshot.

`PowerShellSnapshotSerializer` serializes and restores those immutable facade
snapshots as deterministic, versioned UTF-8 JSON (format version 1). It is for
storage or display only, capped at 1 MiB and JSON depth-limited to 16 by the
source-generated DTO/parser contract. It rejects unknown members, unsupported versions, malformed
tagged-value envelopes, oversized values, and invalid stream totals. It never
deserializes into PowerShell, SMA, a parent CLR object, or an arbitrary CLR
type.

Call `PowerShellRuntime.Activate()` to select `pwsh` from `PATH`, or
`PowerShellRuntime.Activate(payloadDirectory)` to select an explicit payload.
Then use `runtime.Create()` (or the equivalent `PowerShell.Create()`
process-global entry point) to construct builders. The runtime object reports
the selected path and negotiated ABI/features; it does not permit selecting a
second payload or unloading the selected runtime.

`PowerShellRuntime.Diagnostics` is a read-only descriptive report for the
active runtime. It contains the canonical payload directory, an explicitly
nullable PowerShell file version, the V1 payload table ABI/size/slot shape,
enabled feature flags, and registered contract-pack adapter type names. It is
not an integrity or deployment-policy result: it returns no payload hashes,
assembly paths, environment dump, secret, or payload object, and it cannot
mutate runtime state. The payload directory is the runtime's canonicalized
active payload directory rather than a verbatim echo of an activation argument:
on Windows it is an extended-length `\\?\` path, and `PATH`-based activation
discovers it at runtime, so consumers must not compare it against their own
input string. Because it is a local filesystem path it can embed a user profile
directory, so redact it before writing the report to shared logs or external
telemetry.

## Facade scope

This SDK offers copied-DTO compatibility, not SMA compatibility. The table
below defines the generic managed facade boundary.

| Application need | Status | FFI replacement and boundary |
| --- | --- | --- |
| Explicit payload activation, hostfxr startup, one runtime per process | Implemented | Direct payload or `PATH` activation and an opaque `PowerShellRuntime`; only `win-x64` has NativeAOT smoke evidence. |
| Script or named-command execution with scalar parameters | Implemented | `AddScript`, `AddCommand`, `AddParameter`, and bounded `PowerShellValue` inputs. No raw `object` or `PSObject` crosses the boundary. |
| Script parameter declarations and syntax errors | Implemented, copied-only | `PowerShellRuntime.ParseScriptParameters` passes the input to payload-local `Parser.ParseInput` as data, never executable pipeline text. It returns bounded parameter/parse-error DTOs, not SMA AST or token objects. |
| Output, errors, diagnostics, warning/progress streams | Implemented | Immutable bounded snapshots, typed result pages, and observed result/diagnostic cursors. `ReadPresentation` adds fixed copied progress fields for pull-based text/progress rendering; it never returns SMA records or objects. |
| Timeout, cancellation, async completion, deterministic disposal | Implemented | `InvokeAsync(CancellationToken)`, `BeginInvoke`, `Wait`, `Stop`, and `SafeHandle` ownership. Cancellation wins and never returns a partial success result. |
| Long-lived local state | Implemented, local-only | Opaque local `PowerShellSession` plus serialized builders. `PowerShellRemoteSession` retains one payload-local PSSession and returns fixed copied metadata plus copied invocation results; generic durable restore and pools are excluded. |
| `SessionStateProxy.SetVariable` for value data | Implemented, copied-only | `SetVariable`, `TryGetVariable`, and `RemoveVariable` transfer bounded tagged values only. No methods, proxies, handles, or CLR identity survive the boundary. |
| Application-selected local modules | Implemented, local-only | `PowerShellModuleImport` declares a bounded name/path, required version, and finite import options. Paths remain subject to caller-supplied roots; this does not validate PowerCLI or remoting dependencies. |
| `SecureString` / `PSCredential` parameter | Implemented, explicit lease only | `PowerShellSecret` and `PowerShellCredential` create payload-local SMA values. Secret-bound pipelines reject ordinary output, diagnostics, paging, and async paths; the explicit invocation API returns only no result, one leased secure string, or one leased credential. |
| Enumerated application capability calls and bounded host interaction | Implemented, opt-in | A registered `PowerShellCapabilitySet` makes only declared typed calls available through the temporary payload-local `$DpsCapabilities` object. `PowerShellHostInteraction` supplies schemas for text, progress, line, and choice interactions; it is not a `PSHost` proxy. |
| Asynchronous application requests from a pipeline without a callback on the pipeline thread | Implemented, opt-in | The Duplex Broker Channel. Strictly dispatch-only: a pump copies a bounded opaque frame, releases it, and replies later by correlation ID. It carries no CLR object, delegate, secret, or self-describing wire format. |
| PowerCLI typed return objects, PSRP/WinRM/SSH, pools, and transports | Unsupported | Retain the existing SMA/process paths. No CLR type, transport, or live session crosses the facade. |

This is not a drop-in replacement for `Devolutions.PowerShell.SDK`. Existing
applications that expose SMA parser AST/token types, `Collection<PSObject>`,
live runspaces, typed `PSObject` base objects, or interactive/remoting APIs
must retain an SMA-backed path or migrate to application-owned copied DTOs from
`PowerShellInvocationResult` snapshots. This facade never becomes an SMA
forwarding assembly or a generic proxy bridge.

### Copied session variables

`PowerShellSession.SetVariable(string, PowerShellValue)`,
`TryGetVariable(string, out PowerShellValue?)`, and `RemoveVariable(string)`
are the migration path for declarative `Result`, `Core`, `connection`, and
option snapshots. Variable names are ASCII identifiers up to 64 characters.
Values are copied on every set/get, capped at 64 KiB, at most 64 entries per
array/property bag, and at most eight nested levels. Cycles and unsupported CLR
objects are rejected before the native call.

The operation is rejected with `Backpressure` while that session has a pending
or running async operation. Synchronous operations and variable operations use
the process-global operation lock, so they cannot race a runspace mutation.
`TryGetVariable` cannot return a `PSVariable`, adapted object, methods, or any
value that cannot be encoded as the documented copied graph; such values return
`UnsupportedValue` rather than being stringified.

### Value-only projection readers

`PowerShellValue` is an immutable copied DTO, not an opaque serialized
`PSObject`. Its `TryGetString`, numeric, Boolean, date/time, GUID, URI, and
byte-array readers, together with `GetArray`, `GetPropertyBag`, and
`TryGetProperty`, let an application adapter map data into its own DTOs without parsing
display text or snapshot JSON. Arrays and property bags always return copied,
immutable collections; bytes are cloned on every read.

### Script parameter metadata

`PowerShellRuntime.ParseScriptParameters(script)` supports application script-editor
metadata without a facade reference to `System.Management.Automation.Language`.
The supplied script is an argument to a fixed payload-local
`Parser.ParseInput` helper and is never executed. On success the immutable DTO
contains each parameter's name, declared type spelling, default-expression
spelling, mandatory flag, description/help text, `ValidateSet` entries, aliases,
parameter-set name/position/pipeline flags, and copied
`ValidatePattern`/`ValidateRange`/`ValidateLength`/`ValidateCount` argument
spelling. On syntax failure it returns copied message, error-ID, and
source-offset DTOs instead of parameters. The API rejects scripts above 64 KiB
and fails rather than truncating more than 16 parameters, 16 total
`ValidateSet` values, eight aliases/parameter sets/validations per parameter,
four validation arguments, 32 combined metadata records, or 16 parse errors.
It deliberately does not return tokens, AST nodes, evaluated attribute
expressions, or arbitrary parser objects.

Output projection is deliberately narrower than input values. A
`PowerShellObjectSnapshot` can contain either one scalar or a property bag of
up to 16 scalar properties. Callers must check `IsPropertyBagTruncated` and
`DroppedPropertyEntryCount` before treating a projected bag as complete. For
example, a value-only replacement for reading a custom resolver result is:

```csharp
PowerShellObjectSnapshot record = result.Output.Records.Single();
if (record.IsPropertyBagTruncated ||
    record.PropertyBag is not { } properties ||
    !properties.TryGetProperty("Name", out PowerShellValue? name) ||
    !name!.TryGetString(out string? connectionName))
{
    throw new InvalidOperationException("The resolver result was not a complete DTO projection.");
}
```

This is an application-owned projection contract. It is not a substitute for
`PSObject.BaseObject`, adapted members, methods, PowerCLI CLR types, or generic
typed invocation.

`PowerShellSnapshotReader.GetCompleteProperties` makes the completeness check
mandatory before returning a property bag, while
`PowerShellSnapshotReader.CreateDisplaySnapshot` builds an immutable copied
text view of every output/stream channel and reports whether it is complete.
Neither helper recreates an SMA object, parses CLIXML as a CLR graph, or treats
display text as a typed DTO.

For an exactly-one generated DTO result, use
`PowerShellCompleteResultProjection.Read` with the generated mapper explicitly:

```csharp
ConnectionDto connection = PowerShellCompleteResultProjection.Read(
    completedResult,
    ConnectionDtoPowerShellDtoProjection.Read);
```

The helper also accepts the complete ordered `PowerShellValuePage` sequence
from typed or observed invocation paging. It fails closed with distinct
`ZeroResults`, `MultipleResults`, `IncompleteOrTruncated`, and `MapperFailure`
reasons. It only invokes the provided generated mapper; it does not execute
PowerShell, discover DTOs by reflection, or transfer arbitrary CLR/`PSObject`
data.

### Replacing injected application data

Value-only scripts can map application-owned input DTOs into explicit
copied variables: for example, `FfiConnection`, `FfiCoreOptions`, and
`FfiResult`. The caller sets input bags before invoking a session builder. A
script may replace `FfiResult` with a scalar-only `PSCustomObject` or
hashtable, and the caller retrieves its bounded property-bag snapshot with
`TryGetVariable` or `TryGetPropertyBag` after the invocation completes.
`SetPropertyBag` and `InvokeAndReadVariable` are convenience APIs for this
same copied-only flow. The package NativeAOT contract covers this update/readback
flow.

The old object's methods do not migrate this way. A reviewed operation that
needs parent-side behavior must instead be a named `app.*` capability with
explicit arguments and a copied response. A script that needs a local resource
may create it inside an opaque `PowerShellSession` and use it only through
approved script or module commands in that same serialized session; no
`PSSession`, `Runspace`, PowerCLI object, or resource handle is returned to the
parent.

### Local module roots

Session configuration accepts module names and absolute existing module roots.
Imports are resolved only beneath those roots, and the payload-local
authorization manager permits external scripts only from those locations. This
is not a module sandbox or a PowerCLI claim. Do not enable PowerCLI, arbitrary
module initialization, native loading, or remoting modules until their runtime
behavior has passed a separate application-owned validation.

`PowerShellRuntime.ValidateSessionConfiguration` applies the same root and
import resolution before session creation without creating a runspace, importing
or loading a module, or executing PowerShell/module code. It returns immutable,
bounded copied diagnostics for missing/invalid roots, unresolvable imports, and
invalid/unreadable manifests. Module-loading declarations must be static and
path-like declarations must resolve beneath their approved root after resolving
reparse-point ancestors; a dynamic declaration or a junction/symlink escape is
rejected. Static manifest declarations may expose a bounded module version and
command list; they are parsed as data only and do not authorize or execute a
module.

### Declarative recipes and result schemas

`PowerShellCommandRecipe` describes one bounded command name plus copied
parameters; `PowerShellScriptRecipe` describes bounded source text. Both accept
an optional timeout and `PowerShellResultSchema`. `PowerShellRuntime.Invoke`
and `InvokeAsync` apply the timeout using the normal cancellation contract; a
timed-out recipe has no successful partial result.

The schema verifies output record bounds, an optional allowed copied scalar
kind set, required copied property names, error policy, and snapshot
completeness. It rejects a missing, wrong-kind, or truncated result rather than
silently treating display strings as data. This is intended for migration
adapters that already own the expected DTO contract, not as a general result
deserializer.

`PowerShellCommandPolicy` is an opt-in application guardrail for recipe command
allowlists, parameter counts, script opt-in, and source size. It is explicitly
not a PowerShell sandbox: once arbitrary script source is allowed, that script
retains the full authority of its payload session. Applications must keep their real authorization and capability decisions outside
this advisory policy.

### Payload-owned module adapter contract

A future adapter for a specific reviewed module must be application-owned. Its
public contract may accept and return only documented copied `PowerShellValue`
DTOs and may use only explicitly registered capability names. It must prove its
target payload and module behavior through a NativeAOT package smoke before
being advertised.

This is not a generic module bridge. It cannot expose PowerCLI CLR objects,
remoting/PSRP, credentials, live application objects, arbitrary callbacks, or an
ambient `PSModulePath`. Any such behavior requires a separately designed
boundary rather than an exception to this contract.

### Explicit secret lease boundary

`PowerShellSecret` is a bounded, disposable UTF-16 lease. It can only be bound
as a payload-local `SecureString`; `PowerShellCredential` pairs that lease with
a copied user name to create a payload-local `PSCredential`. These are private
parameter encodings, not `PowerShellValue` kinds: ordinary arguments, input,
session variables, property bags, snapshots, pagers, serializers, bridge
contracts, and broker frames continue to reject secret material.

A secret-bound builder rejects normal invoke, async/live invocation, typed
pages, result snapshots, and copied diagnostics. It must call
`InvokeWithSecretBindings()` or `InvokeSecretResult(...)`. The latter accepts
only an explicit result kind: no output, one `SecureString`, or one
`PSCredential`; it returns a redacted disposable lease and generic failure text.
The payload clears its secure-string copies when the builder is cleared,
released, or finishes the dedicated invocation. The public lease zeroes its
managed character buffer on disposal.

`PowerShellSecret.Length`, `CopyTo(Span<char>)`, and `ToSecureString()` are
the only public transfer operations. `CopyTo` requires an exactly sized,
caller-owned buffer which the caller must clear. `ToSecureString()` returns a
separately owned, read-only `SecureString` which the caller must dispose.
Neither operation creates a plaintext string getter, and `ToString()` is
always redacted.

`InvokeCredentialResult()` is the separate credential-resolver sink. For one
invocation only, the payload injects `$Result` with setter-only `Username`,
`Domain`, `Password`, `SecurePassword`, `OutputMessages`, `ErrorMessages`,
`LogMessage`, and `Cancel` properties. `Password` accepts a string only long
enough to copy it into an internal `SecureString`; `SecurePassword` accepts a
`SecureString` only. Text and messages have fixed bounds, and the returned
`PowerShellCredentialResult` contains only copied username/domain/log text,
bounded output/error messages, cancellation state, and an optional disposable
redacted `PowerShellSecret` password. Normal pipeline output, error records,
or unsupported setter shapes reject the invocation rather than becoming a
generic result.

Sessions created from `InitialSessionState.CreateDefault2()` explicitly add
only `Microsoft.PowerShell.Commands.ConvertToSecureStringCommand` for this
sink, so `$Result.SecurePassword = ConvertTo-SecureString ... -AsPlainText
-Force` is supported without importing a module. No other module or
application-specific command surface is added.

This limits accidental FFI, logging, and DTO disclosure. It does **not** make
an arbitrary script, module, or remoting endpoint trustworthy: any code that
receives a credential can deliberately emit or retain it. General strings are
never a supported secret path, and consumers must not place secrets in script
text, `PowerShellValue`, or any copied result path.

For persistent local sessions, use
`PowerShellSessionConfiguration.CreateWithModuleImports(...)` with
`PowerShellModuleImport` declarations. Name and path declarations are bounded;
explicit paths must resolve below an approved root, while named imports are
resolved from the payload's approved `PSModulePath`. The package smoke verifies
the local secret and module cases. Set `PWSH_FFI_REMOTE_COMPUTER` in a controlled
environment to opt into its PSSession adapter proof; remoting is otherwise not
an installation or test prerequisite.

### Bounded capability RPC and host interaction

Capability RPC is feature-gated, disabled by default, and deliberately narrow.
`PowerShellRuntime.RegisterCapabilities` copies an immutable set of at most 16
definitions into Rust. Each definition has a canonical lowercase
namespace-qualified name, exact argument arity and value-kind schemas, allowed response
kinds, permissions, input/output byte caps, and a deadline. `WithCapabilities`
attaches one registration to one builder invocation. The payload creates the
temporary `$DpsCapabilities` only for that invocation and removes it before the
result is returned.

```csharp
var definition = new PowerShellCapabilityDefinition(
    "app.get-label",
    Array.Empty<PowerShellCapabilityArgumentSchema>(),
    new[] { PowerShellValueKind.String },
    PowerShellCapabilityPermission.Read,
    maximumInputBytes: 64,
    maximumOutputBytes: 256,
    deadline: TimeSpan.FromSeconds(5));
using var capabilities = runtime.RegisterCapabilities(new[]
{
    new PowerShellCapabilityBinding(definition, connectionNameHandler),
});
using var command = session.CreatePowerShell()
    .AddScript("$DpsCapabilities.Invoke('app.get-label')")
    .WithCapabilities(capabilities);
PowerShellInvocationResult result = command.Invoke();
```

The cross-boundary path is tagged `PowerShellValue` data only:

```text
script -> $DpsCapabilities.Invoke -> payload-local bridge -> Rust
       -> static AOT-safe managed callback -> typed handler -> Rust -> script
```

The opaque registration ID maps only to the private immutable definition set.
The payload cannot receive a parent CLR object, `GCHandle`, raw function
pointer, delegate, member path, or arbitrary method name. Unknown names,
duplicates, schema/type mismatches, malformed/deep/oversized values, inactive
registrations, handler exceptions, and invalid response kinds fail rather than
falling back to PowerShell reflection or string serialization. Diagnostics
from a failed callback are intentionally generic.

Dispatch is synchronous from the payload's point of view. Its deadline is
capped by the registered definition; cancellation, stopping the operation, or
unregistering a registration cancels the managed handler cooperatively and
rejects a late response. Rust rejects any FFI call attempted from the same
capability callback, preserving global operation serialization and avoiding a
callback-to-session deadlock. Registrations use `SafeHandle` ownership, and
disposal revokes new dispatches while cancellation is requested for active
ones.

`PowerShellHostInteraction` is a library of typed capability definitions:
`host.write-text`, `host.report-progress`, `host.read-line`,
`host.prompt-choice`, and `host.prompt-multiple-choice`. Handlers choose the
actual UI policy. They accept only bounded text/property-bag/choice DTOs and
never carry a credential or secure-string response. This reuses the capability
mechanism; there is no second callback vtable, direct console I/O, or `PSHost`
surface.

Applications define their own capability schemas; no application-specific
catalog is part of this SDK. For
`host.report-progress`, handlers can use
`PowerShellHostInteraction.ParseProgressUpdate` to validate explicit copied
`ActivityId`, `ParentActivityId`, activity/status text, percentage, remaining
seconds, and completion fields. It intentionally never derives typed progress
from a generic progress-stream display string.

This remains intentionally unlike a generic application-object bridge. Put
copied input and result data in session variables. Promote only a reviewed
operation, such as the example `app.get-label`, to an enumerated capability.
Scripts cannot call arbitrary proxy methods or obtain original managed objects.

`PowerShellStagedIntentCoordinator` is a generic lifecycle layer over this
same capability registration. It does not add a callback vtable, native
callback path, reentrancy, or application-object bridge. A
`PowerShellStagedIntentDefinition` declares a canonical operation name, copied
property-bag schema, handler, and retained stage deadline. It registers the
four capability names `<operation>.stage`, `.validate`, `.commit`, and
`.abort`; up to four definitions fit in the existing 16-capability bound.

`stage` accepts exactly `{ stageId, intent }`, where `stageId` is a bounded
opaque identifier supplied by the caller and `intent` is the schema-validated
copied property bag. The remaining operations accept only the stage identifier.
Each result is a copied property bag with `operation`, `status`, `stageId`,
`expiresAt`, and `message`. The explicit status values distinguish `staged`,
`validated`, `committed`, `aborted`, `rejected`, `unknown-stage`, `expired`,
`terminal`, `cancelled`, and `busy`. Envelope/schema/bounds checks occur before
the application handler is called. The coordinator rejects duplicate, unknown,
expired, and terminal stages; retains at most 64 active stages; and removes
active stage data on deadline, cancellation, and disposal.

Every retained stage has at most one terminal transition: commit or abort.
Expiry, cancellation, and disposal abort the coordinator's retained copied
state, then best-effort deliver `Abort` to the application handler so it can
release its own retained data. The notification runs outside the coordinator
lock, ignores handler failures, does not provide rollback, and requires an
idempotent handler.

The NativeAOT sample uses `rdm.connection-patch` with copied `ConnectionId`
and `DisplayName` properties, then demonstrates stage/validate/commit and a
separate stage/abort flow. This remains a narrowly reviewed operation, not an
injected `$RDM` object: scripts cannot discover other members, obtain the
original connection, or invoke another application operation.

`committed` has deliberately narrow meaning: the host accepted the intent. It
is not a cross-resource atomic transaction, supplies no rollback guarantee,
and does not prove that a handler's persistence or side effect completed.
Authorization, review UI, persistence, actual effects, compensating actions,
and any expiry cleanup outside the coordinator's copied state remain the
application's responsibility. Stage only copied non-secret `PowerShellValue`
data; never use staged intents for credentials or secret material.

### One-shot administrative command example

A local, no-credential `Stop-Computer` or `Restart-Computer` command is a
bounded one-shot example. Use `-WhatIf` in development and CI; real reboot or
shutdown execution must require an application-owned feature flag and explicit
operator approval.

```csharp
using var cancellation = new CancellationTokenSource();
cancellation.CancelAfter(TimeSpan.FromSeconds(15));
using PowerShell command = runtime.Create()
    .AddCommand("Restart-Computer")
    .AddParameter("ComputerName", computerName)
    .AddParameter("WhatIf");

try
{
    PowerShellInvocationResult result = await command.InvokeAsync(cancellation.Token);
    foreach (PowerShellInvocationError error in result.Errors.Records)
    {
        Console.Error.WriteLine(error.Message);
    }
}
catch (PowerShellInvocationException exception)
{
    foreach (PowerShellInvocationError error in exception.InvocationResult.Errors.Records)
    {
        Console.Error.WriteLine(error.Message);
    }
    throw;
}
```

The caller owns activation, cancellation, and disposal; it contains no
`System.Management.Automation` reference. A destructive path is not part of
this repository's normal tests.

## Duplex Broker Channel (DBC)

The Duplex Broker Channel is an **opt-in, strictly dispatch-only** request/reply
and one-way-event primitive. It exists so a PowerShell pipeline can ask the
consuming application for work **without executing application code on the
pipeline thread**.

DBC does not replace, change, or deprecate `PowerShellCapabilitySet`. Capability
RPC keeps its existing direct-callback semantics unchanged; DBC is a separate
facility with its own handles, exports, statuses, and feature bits. A build that
never opens a channel behaves exactly as before.

### Why a pump instead of a callback

Capability RPC calls the consumer's registered function pointer **on the
PowerShell pipeline thread**, and Rust therefore rejects every FFI call made
from that callback. DBC inverts the direction: the payload parks on Rust-owned
state, and the consumer pulls frames from an ordinary thread of its own.

```text
capability RPC:  script -> payload bridge -> Rust -> consumer callback   (pipeline thread blocked in consumer code)
DBC:             script -> payload bridge -> Rust queue                  (pipeline thread blocked in Rust only)
                                              ^
                             consumer pump ---+ receive, then reply later by correlation ID
```

A pump alone is **not** a liveness proof. DBC's liveness comes from the
mandatory preconditions and rules in *Dispatch-only liveness* below.

### Both protocol halves

DBC is fully specified in both directions. Neither half may be implemented
alone.

#### Payload half — payload calls into Rust

Rust hands the payload two trampolines through the single new binding-table
slot `PowerShell_SetBrokerContext`, mirroring the proven
`PowerShell_SetCapabilityContext` pattern. The payload never links a native
symbol and never retains a Rust pointer beyond the invocation.

```c
/* request/reply: blocks the calling payload thread on Rust-owned state */
int broker_enqueue_and_wait(
        uint64_t channel, uint64_t generation,
        uint32_t kind, uint32_t flags, uint64_t ordering_key, uint32_t deadline_ms,
        const uint8_t* body, uint32_t body_len,
        uint64_t* correlation_id,                      /* out, for diagnostics */
        uint8_t* reply, uint32_t reply_capacity, uint32_t* reply_len,
        multi_pwsh_call_result*);

/* one-way event: never blocks, never applies backpressure to a pipeline */
int broker_post(
        uint64_t channel, uint64_t generation,
        uint32_t kind, uint64_t ordering_key,
        const uint8_t* body, uint32_t body_len,
        multi_pwsh_call_result*);
```

`ordering_key`, `flags`, and `deadline_ms` are **explicit trampoline
parameters**. Rust cannot and does not infer them from an opaque body, and
there is no kind-registration table. `deadline_ms == 0` means "use the
channel's `default_deadline_ms`"; any other value is clamped to it.

There is deliberately **no cancel-polling trampoline**. Cancellation is a
terminal transition that wakes the blocked `broker_enqueue_and_wait` directly
and returns `MULTI_PWSH_OPERATION_CANCELLED`, so a separate poll would be
redundant.

`PowerShell_SetBrokerContext(pipeline, channel, generation, enqueueFn, postFn, CallResult*)`
attaches one channel to exactly one invocation and is cleared afterwards, like
the capability context. Passing all-zero arguments clears it; a partially zero
argument set is invalid.

For that one invocation the payload installs a fixed payload-local
`$DpsBroker` object with exactly two members and no others:

```powershell
[byte[]] $DpsBroker.Request([uint32] $kind, [byte[]] $body)   # request/reply
           $DpsBroker.Post([uint32] $kind, [byte[]] $body)    # one-way event
```

Any pre-existing `$DpsBroker` variable is saved and restored, matching
`$DpsCapabilities` behaviour.

**Generation is required, not redundant.** Clearing the pipeline's broker
context does not revoke a `$DpsBroker` object that script code captured into a
longer-lived variable. Every trampoline call therefore validates
`(channel, generation)` against Rust's active-invocation registry, exactly as
capability RPC validates `(registrationHandle, invocationId)`. Generations are
process-monotonic `uint64` values, never reused, allocated when a broker
context is attached and invalidated when that invocation reaches a terminal
state. A call with a stale generation fails with `MULTI_PWSH_INVALID_HANDLE`
and never reaches a queue.

#### Consumer half — NativeAOT calls into Rust

Fourteen additive `multi_pwsh_broker_*` exports. The public native ABI stays **v2**;
availability is advertised through a new public feature bit so a consumer can
reject an older native asset before calling an additive export.

```c
int multi_pwsh_broker_open (const multi_pwsh_broker_channel_options*, uint64_t* channel, multi_pwsh_call_result*);
int multi_pwsh_broker_close(uint64_t channel, multi_pwsh_call_result*);

/* consumer pump; returns *frame == 0 with MULTI_PWSH_SUCCESS on timeout */
int multi_pwsh_broker_wait (uint64_t channel, uint32_t timeout_ms, uint64_t* frame, multi_pwsh_call_result*);

int multi_pwsh_broker_frame_get_info(uint64_t frame, multi_pwsh_broker_frame_info*, multi_pwsh_call_result*);
int multi_pwsh_broker_frame_read    (uint64_t frame, uint8_t* buffer, uint32_t capacity, uint32_t* required, multi_pwsh_call_result*);
int multi_pwsh_broker_frame_release (uint64_t frame, multi_pwsh_call_result*);

/* worker-safe terminal observation; never holds a delivery handle */
int multi_pwsh_broker_observe(uint64_t channel, uint64_t correlation, uint64_t* observation, multi_pwsh_call_result*);
int multi_pwsh_broker_observation_get_info(uint64_t observation, multi_pwsh_broker_terminal_info*, multi_pwsh_call_result*);
int multi_pwsh_broker_observation_wait(uint64_t observation, uint32_t timeout_ms, multi_pwsh_broker_terminal_info*, multi_pwsh_call_result*);
int multi_pwsh_broker_observation_release(uint64_t observation, multi_pwsh_call_result*);

int multi_pwsh_broker_reply      (uint64_t channel, uint64_t correlation, const uint8_t* body, uint32_t len, multi_pwsh_call_result*);
int multi_pwsh_broker_reply_error(uint64_t channel, uint64_t correlation, int32_t code, multi_pwsh_utf8_span message, multi_pwsh_call_result*);
int multi_pwsh_broker_cancel     (uint64_t channel, uint64_t correlation, multi_pwsh_call_result*);

/* attach one channel to one builder invocation, mirroring multi_pwsh_set_capabilities */
int multi_pwsh_set_broker(uint64_t builder, uint64_t channel, multi_pwsh_call_result*);
```

### Delivery handle and correlation are different lifetimes

This is the single most important ownership rule.

`multi_pwsh_broker_frame_release` releases **only the readable delivery
handle**. It does **not** abandon the request, does not make the frame
terminal, and does not send an automatic reply. The correlation remains
outstanding until `reply`, `reply_error`, `cancel`, its deadline, or channel
close.

That separation is precisely what lets a pump copy a frame, release the
handle, return to `wait`, and have its dispatcher reply much later. Abandonment
is never implicit: a consumer that decides not to service a request must say so
with `multi_pwsh_broker_reply_error`, and a consumer that simply disappears is
bounded by the deadline.

| Object | Owner | Rules |
| --- | --- | --- |
| Channel handle | Consumer | Monotonic `uint64`, never reused by the process. Released by `multi_pwsh_broker_close`. |
| Delivery (frame) handle | The **pump thread that received it** | Returned by `multi_pwsh_broker_wait`. Read and released on that same thread. Releasing from another thread fails with `MULTI_PWSH_BROKER_DISPATCH_VIOLATION`. Releasing is not abandonment. |
| Frame body | Rust | Copied out by `multi_pwsh_broker_frame_read`. Rust never retains a consumer buffer and the consumer never receives a Rust pointer. |
| Correlation ID | Channel | Channel-scoped, monotonic, never reused. Valid for reply/cancel from **any** thread until the frame is terminal. |
| Terminal observation | Worker that acquired it | Bounded explicit lease over copied state only. It is not a delivery handle and may be queried or waited from any thread. |

Handle non-reuse is by monotonic allocation, matching every other handle table
in this ABI: a released or stale handle is simply absent and deterministically
returns `MULTI_PWSH_INVALID_HANDLE`.

Ownership of a delivery handle is tracked by a **process-unique owner token**
allocated per `wait` and stored both in the frame record and in the receiving
thread's thread-local state. A token, not a thread identifier, is used because
operating-system thread IDs can be reused. Consequences:

- Release requires a matching token, so one thread can never release another
  thread's delivery handle.
- If the owning thread exits without releasing, the delivery handle is
  reclaimed at channel close. The **request itself is unaffected** and still
  completes through its deadline, because release is not abandonment.
- Channel close and deadline expiry never block on a held delivery handle.

### Canonical wire structures

Every exported metadata field is fixed width. There is **no `usize`, no raw
pointer, and no platform-dependent field** in exported broker metadata, so the
layout is identical on 32-bit and 64-bit targets. All multi-byte integers are
**little-endian**. Both structures are size- and version-checked on entry; a
mismatched `size` or `abi_version` fails with `MULTI_PWSH_INVALID_ARGUMENT`
before any other field is read.

```c
#define MULTI_PWSH_BROKER_ABI_V1 1u

typedef struct {                 /* 24 bytes: 0,4,8,12,16,20 */
    uint32_t size;               /* = sizeof(struct); validated first          */
    uint32_t abi_version;        /* = MULTI_PWSH_BROKER_ABI_V1                 */
    uint32_t max_inflight;       /* 1..32,     default 32                      */
    uint32_t max_body_bytes;     /* 1..65536,  default 65536                   */
    uint32_t default_deadline_ms;/* 1..30000,  default 30000                   */
    uint32_t flags;              /* reserved, must be 0                        */
} multi_pwsh_broker_channel_options;

typedef struct {                 /* 56 bytes: 0,4,8,16,24,32,36,40,44,48,52 */
    uint32_t size;               /* = sizeof(struct)                           */
    uint32_t abi_version;        /* = MULTI_PWSH_BROKER_ABI_V1                 */
    uint64_t correlation_id;     /* channel-scoped, monotonic, never reused    */
    uint64_t ordering_key;       /* one active mutating frame per key          */
    uint64_t deadline_epoch_ms;  /* absolute, channel-owned monotonic epoch    */
    uint32_t remaining_ms;       /* computed at this call; 0 means expired     */
    uint32_t kind;               /* application-defined frame kind             */
    uint32_t flags;              /* see table below                            */
    uint32_t body_length;        /* 0..max_body_bytes                          */
    uint32_t state;              /* observational frame state, see table       */
    uint32_t dropped_before;     /* one-way frames coalesced before this one   */
} multi_pwsh_broker_frame_info;

typedef struct {                 /* 24 bytes: 0,4,8,12,16 */
    uint32_t size;               /* = sizeof(struct); validated first          */
    uint32_t abi_version;        /* = MULTI_PWSH_BROKER_ABI_V1                 */
    uint32_t state;              /* Queued through Aborted, see table           */
    int32_t terminal_status;     /* OPERATION_NOT_TERMINAL until terminal       */
    uint64_t terminal_epoch_ms;  /* channel epoch; zero while non-terminal      */
} multi_pwsh_broker_terminal_info;
```

| Flag | Value | Meaning |
| --- | --- | --- |
| `ONE_WAY` | `0x1` | Event frame. No reply is expected or accepted. |
| `MUTATING` | `0x2` | Participates in one-active-frame-per-ordering-key. |

| `state` | Value | Terminal |
| --- | --- | --- |
| `Queued` | `0` | no |
| `Dispatched` | `1` | no |
| `Completed` | `2` | yes |
| `Failed` | `3` | yes |
| `Cancelled` | `4` | yes |
| `TimedOut` | `5` | yes |
| `Aborted` | `6` | yes |

Bounds are hard failures, never silent truncation. A request body above
`max_body_bytes`, or a reply/error message above it, fails with
`MULTI_PWSH_INVALID_ARGUMENT` before it is queued or applied; it is never
truncated and delivered. A null body pointer is valid only with
`body_len == 0`. `reply_error` messages are additionally capped at 512 UTF-8
bytes and rejected, not truncated, above that.

`max_inflight` counts **every non-terminal correlation on the channel**, both
`Queued` and `Dispatched`, so releasing a delivery handle does not free a slot
and the bound is a real concurrency and memory limit.

`multi_pwsh_broker_wait` permits multiple simultaneous waiters on one channel.
Each queued frame is delivered to exactly one waiter.

### Terminal observation

`multi_pwsh_broker_observe` acquires a read-only, worker-safe lease for one
non-one-way correlation before or after its delivery handle is released. It
does not transfer a `CancellationToken`, payload callback, frame pointer, or
right to reply. `get_info` and `wait` copy the current `state`,
`terminal_status`, and channel-relative terminal epoch; waiting touches only
the completion cell and never calls PowerShell or acquires session-operation
locks.

The terminal transition is first-wins. `Queued` and `Dispatched` report
`MULTI_PWSH_OPERATION_NOT_TERMINAL` and epoch zero. `Completed`, `Failed`,
`Cancelled`, `TimedOut`, and `Aborted` report their defined terminal status.
Channel close publishes `Aborted` into every previously acquired observation
and leaves it readable until `observation_release`. Unknown correlations,
one-way frames, observations on another channel, and released handles fail
closed. Native retention is bounded to four observation leases per correlation
and `max_inflight * 4` leases per channel; exhaustion is
`MULTI_PWSH_BACKPRESSURE`.

### Deadline epoch

Each channel owns **one** monotonic epoch captured at
`multi_pwsh_broker_open`. Every deadline in that channel is a `uint64`
millisecond offset from that single epoch, so the payload, Rust, and the
consumer never disagree and no wall-clock change can move a deadline.

Deadlines are **absolute**, not a relative duration copied into the frame. A
frame that waited in the queue reports the time it actually has left in
`remaining_ms`; a relative value computed at enqueue would let a handler start
with a full budget moments before its raiser gives up.

### Frame state machine

```text
                      +--------------------- payload calls broker_enqueue_and_wait
                      v
   Queued ---wait()---> Dispatched ---reply--------------> Completed
     |                     |         ---reply_error------> Failed
     |                     |         ---cancel-----------> Cancelled
     |                     |         ---deadline---------> TimedOut
     |                     |         ---close------------> Aborted
     |---cancel----------> Cancelled
     |---deadline--------> TimedOut
     |---close-----------> Aborted
     |---queue full------> (never queued; payload gets BrokerBusy)

   frame_release affects only the delivery handle; it is NOT a state transition.
```

Exactly one terminal transition wins, applied atomically under the channel's
own lock. The blocked payload raiser is released exactly once, in every path
including consumer crash or abandonment. `Completed` is the only state that
yields a reply body.

`Completed`, `Failed`, `Cancelled`, `TimedOut` and `Aborted` are all terminal:
a later `reply`, `reply_error`, or `cancel` for that correlation returns
`MULTI_PWSH_BROKER_INVALID_TERMINAL_STATE` and **cannot** resurrect the frame,
affect another frame, or be delivered to a reused correlation ID.

Payload results per terminal state:

| Terminal state | `broker_enqueue_and_wait` returns |
| --- | --- |
| `Completed` | `MULTI_PWSH_SUCCESS` with the copied reply body |
| `Failed` | `MULTI_PWSH_MANAGED_FAILURE` with the consumer's bounded message as the call diagnostic |
| `Cancelled` | `MULTI_PWSH_OPERATION_CANCELLED` |
| `TimedOut` | `MULTI_PWSH_BROKER_TIMEOUT` |
| `Aborted` | `MULTI_PWSH_BROKER_CLOSED` |

A reply larger than the payload's `reply_capacity` fails that call with
`MULTI_PWSH_BUFFER_TOO_SMALL` and reports the required length; the frame stays
`Completed` and is not re-delivered.

### Exact statuses

New negative statuses continue the existing sequence
(`MULTI_PWSH_UNSUPPORTED_CAPABILITY` is `-17`):

| Status | Value | Raised when |
| --- | --- | --- |
| `MULTI_PWSH_BROKER_BUSY` | `-18` | The channel is at `max_inflight`, or an ordering key already has an active mutating frame. |
| `MULTI_PWSH_BROKER_NO_CONSUMER` | `-19` | No pump has ever called `wait` on the channel, so a request would hang. Distinct from a pump that is attached but saturated, which is `BROKER_BUSY`. |
| `MULTI_PWSH_BROKER_CLOSED` | `-20` | The channel is closing or closed. |
| `MULTI_PWSH_BROKER_INVALID_TERMINAL_STATE` | `-21` | Duplicate reply, late reply, reply to a one-way frame, or reply to an unknown correlation. |
| `MULTI_PWSH_BROKER_DISPATCH_VIOLATION` | `-22` | A non-broker FFI call was made while holding a delivery handle, or a delivery handle was released without its owner token. |
| `MULTI_PWSH_BROKER_TIMEOUT` | `-23` | The request passed its absolute deadline. |

"A pump has waited" is a sticky per-channel fact: once any thread has called
`multi_pwsh_broker_wait`, the channel is considered attached for its lifetime.
A pump that later exits does not retroactively turn requests into
`BROKER_NO_CONSUMER`; those requests fail on their deadline instead.

`multi_pwsh_broker_wait` reports "nothing arrived" as `MULTI_PWSH_SUCCESS` with
`*frame == 0` rather than a status, matching `multi_pwsh_operation_wait`'s
existing convention of reporting a non-terminal outcome on timeout.

Broker statuses are written **directly** into `multi_pwsh_call_result`. They
must never be routed through `FfiBindingError`/`managed_failure()`, which maps
unrecognised values to `MULTI_PWSH_MANAGED_FAILURE` and would erase them.

Every broker export writes its own bounded per-call diagnostic through the
existing `multi_pwsh_call_result`. There is no global last-error slot.

### Normative call-guard matrix

The existing guards are load-bearing and each broker entry point must use the
correct one. `v2_call` rejects whenever *any* pipeline is active anywhere;
`v2_call_allow_active_pipeline` rejects only when the *calling thread* is
inside a pipeline execution scope.

| Entry point | Guard | Reason |
| --- | --- | --- |
| `multi_pwsh_broker_open`, `multi_pwsh_set_broker` | `v2_call` | Called before invocation; must not race a running pipeline. |
| `broker_close`, `wait`, `frame_get_info`, `frame_read`, `frame_release`, `observe`, `observation_get_info`, `observation_wait`, `observation_release`, `reply`, `reply_error`, `cancel` | `v2_call_allow_active_pipeline` | The pump and worker run **while** a pipeline is active. Plain `v2_call` would return `MULTI_PWSH_BACKPRESSURE` and make the channel useless. |
| `broker_enqueue_and_wait`, `broker_post` | **neither** | These are payload trampolines called from a pipeline thread whose thread-local execution depth is nonzero, so even the permissive helper would reject them. They use `prepare_call_result` plus panic containment directly, exactly like `capability_dispatch`. |

Using different guards for open and wait is correct, not an inconsistency.

The delivery-handle check is evaluated **before** any test-only scope lock is
acquired, so a misusing call returns `MULTI_PWSH_BROKER_DISPATCH_VIOLATION`
deterministically instead of deadlocking a unit test.

### Lock ownership

Broker waiting must never hold a shared lock.

1. `broker_enqueue_and_wait` clones the channel `Arc` **under** the global
   `STATE` mutex, **drops `STATE`**, and only then waits on the channel's own
   `Mutex`/`Condvar`.
2. No broker data-plane operation ever acquires `SESSION_OPERATION_LOCK`. That
   lock is held across pipeline execution, so a reply that needed it would
   deadlock permanently.
3. No channel lock is held while reacquiring `STATE` or while calling managed
   code.

### Dispatch-only liveness

**This first DBC version has no synchronous nested invocation path at all.**
There is no wait-for graph, no causality token, and no reentrancy budget,
because none of those can be made safe before a complete resource model exists.

**Mandatory precondition: a builder with a broker attached may only be invoked
asynchronously.** `multi_pwsh_set_broker` marks the builder, and the
synchronous paths — `multi_pwsh_invoke` and the legacy `multi_pwsh_invoke_utf8`
— **reject** that builder with `MULTI_PWSH_UNSUPPORTED_CAPABILITY`. DBC is
carried only through the four asynchronous paths: async/live invocation, typed
result invocation, and observed invocation, in each case installed at start and
cleared on every completion, stop, release, and failed-start path.

This precondition is what makes the liveness claim true rather than
aspirational. Without it a UI thread could synchronously invoke a pipeline,
the pipeline could raise a request, and the pump could post its handler back to
that same blocked UI thread — a deadlock in which **no FFI call ever occurs**,
so no guard could fire. Forbidding synchronous invocation removes that shape
entirely, mechanically and at attach time.

The remaining rules are also mechanical, not advisory:

1. A pump handler **must** copy the frame, hand it to the application's own
   dispatcher, release the delivery handle, and return to
   `multi_pwsh_broker_wait`.
2. While a thread holds a delivery handle, **every non-broker FFI export called
   from that thread fails with `MULTI_PWSH_BROKER_DISPATCH_VIOLATION`.** This is
   enforced at the single choke point that 87 of the 88 exports already share;
   `multi_pwsh_get_abi_info` is exempt because it reads a static struct and
   takes no call-result. Consumers must not feature-probe while holding a
   delivery handle.
3. A handler therefore cannot start a pipeline, invoke a session, open a
   channel, or wait on work caused by its own frame. Direct recursion,
   cross-key A→B→A cycles, and pump saturation are impossible because no pump
   worker may block on broker-caused work.
4. Replies happen later, from any thread, by correlation ID. A handler learns
   that nobody is waiting any more because its `reply` returns
   `MULTI_PWSH_BROKER_INVALID_TERMINAL_STATE`; there is no separate consumer
   cancellation notification in this version.
5. **One active mutating frame per ordering key.** A second mutating frame for
   the same key is not dispatched while the first is non-terminal, so side
   effects for one key cannot be reordered or interleaved.
6. **One-way frames never block a producer.** When the channel is full, a
   one-way frame first evicts the oldest queued one-way frame of the same
   `kind`; if there is none, it evicts the oldest queued one-way frame of any
   kind; if there is still none, the post is dropped. Every eviction or drop
   increments a counter reported in the next delivered frame's
   `dropped_before`. `broker_post` returns `MULTI_PWSH_SUCCESS` in all three
   cases — an event must never apply backpressure to a pipeline.

Because the existing guard already rejects ordinary FFI calls while any
pipeline is active, an application dispatcher that tries to call back into
PowerShell during the invocation it is servicing still receives
`MULTI_PWSH_BACKPRESSURE`. DBC does not relax that rule.

### Compatibility and rejection

The payload binding table stays a **required, all-or-nothing V1 contract**.
DBC adds exactly one slot, `PowerShell_SetBrokerContext`, **appended after the
current final slot** so no existing slot offset moves. Appending is mandatory:
inserting the slot mid-table would let an older host load a larger table and
call through shifted, incorrectly typed pointers.

There are two distinct feature-bit contracts and both gain a DBC bit:

- the **payload table** required-feature mask, validated header-first before
  the table is read;
- the **public native ABI** feature word reported by `multi_pwsh_get_abi_info`.

Because 0.17 is unreleased, this is an intentional synchronized change to the
matched table rather than a compatibility shim. The consequences are explicit:

- A payload table that is older, undersized, missing the new required feature
  bit, or has any null slot is rejected by the existing header-first check
  **before any channel can open and before any invocation runs**.
- A newer Rust host cannot load an older payload table. That is the intended
  behaviour: the native asset and the managed payload bindings ship together.
- There is no optional-slot negotiation and none is planned.

The synchronized change set is larger than the slot itself, and every item is
required in the same commit:

| Location | Changes |
| --- | --- |
| `crates\pwsh-sdk-ffi\src\lib.rs` | broker statuses, public feature bit, channel/frame tables, ten exports, two trampolines, guard-matrix wiring |
| `crates\pwsh-host\src\bindings\ffi.rs` | required feature mask, raw table field, fn alias, null-slot list, typed `FfiBindings` field, transmute initializer, wrapper method |
| `dotnet\bindings\FfiBindings.cs` | payload feature constant, table field, feature mask, slot initializer, `UnmanagedCallersOnly` method, `$DpsBroker` bridge, all four async invocation paths |
| `dotnet\sdk-ffi\` | `NativeMethods.cs` imports, `PowerShellFfiStatus.cs`, facade feature check, `PowerShellBrokerChannel` and its DTOs |
| `tests\Verify-PwshFfiApiBaseline.ps1` | slot descriptor, size `688` → `696`, feature expression, Rust field order, alias fixture, status/struct expectations |
| `tests\PwshFfiApiBaselineInspector\Program.cs` | required-feature mask/loop |
| `tests\PwshFfiApiBaseline.txt` | new binding entry and new public facade surface |
| `dotnet\nativeaot-sample\` | end-to-end pump smoke |

### What DBC deliberately does not carry
The broker moves **bounded opaque byte frames and fixed-width metadata**. It is
not an object bridge. The following are excluded by construction, not by
convention:

- no dynamic member access, reflection, member discovery, or runtime contract
  negotiation;
- no JSON or any self-describing text wire format;
- no `PSObject`, `ErrorRecord`, SMA type, live runspace, or CLR object identity;
- no delegates, function pointers, `GCHandle`, or arbitrary callbacks from
  script;
- no credential, `SecureString`, or secret material — DBC frames are ordinary
  copied data and must never be used as a secret transport;
- no injection of a consumer bridge object into a **remote** runspace: a
  payload proxy is a local managed object and would be serialized, stripping
  its members;
- no relaxed package compatibility fallback.

Frame `kind` values and body encodings are application-owned static contract
data in this tranche. The generated contract layer that gives them meaning is
deliberately a later, separate change.

### DBC test coverage

| Layer | Coverage | Runs in |
| --- | --- | --- |
| Rust unit | 15 lifecycle/liveness tests plus a bounded randomized soak: request/reply, cancel after dispatch, timeout, close waking all waiters, no consumer, saturation, duplicate and late replies, stale and cross-channel handles, release races, cross-thread release, dispatch-violation rejection, one-way coalescing, and wire-structure layout | `cargo test`, every CI `test` job |
| NativeAOT sample | A real pipeline calls `$DpsBroker.Post` then `$DpsBroker.Request`; a non-UI pump dispatches and replies from another thread; synchronous invocation with a broker attached is rejected | `nativeaot-sdk` CI job, asserted by output line |
| Packed SDK consumer | The same flow through the **restored NuGet package**'s public facade and staged native asset, from an isolated local feed with no fallback folders | `tests/Test-PwshFfiPackage.ps1`, `nativeaot-sdk` CI job |

The randomized soak uses a fixed seed and a deterministic linear congruential
generator, so it is reproducible and bounded (24 iterations, well under a
minute) rather than opt-in. It asserts that every payload thread reaches a
defined terminal status, that no channel handle or delivery handle is ever
reused, and that closing a channel leaves no registry entries behind.

Both the sample and the packaged consumer assert an explicit success line, so
silently dropping the broker smoke fails CI rather than passing quietly.

## Bridge Contract v2

> **Current carrier:** generated Bridge Contract v2 attachment uses the
> DBC-only protocol in [Generated Bridge Contract v2 over
> DBC](bridge-contract-v2-dbc.md). That document is normative for attachment,
> routing, lifecycle, authorization, mutation, and error semantics. The
> historical COM-carrier design notes below remain implementation history only;
> they do not describe an attached bridge invocation.

Bridge Contract v2 is a **closed generated IDL** for a finite local application
object surface. It exists so a trusted payload pack can offer ordinary
PowerShell property and method syntax over an application object graph that the
application declared, member by member, at compile time.

It is a compiler, not a bridge policy. Everything it accepts is enumerated in
source; everything else is a compile-time error. There is no runtime member
discovery, no name-based dispatch, and no way to widen the surface without
recompiling both sides.

### Relationship to the v1 live-contract preview

`[LiveContract]`, `[LiveObject]`, and `[LiveMember]` — the v1 preview described
in *Live-contract generator preview* — are **unchanged**. v1 keeps its own
generator, its own diagnostics (`MPWLC001`-`MPWLC010`), its own version-1 wire
format, and its existing narrow root/collection/child graph. A package built
against v1 keeps its exact behaviour.

v2 is a **separate attribute family** compiled by a second generator in the same
analyzer assembly. v1's shape-matched three-object graph cannot be generalised
without changing its emitted surface, and keeping legacy packages working is a
delivery requirement, so the two families coexist rather than merge.

Coexistence rules, which the generators implement mechanically:

- The v2 generator produces nothing and reports nothing when a compilation
  declares no `[BridgeContract]` root, exactly as v1 does for `[LiveContract]`.
  Referencing the SDK never turns an ordinary consumer into diagnostics.
- A compilation that declares both a v1 root and a v2 root is an error
  (`MPWLC012`), reported by the v2 generator only, which then emits nothing.
- Both families share `AnalyzerReleases.Unshipped.md`; `MPWLC011`-`MPWLC024`
  are added there in the same change that adds them to the generator, because
  `EnforceExtendedAnalyzerRules` makes an unlisted rule an RS2008 build error.

### Declaration surface

A contract is declared once, in one source file, and compiled twice — once with
`LiveContractMode=Host` for the NativeAOT consumer and once with
`LiveContractMode=Payload` for the trusted payload pack.

```csharp
// The contract author declares the COM transport interface by hand.
// A source generator cannot see another generator's output, so the
// [GeneratedComInterface] declaration must exist in user source.
[GeneratedComInterface]
[Guid("2C7E8A11-6B44-4E27-9F0A-0C6C0F53D8E1")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public partial interface IRdmBridgeTransport
{
    [PreserveSig]
    int Invoke(ulong leaseId, uint generation, ulong objectId, uint memberId,
               nint input, int inputLength, nint output, int outputCapacity, out int outputLength);

    [PreserveSig]
    int CloseLease(ulong leaseId, uint generation);
}

[BridgeContract("6F1B2D2A-...", 1, 0, "2C7E8A11-6B44-4E27-9F0A-0C6C0F53D8E1")]
[BridgeObject(1, ReleaseId = 900)]
public interface IRdmBridge
{
    [BridgeMember(1, Permission = BridgePermission.Read, MaximumUtf8Bytes = 256)]
    string ProductVersion { get; }

    [BridgeMember(2, Permission = BridgePermission.Execute, ResultObjectId = 2)]
    IRdmConnection? GetConnection([BridgeBound(MaximumUtf8Bytes = 128)] string id);

    [BridgeEvent(500, OrderingKey = 1)]
    void OnScriptProgress(int percent);
}

[BridgeObject(2, ReleaseId = 901)]
public interface IRdmConnection
{
    [BridgeMember(10, SetterId = 11,
        Permission = BridgePermission.Read,
        SetterPermission = BridgePermission.Write,
        SetterMutation = BridgeMutation.Direct,
        MaximumUtf8Bytes = 256)]
    string Name { get; set; }

    [BridgeMember(12, MaximumCollectionCount = 64, MaximumUtf8Bytes = 128)]
    IReadOnlyList<string> Tags { get; }
}

[BridgeData(70)]
public interface IRdmFailure
{
    [BridgeField(1, MaximumUtf8Bytes = 256)] string Reason { get; }
}

[BridgeEnum(80)]
public enum RdmConnectionState { Closed = 0, Open = 1 }
```

| Attribute | Applies to | Carries |
| --- | --- | --- |
| `[BridgeContract]` | the root interface | contract ID, major, minor, **the IID of a user-declared COM transport interface** |
| `[BridgeObject]` | every object interface, root included | object type ID, explicit `ReleaseId` ordinal |
| `[BridgeMember]` | property or method | ordinal, `SetterId`, `Mutation`, `SetterMutation`, `Permission`, `SetterPermission`, `ResultObjectId`, `ErrorDataId`, and the bounds for the **result** position |
| `[BridgeBound]` | parameter or return value | the bounds and `ResultObjectId` for that **one** position |
| `[BridgeEvent]` | `void` method | one-way ordinal in `1..65535` and a static `OrderingKey` |
| `[BridgeData]` | DTO interface | data type ID |
| `[BridgeField]` | DTO property | field ordinal and that field's bounds |
| `[BridgeEnum]` | enum | enum type ID |

Bounds are **per position, never inherited**. A member-level cap applies only to
the property or method result; every bounded parameter declares its own
`[BridgeBound]`. A `[return: BridgeBound]` declaration **wins over** the
member-level cap for the result position. A setter's value position is the
property's own type and reuses the property's declared cap, because it is the
same position.

Declared member, parameter, and field names must not begin with `__bridge`. The
generator reserves that prefix for its own locals, and a contract that used it
would fail with a raw compiler error inside generated code instead of a
diagnostic. This is `MPWLC014`.

The accepted CLR spellings are closed:

| Declared CLR type | Tag | Required bound |
| --- | --- | --- |
| `bool`, `int`, `long`, `double`, `System.Guid` | `Bool`, `Int32`, `Int64`, `Double`, `Guid` | none |
| `string` | `Utf8String` | `MaximumUtf8Bytes` |
| `byte[]` | `Bytes` | `MaximumByteCount` |
| an enum declaring `[BridgeEnum]` | `Enum32` | none |
| an interface declaring `[BridgeObject]` | `Handle` | `ResultObjectId` |
| an interface declaring `[BridgeData]` | `Data` | none |
| `IReadOnlyList<T>` | `List` | `MaximumCollectionCount` |
| `T?` and `Nullable<T>` | the base tag, plus `Null` | the base tag's bound |

`byte[]` and `IReadOnlyList<T>` are the **only** array and generic spellings the
compiler accepts, plus `Nullable<T>` over a supported scalar. Every other array
and every other generic is rejected. `System.Guid`, `IReadOnlyList<T>`, and
`Nullable<T>` are matched by **symbol**, not by name: a namespace-leaf comparison
would accept `Acme.System.Guid` and emission would then silently substitute the
real type, so an allow-list validated by string matching is not closed.

A null in a non-nullable `byte[]` or `string` position is rejected rather than
encoded. Both fail closed; neither substitutes an empty value.

A reference-type position must be declared in an enabled nullable context. An
unannotated reference type is rejected, because inferring nullability from
project settings would make the descriptor depend on how each side is built and
break Host/Payload hash parity.

**Every contract owns a distinct COM IID.** The payload pack registry keys on
`InterfaceId` and rejects duplicates
(`dotnet\bindings\LiveObjectContractPackRegistry.cs`), and
`PowerShellLiveObject<TContract>` requires `contract.InterfaceId` to equal
`typeof(TContract).GUID`. A shared v2 interface would therefore let only one
contract exist per process and would collide with v1, whose fixture already uses
`IPowerShellLiveObjectBrokerContract`'s fixed IID. The generator verifies that
the compilation declares a `[GeneratedComInterface]` interface with the declared
IID and the exact `Invoke`/`CloseLease` shape above, and reports `MPWLC012`
otherwise. Major must be `1..65535` and minor `0..65535`, because the pack
descriptor stores both as `ushort`.

Ordinals are **globally unique within the contract** across getters, setters,
methods, events, and release ordinals. Nothing on the wire is a name.

`Permission` is metadata, never a decision. It is an **input** to the
application's authorizer, which is consulted independently for the getter, the
setter, and each method. A member declaring `BridgePermission.Read` cannot make
its setter callable, because the setter carries its own `SetterPermission` and
its own authorization call.

### Closed value system

All wire integers are **little-endian**, `Double` is IEEE-754 transported as its
little-endian `int64` bit pattern, and a `Guid` payload is exactly the 16 bytes
of `Guid.ToByteArray()`. Every value uses this 8-byte header followed by its
payload:

```text
0  u8   version   = 2
1  u8   tag
2  u8   flags     = 0
3  u8   reserved  = 0
4  u32  length    (payload bytes)
```

| Tag | Value | Payload | Declared bound |
| --- | --- | --- | --- |
| `Null` | 0 | none | — |
| `Bool` | 1 | 1 byte, `0` or `1` | — |
| `Int32` | 2 | 4 | — |
| `Int64` | 3 | 8 | — |
| `Double` | 4 | 8 IEEE-754 | — |
| `Utf8String` | 5 | strict UTF-8, no NUL | `MaximumUtf8Bytes` |
| `Bytes` | 6 | opaque octets | `MaximumByteCount` |
| `Guid` | 7 | 16 | — |
| `Enum32` | 8 | 4, must equal a declared member value | closed member set |
| `Handle` | 9 | `objectTypeId:u64`, `objectId:u64` | declared object type |
| `List` | 10 | `count:u32`, `elementTag:u8`, `reserved:u8[3]`, elements | `MaximumCollectionCount` |
| `Data` | 11 | `dataId:u64`, `fieldCount:u32`, `reserved:u32`, then each field as `ordinal:u32`, `reserved:u32`, value | declared field set |
| `Error` | 12 | `code:i32`, `reserved:u32`, one nested value | declared error data ID |

`Bytes` has its own bound because opaque octets have no UTF-8 semantics.

**Nesting is closed and non-recursive in its bounds.** Exactly these shapes are
legal, so every position that can hold a string, a byte payload, or a
collection declares its own single cap and no bound is ever inherited or
inferred:

| Position | May hold |
| --- | --- |
| member result, method parameter | any scalar tag, `Data`, or `List` |
| `List` element | any scalar tag or `Data` — never `List` |
| `Data` field | any scalar tag except `Handle`, or `List` of such scalars — never `Data`, never `List` of `Data` |
| `Error` payload | `Null` or one `Data` |

A `Data` field cannot carry a `Handle`. A data contract is a copied value and
must not depend on lease-scoped identity; excluding it also keeps the emitted
data class and its codec identical in `Host` and `Payload` mode.

The deepest legal value is therefore `Error > Data > List > scalar`, four
levels, which is the runtime depth cap. Every element of a `List` still carries
its own value header; `elementTag` states the single tag every element must
use, and a nullable element position accepts `Null` in addition to it.

`Data` fields are written in ascending ordinal order with no duplicates and
**every declared field present**; a closed contract has no optional field, so an
absent value is `Null`, never an omission.

A nullable declaration widens the accepted tag set for that position by exactly
`Null`. It is not a distinct encoding.

`Handle` carries its declared object type ID next to the runtime object ID, so a
handle of the wrong type is rejected structurally, before any lookup.

### Structural limits

| Limit | Value | Enforced |
| --- | --- | --- |
| Object types per contract | 64 | compile time |
| Object DAG depth from the root | 8 | compile time |
| Members per object | 64 | compile time |
| Method parameters | 8 | compile time |
| DTO fields | 32 | compile time |
| Enum members | 64 | compile time |
| Value nesting depth | 4 | compile time and runtime |
| `MaximumUtf8Bytes`, `MaximumByteCount` | 1..8192 | compile time and runtime |
| `MaximumCollectionCount` | 1..4096 | compile time and runtime |
| Frame bytes | 65536 | compile time budget and runtime |

The per-position caps are independent upper bounds; the **frame budget is the
binding constraint**. The generator computes the worst-case encoded request and
reply size of every member from its declared caps using saturating arithmetic
that stops at 65537, and fails the build (`MPWLC022`) if either exceeds 65536.
A declaration at both maxima — 4096 elements of 8192 bytes — is roughly 33 MB
and is rejected at compile time, so it never becomes a runtime bound check. At
runtime the codecs still enforce every declared cap and the frame header's own
length, so a mismatched or hostile peer fails loudly rather than truncating.

### Canonical descriptor and SHA-256

The generator emits a **canonical descriptor byte sequence** that fully
determines the contract, and the SHA-256 of those bytes.

Every integer is little-endian. Every name is `u32` byte length followed by
strict UTF-8 bytes with no NUL. Nothing is derived from
`ISymbol.ToDisplayString`, assembly identity, namespace, culture, nullable
project settings, or any dictionary or `GetMembers()` iteration order; every
collection below is emitted in the stated total order, and a tie is impossible
because the sort keys are the unique IDs the author declared.

```text
magic         u32  0x32574D42            (bytes 42 4D 57 32, "BMW2")
version       u32  2
contractId    name
major         u32
minor         u32
interfaceId   16 bytes, exactly Guid.ToByteArray()
rootObjectId  u64
enums       u32 count, ascending by enum ID:
              id u64, name, u32 member count, ascending by value: value i32, name
data        u32 count, ascending by data ID:
              id u64, name, u32 field count, ascending by ordinal: ordinal u32, name, type-ref
objects     u32 count, ascending by object type ID:
              id u64, name, releaseId u32, u32 record count, ascending by ordinal: member record
```

Names are `ISymbol.Name`, never a display string. A property expands into **one
or two independent member records** — a `Getter` record at its ordinal and, when
`SetterId` is non-zero, a `Setter` record at the setter ordinal. A method
expands into one `Method` record and an event into one `Event` record. The
object's `releaseId` is emitted in the object header and does not produce a
member record. Each record's fields are fully determined:

| Record | `name` | `mutation` | `permission` | `result` | `parameters` |
| --- | --- | --- | --- | --- | --- |
| `Getter` | the property name | always `None` | `Permission` | the property type | none |
| `Setter` | the property name | `SetterMutation` | `SetterPermission` | `Null` | one, named `value`, of the property type |
| `Method` | the method name, or `get_Item` for an indexer | `Mutation` | `Permission` | the return type, `Null` for `void` | in declaration order |
| `Event` | the method name | always `None` | always `Execute` | `Null` | in declaration order |

`orderingKey` is the declared value for an `Event` record and `0` everywhere
else. `errorDataId` is the declared `ErrorDataId`, and `0` means the member
declares no typed error reply.

```text
ordinal      u32
name         name
kind         u8   Getter=1, Setter=2, Method=3, Event=4
mutation     u8   None=0, Direct=1, Staged=2
permission   u8   None=0, Read=1, Write=2, Execute=3
flags        u8   bit 0: nullable result
errorDataId  u64
orderingKey  u64  (0 for every non-event record)
result       type-ref
parameters   u32 count, in declaration order: name, type-ref
```

A type-ref is recursive and finite:

```text
tag u8 (bit 7 set when the position is nullable)
  Utf8String / Bytes : maxBytes u32
  Enum32             : enumId u64
  Handle             : objectTypeId u64
  List               : maxCount u32, element type-ref
  Data / Error       : dataId u64
  everything else    : no trailing bytes
```

Names are included. Both sides compile the same declaration, so names always
agree; including them catches an ordinal swapped between two same-shaped
members, which a shape-only hash would silently accept.

The hash is emitted as a hex constant and as a `ReadOnlySpan<byte>`-returning
property over a `new byte[] { ... }` literal, so reading it allocates nothing
and touches no reflection. The generator itself runs on `netstandard2.0` and
therefore uses `SHA256.Create().ComputeHash`, not `SHA256.HashData`.

### Frames

One 32-byte request header serves every carrier.

```text
0   u8   version        = 2
1   u8   frameKind      Invoke=0, Release=1, Event=2, Open=3
2   u16  argumentCount
4   u32  memberId
8   u64  objectId
16  u64  leaseId
24  u32  generation
28  u32  bodyLength     (bytes after this header)
32  ...  argumentCount tagged values, back to back
```

The reply header is 8 bytes and carries exactly one tagged value:

```text
0   u8   version    = 2
1   u8   replyKind  Value=0, Error=1
2   u16  reserved   = 0
4   u32  bodyLength
8   ...  one tagged value (Null for void, Error for a typed failure)
```

`frameKind` is **not** redundant with the globally unique ordinal, and it is not
trusted on its own. Before any lease lookup the dispatcher requires exact
agreement between the carrier, `frameKind`, the descriptor's declared kind for
`memberId`, and `argumentCount`:

- `Invoke` is legal only over the COM transport and only for a `Getter`,
  `Setter`, or `Method` record;
- `Release` is legal only over the COM transport, only when `memberId` equals
  the `ReleaseId` of the object type that `objectId` resolves to, and only with
  `argumentCount == 0`;
- `Event` is legal only over the one-way event carrier and only for an `Event`
  record; it is never accepted on the request/reply transport;
- `Open` is legal only over the COM transport with
  `memberId == 0`, `objectId == 0`, `leaseId == 0`, `generation == 0`, and
  exactly one `Bytes` argument.

`replyKind` must be `Error` if and only if the nested tag is `Error`.

`(leaseId, generation, objectId, memberId)` is present in every request frame
even though the COM transport also passes it out of band. The dispatcher
compares the two and rejects a mismatch, so a request that agrees with itself is
the only one that reaches an application handler.

### Lease handshake

v1 bootstraps a lease with an all-zero `Invoke` whose reply is the UTF-8 string
`"leaseId:generation:hash"`, and the **payload** compares the hash. v2 keeps the
same bootstrap position but makes it typed and two-sided.

An `Open` frame carries the payload's own 32-byte descriptor hash as its single
`Bytes` argument. The consumer compares it to its own hash **before allocating
anything** and returns `PowerShellBridgeStatus.ContractMismatch` on any
difference. On agreement it replies with a `Bytes` value of exactly 52 bytes:

```text
0   u64  leaseId          (non-zero)
8   u32  generation       (non-zero)
12  u64  rootObjectId     (non-zero)
20  32   descriptorHash   (echoed)
```

The payload verifies the echoed hash as well, so neither side can accept a
mismatched peer. Both sides therefore reject a mismatched artifact before any
member call, which is what "lock-step" means operationally.

`Open` is **not** idempotent and is not a query. A consumer broker has exactly
one `Unopened -> Active -> Closed` progression:

- an `Open` while a lease is `Active` returns `E_ACCESSDENIED` and allocates
  nothing, so a replayed or concurrent `Open` cannot consume lease slots;
- an `Open` after closure allocates a **new** lease with a lease ID that the
  process never reuses, so a wrapper holding the previous lease stays revoked;
- the consumer validates `outputCapacity` against the full 68-byte `Open` reply
  — an 8-byte reply header plus an 8-byte value header plus the 52-byte
  payload — **before** it allocates a lease, so a buffer-too-small probe cannot
  consume a slot either.

The same consumer object may legitimately be assigned to more than one session
variable, and each assignment creates a payload proxy. Those proxies share one
lease and one `Close`; that is the intended model, and it is why closure is
first-caller-wins rather than reference-counted.

### Historical pre-DBC transport design (superseded)

> This section records the discarded COM-carrier design. Generated Bridge
> Contract v2 attachment is fully DBC-routed; see
> [Generated Bridge Contract v2 over DBC](bridge-contract-v2-dbc.md). The
> carrier/status statements below are not current behavior.

| Direction | Carrier | Status |
| --- | --- | --- |
| `Open`, `Invoke`, `Release` | the contract's own COM transport interface, through the existing consumer-to-session contract pack registry | wired |
| `Event` | `IPowerShellBridgeEventSink`, obtained by `QueryInterface` on the same `IUnknown` | wired, non-blocking by contract |
| `Event` | duplex broker channel `Post` | **not wired**; see below |

The generated payload wrapper talks to a seam rather than to a carrier:

```csharp
public interface IPowerShellBridgeTransport
{
    int Invoke(ulong leaseId, uint generation, ulong objectId, uint memberId,
               ReadOnlySpan<byte> request, Span<byte> reply, out int replyLength);
    void PostEvent(uint kind, ulong orderingKey, ReadOnlySpan<byte> body);
}
```

The interim event carrier has a real ABI, not a convention. It is a single
shared `[GeneratedComInterface]` declared by the SDK, so a pack implements one
known shape rather than a per-contract one:

```csharp
[GeneratedComInterface]
[Guid("9D4B2F87-1A63-4F0E-A5C4-6E0B1D5C7A32")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public partial interface IPowerShellBridgeEventSink
{
    [PreserveSig]
    int PostEvent(uint kind, ulong orderingKey, nint body, int bodyLength);
}
```

It carries the same 32-byte request frame with `frameKind = Event`. Any non-zero
HRESULT fails the call deterministically; a saturated consumer queue must return
a non-zero HRESULT rather than block, because it is servicing a pipeline thread.

**The duplex broker channel is not reachable from a payload pack today, and
saying otherwise would be false.** A pack is created only through
`NativeLiveObjectContractPackApi.CreatePayloadProxy(IntPtr comObject, IntPtr* proxyHandle)`
and receives nothing but a consumer-owned `IUnknown`. The only producer of DBC
frames is `FfiBrokerContext`/`FfiBrokerBridge`, which is private to
`FfiBindings` and is installed only as the payload-local `$DpsBroker` variable
for the duration of one invocation. A pack additionally compiles its **own
private copy** of the live-contract sources, so no plain managed interface can
be handed across by type identity.

Until that is changed, an event sink is obtained by `QueryInterface` for the
hand-authored `IPowerShellBridgeEventSink` on the same `IUnknown` the pack
already receives, and a consumer that supplies one **must** return from
`PostEvent` without blocking. A contract that declares an event and is leased
without a sink fails that call with a deterministic
`InvalidOperationException`; it never silently degrades, because an event that
blocks a pipeline is a different primitive with different liveness.

Making non-blocking delivery *structural* rather than contractual, and moving
request/reply off the pipeline thread, both require the same change: the payload
must be able to hand a pack a broker trampoline. The concrete shape follows.

#### Planned: the v2 broker binding

**Decided: v2 will have one carrier.** Every v2 frame travels over the duplex
broker channel, and the COM carrier is deleted for v2 — not left dormant behind
a flag. v1 keeps every COM mechanism unchanged, bit for bit.

**The sections below this one still describe the COM carrier.** They are
authoritative for what is implemented today and are rewritten when the carrier
moves.

A first design pass did not survive review. What follows is the second, with the
corrections folded in and each remaining open question named rather than
papered over.

##### Status travels in a reply envelope, not in `reply_error`

`multi_pwsh_broker_reply_error` cannot carry the bridge status: Rust converts a
reply error into diagnostic text and the payload observes only
`MULTI_PWSH_MANAGED_FAILURE`, so `InvalidArgument`, `AccessDenied`,
`ContractMismatch`, and `Bounds` would all collapse into one value and the
normative failure table would become unimplementable.

Every bridge reply is therefore a **successful** broker reply carrying an
8-byte envelope:

```text
0  u32  status     the bridge status; 0 means the reply frame follows
4  u32  reserved   = 0
8  ...  the reply frame, present only when status == 0
```

`reply_error` is reserved for pump and infrastructure failure — no dispatcher
for that contract, or a pump that is shutting down — which is exactly the case
where no bridge status exists to report.

##### Reply capacity becomes a compile-time constant, not a runtime discovery

The dispatcher checks reply capacity before dispatch so a handler cannot mutate
and then fail on an undersized buffer. Nothing in the broker path conveys a
caller capacity, so instead of discovering it the payload **allocates from the
member's compile-time maximum**, which the static member table already carries on
both sides.

That turns the check into a per-bind validation rather than a per-call one:
binding rejects the contract when

```text
channel MaximumBodyBytes  <  8 (envelope) + 8 (routing prefix) + member maximum request or reply
```

for any declared member. `PowerShell_SetBrokerContext` now conveys the channel's
configured bound, so this is checkable at the moment of binding and fails
deterministically there rather than at the first oversized call. A
buffer-too-small outcome then cannot arise at all between matched artifacts,
because both sides size from the same hash-verified member table.

This works only because bridge traffic goes through its own sink rather than
through `$DpsBroker`. `FfiBrokerContext.Request` allocates a reply buffer of the
full channel bound on every call and gives the caller no way to size it, so a
bridge riding that path would allocate up to 64 KiB to read a short string. The
sink sizes each reply buffer from the member maximum the pack declared, which is
both the correctness argument above and the reason an ordinary property read
costs a few hundred bytes rather than the channel's whole body.

##### Bind and unbind need no new frame kinds

A one-way broker frame may be coalesced away and carries no ordering guarantee,
so bind and unbind cannot be one-way frames. They do not need to be new frames
at all:

- **bind is `Open`** — it already allocates the lease, and it is request/reply,
  so it is reliable and ordered;
- **unbind is `Close`**, a new request/reply frame kind replacing the COM
  `CloseLease`, with the same idempotent first-caller-wins transition.

Events remain genuinely one-way and keep their existing at-most-once, coalescible
semantics, which is what an event is. The generated dispatcher rejects `Event`
today by design, because an event was never valid on the request/reply carrier;
a one-way dispatch path with its own admission, authorization, and drop
accounting is generated when the carrier moves.

##### Discovery closes without a pack ABI break

The earlier plan appended slots to `NativeLiveObjectContractPackApi`. That is
**not necessary**, and an unnecessary break of the single required pack ABI is
not worth its cost.

`create_payload_proxy` forwards an arbitrary non-null `IUnknown` and the payload
bindings already project their own objects, so the payload passes a
**payload-owned sink**. The sink is bidirectional: while the pack is creating the
proxy it calls back on the sink to declare what the payload needs to know.

```csharp
[GeneratedComInterface]
public partial interface IPowerShellBridgeBrokerSink
{
    // The pack calls this once, before returning a proxy.
    [PreserveSig] int Declare(in Guid contractIdentity,
                              nint descriptorHash, int descriptorHashLength,
                              nint payloadVariableUtf8, int payloadVariableLength,
                              int maximumRequestBytes, int maximumReplyBytes);

    [PreserveSig] int Request(uint kind, ulong orderingKey, nint body, int bodyLength,
                              nint reply, int replyCapacity, out int replyLength);
    [PreserveSig] int Post(uint kind, ulong orderingKey, nint body, int bodyLength);
}
```

Every buffer crosses as a pointer and a length rather than a span. That is not a
style choice: a `ReadOnlySpan<byte>` parameter on a `[GeneratedComInterface]`
method fails to compile with `SYSLIB1051`, because the span marshaller does not
support the unmanaged-to-managed direction a callback into the payload requires.

The unambiguous "not supported" answer a v1 pack must give comes from the
**descriptor**, not from a return code: a v2 contract sets a new
`PowerShellLiveObjectDirection` bit, and the payload only offers the sink to
contracts carrying it. A v1 pack never declares that bit and is therefore never
asked, so a generic `E_FAIL` from a v1 pack can never be confused with a broken
v2 pack. The bit changes no struct layout and no slot, but it is **not free**:
Rust independently rejects any direction outside `0x03`, so it needs a
validation change on both sides and a baseline update. That remains far smaller
than appending function-pointer slots to the pack ABI.

Ownership follows the proven v1 pattern on both sides: the payload releases the
transit pointer after `create_payload_proxy` returns, and the pack proxy owns its
imported `ComObject` until release, then final-releases it.

##### Routing: a channel carries bridge traffic or application traffic, never both

Multiple waiters are permitted on one channel and each queued frame goes to
exactly one of them, so an application pump and a bridge pump on the same channel
take each other's frames. A channel-owned multiplexer would make separation a
routing property; a dedicated channel makes it structural, which is the stronger
guarantee and the one chosen.

`{ contractId: u32, ordinal: u32 }` still prefixes every bridge frame, because
several contracts may share the bridge channel. `contractId` is derived from the
descriptor hash so it cannot drift from the contract it names, and the pump
rejects a second contract presenting the same value at registration rather than
mis-routing at run time.

**A builder carries exactly one channel, and that is the rule, not a limitation.**
`set_broker` already rejects a second attach with `MULTI_PWSH_BACKPRESSURE`, so
mutual exclusion is the behaviour the ABI enforces today: allowing a bridge and
raw `$DpsBroker` in one invocation would be the *change*, not the restriction.

An invocation therefore uses its channel for a generated bridge **or** for raw
`$DpsBroker`, never both. The supported way to do both is two invocations.

The decisive argument is not cost, it is what the rule makes true. Under mutual
exclusion, for a bridge invocation this holds:

> every application request this invocation can make goes through the generated,
> authorized, leased contract surface.

Allow both and it becomes false. The script would hold a typed surface with
per-accessor authorization, lease validation, and tombstoning next to a raw frame
channel with none of it — and the realistic failure is an application raw handler
that does what a staged member would have staged, without the staging. A closed
surface loses most of its value when an open one sits beside it in the same
invocation.

The restriction is also the reversible choice: adding a second channel later is
additive, while shipping both and restricting later would break consumers. Both
surfaces are unreleased, so the capability being withheld has no users.

The attach failure must name the rule rather than only its mechanism, and say
that two invocations are the supported alternative.

##### How this is decomposed

Two design passes on one increment produced twelve findings and then nine. A
design that does not converge across two passes usually means the increment is
too large rather than the review too harsh — two passes on the compiler
converged. So the carrier move is split, and each piece is independently
testable and survives a decision already made.

**Leases are invocation-scoped, decided up front rather than during the carrier
move.** Bind and unbind *are* `Open` and `Close`, and an unbind arriving while a
call is in flight is the same idempotent first-caller-wins transition as close,
never a second mechanism.

The argument for it is not that a captured object should not outlive its
invocation — that presupposes the answer. It is that **the transport is bound
per invocation regardless**, so a session-scoped lease would persist while being
unusable outside an invocation. "Session-scoped" would then name a lease that
survives but cannot be called, which is strictly worse than one that honestly
ends. Once the carrier binds per invocation, invocation scope is forced rather
than chosen.

**One correction first.** It is tempting to say the sink is what creates the
invocation signal that lease scoping needs. It is not: the signal already exists,
because `$DpsBroker` is installed at invocation start and removed at cleanup.
What is missing is a **pack-reachable object** to deliver it to. The sink is that
object. That distinction is what makes the split below work.

1. **The direction bit — landed.** A v2 contract needs a new
   `PowerShellLiveObjectDirection` value, and that is **not** a payload-and-pack
   change: `PowerShellLiveObjectContract.cs` is compiled into the facade, the
   enum is recorded in the public ABI baseline as `facade:type` and
   `facade:field` entries, and Rust independently rejects any direction outside
   `0x03`. So the bit landed on its own — facade enum, both validation masks, an
   additive baseline update, and a contract-pack rejection fixture — before any
   protocol work. It is mechanical, it is where a mistake is expensive and a
   review is cheap, and separating it kept an ABI diff out of the protocol
   commit.

   This split was not a preference. Applying the boundary test to the original
   increment — *does it touch anything outside payload and pack?* — answered no
   on the third check, which is exactly the leading indicator agreed for
   stopping and re-splitting rather than pushing through.

   `BridgeContract` is declared **with** `ConsumerToSession`, never instead of
   it, and both validators enforce the pairing independently rather than
   trusting the other to have checked. The rule is defense in depth plus an
   early, specific diagnosis: a lone marker was already rejected downstream by
   the registry's `ConsumerToSession` requirement, so the rule moves the error
   to the line that made the mistake rather than adding a barrier where none
   existed. The rejection fixture writes raw descriptor bits, because the
   managed contract type refuses to construct one and a pack is native memory
   the consumer does not control.

   **Why the marker lives in the direction enum rather than the reserved
   field.** The objection is a fair one and worth recording rather than
   relitigating: every other member of `PowerShellLiveObjectDirection` names a
   way objects flow, a protocol marker is not a direction, and the descriptor
   already carries a `Reserved` word that both sides validate as zero. Packing
   two axes into one `[Flags]` enum does make some bit combinations meaningless.

   Three things decided it the other way.

   The pairing rule does not go away. "A v2 contract is consumer-to-session" is
   a fact about the domain, not an artifact of packing: with a separate kind
   field, a descriptor could still declare the v2 kind alongside
   `PayloadToConsumer`, which is exactly as meaningless. The constraint would
   move from a cross-bit rule to a cross-field one and would still need writing
   on both sides. Splitting the axes changes where the rule is spelled, not
   whether it exists.

   Both spellings already fail closed against an older host. A pre-marker host
   rejects `0x06` because it is outside the known direction mask, exactly as it
   rejects a non-zero reserved word. Neither is safer than the other on that
   axis, so it does not choose between them.

   The reserved word costs more public surface, not less. It is already a
   recorded baseline field, so giving it meaning means either renaming it — a
   removal, and the first non-additive change to this ABI — or leaving a field
   called `Reserved` that is not reserved. Worse, `PowerShellLiveObjectContract`
   has no path to it at all: `ToNative` never writes it, so carrying a kind
   there needs a new enum type, a second constructor overload, and a new
   property. That is roughly six baseline entries against one, and a permanent
   overload-resolution burden, to move a constraint rather than remove it.

   The long-term case for a separate axis is that protocol kinds accumulate and
   crowd the enum. That future is one this architecture forbids: capabilities
   are added by extending the current version in place with lock-step host and
   payload builds, explicitly not by introducing a parallel protocol version.
   Ruling out the trajectory rules out the cost.

2. **The sink handshake and invocation leases.** With the bit already in place
   this is genuinely payload-and-pack only: the sink interface; the registry
   accepting and routing v2 contracts; the payload bindings driving the
   handshake; a declaration state machine; and the requested identity exposed so
   a pack holding two v2 contracts can select. COM still carries `Invoke`
   throughout, and no facade surface moves.

   Discovery and lifetime are one **increment** but not one **mechanism**, and
   keeping them distinct is what makes the increment tractable: a pack proxy is
   created **once**, when the live-object variable is set, and its **lease** is
   opened at each invocation bind and closed at unbind. Discovery is a COM
   `QueryInterface` and a declaration handshake over metadata that never
   changes, so repeating it per invocation would buy nothing. A pack proxy
   created at variable-set time survives across invocations —
   `liveObjectVariables` retains it and reconciles after each one — so the two
   halves genuinely can have different lifetimes.

   That split was verified independently rather than assumed, and the check
   returned three constraints the implementation must respect:

   - **Retention is by exact reference.** Reconciliation matches a retained
     lease against live runspace variables with `ReferenceEquals`, so a script
     that reassigns, removes, wraps, or copies the value drops the proxy. Only
     an exact alias survives. A bridge proxy is therefore no more durable than
     the variable holding it, and the design must not assume otherwise.
   - **Reconciliation is not transactional.** It disposed as it walked and
     rebuilt afterwards, and reading a lease's value can throw, so an exception
     part-way through could dispose earlier entries and propagate — leaving
     disposed leases listed as live, which poisoned every later reconciliation.
     That is now fixed: it partitions first, swaps the table, then releases, and
     an unreadable lease is treated as dropped rather than allowed to abort the
     walk. The residual hazard is narrower — a release that throws after the
     table is already consistent — and every release is attempted before the
     first failure is reported.
   - **The invocation window has two escape hatches.** `Stop` alone does not
     close it — removal waits for a later completion or disposal — and
     `Cleanup` sets its completed flag *before* doing any work, so a failure
     part-way through skips the rest permanently rather than retrying. Lease
     close must therefore be idempotent and exception-safe on its own terms,
     not merely placed in that path and assumed to run.

3. **The host-side construction API.** Retiring the COM carrier removes what an
   author writes but requires the SDK to grow a replacement that associates a
   dispatcher, a channel, and a payload variable. That is new public facade
   surface and gets its own design and its own adversarial review rather than
   riding along with a transport swap. The construction half is
   `PowerShellRuntime.CreateBridgeChannel`, followed by
   `PowerShellBridgeChannel.CreateBinding` over the generated
   `IPowerShellBridgeDispatcher`; the binding owns its generated dispatcher and
   the channel owns its bindings. It deliberately does not yet expose session
   assignment or builder attachment: until the payload can route frames to that
   binding, exposing either would publish a live-object variable that fails at
   its first member call. Those operations land with the carrier move.

4. **The carrier move.** By then the handshake and the lease semantics exist and
   are tested, so this is closer to the transport swap it was originally scoped
   as: reply envelope, transport-failure mapping, routing prefix, `Open`/`Close`
   over the channel, one-way events, and the pump.

Two triggers end this line rather than one. Scope leaking outside an increment's
own boundary is the leading indicator and is objectively checkable; a design pass
failing to converge is the lagging one. On either, the agreed outcome is to ship
the compiler and the lease runtime on their own and revisit the carrier move as
separate work. Both stand alone and both are green.

##### What this pass still does not solve

The second pass was reviewed and did not fully survive either. It is closer, and
the corrections above hold, but these remain open and several need a product
decision rather than more design:

1. **Discovery is one-directional and cannot select a contract.** The registry
   stores one `create_payload_proxy` per pack and calls it with only an
   `IUnknown`, so a pack holding two v2 contracts cannot know which one the
   payload asked for. The sink must expose the *requested* identity for the pack
   to read before it declares, or a pack must be limited to one v2 contract.
2. **The sink needs a state machine.** The registry accepts any callback that
   returns a non-zero proxy, so it cannot detect a missing or duplicated
   declaration, or traffic sent before binding completes.
3. **`Open`/`Close` as bind/unbind *is* an invocation-scoped lease** —
   **resolved by adopting it.** Endorsing the reuse and deferring the lifecycle
   were the same decision made twice with opposite answers, so the lifecycle is
   adopted up front and the reasoning is recorded above.
4. **A timed-out `Open` can strand a lease.** The dispatcher allocates before
   replying, and a late reply merely fails, so the allocation must be rolled
   back on every delivery failure.
5. **The capacity inequality is not the real budget.** Member maxima already
   include their frame headers, requests and replies need different allowances,
   and `Open`, `Release`, `Close`, and events are not declared members at all.
6. **Transport failure has no payload semantics.** Only a completed frame
   carries a body; cancelled, timed out, aborted, busy, and no-consumer
   outcomes need a defined mapping distinct from bridge statuses.
7. **Routing identifies a schema, not an instance**, so two bindings of the same
   contract on one channel cannot be told apart — and the `Open` frame has no
   lease yet to disambiguate with.
8. **The direction bit is not free after all** — **resolved and landed.** Rust
   independently rejects any direction outside `0x03`, so the bit needed a
   validation change on both sides and a baseline update. That was still far
   smaller than appending slots to the pack ABI, so the conclusion stood and
   only the framing did not.
9. **There is no host-side non-COM construction path.** The only existing
   abstraction requires a generated COM interface and exports an `IUnknown`, so
   deleting the COM carrier leaves nothing that associates a dispatcher, a
   channel, and a payload variable. That is new public facade surface.

**The one-channel question is settled by an explicit role on the single slot.**
Mutual exclusion by itself is not a structural invariant, because `set_broker`
records only the channel and its handle — nothing carries intent, so nothing can
decide. Giving the single slot an explicit role does: an attach marks the channel
as carrying bridge traffic, the payload then deterministically installs bridge
sinks or `$DpsBroker`, never both, and mixed use is rejected outright. One slot,
one decider.

The guarantee that follows holds **per invocation, not per session** — a session
may run a bridge invocation and a raw-broker invocation back to back. With
invocation-scoped leases the two scopes line up: the bridge surface a script can
reach is bounded by the same boundary the guarantee is stated at, rather than a
lease outliving the claim made about it.

The attach failure will name that rule once a bridge can be attached. Stating it
before then would describe a rule no code enforces, so it lands with the attach
path that makes it real.

##### What the move does and does not buy
It moves application code off the pipeline thread and bounds the producer's wait
by its deadline. It does **not** make the dispatcher's no-wait and no-lock rules
structural: `TryReceive` releases the delivery handle before it returns, so the
dispatch-violation guard does not cover what a pump does afterwards, and a pump
must hand off to a worker rather than dispatch inline or one blocking handler
wedges the only pump.

##### Increment two, third pass: the sink handshake and invocation leases

The boundary test was run against this increment before any of it was designed,
and the answer was **no touchpoints outside payload and pack**. Nothing here
adds a Rust export, a payload binding-table slot, a public facade member, or an
ABI baseline entry. That is what makes it a self-contained increment; it is not
what makes it small.

**How a pack is reached today, and why the sink can simply be the object.**
A pack exposes `CreatePayloadProxy` as
`delegate* unmanaged<IntPtr, IntPtr*, int>`, and the registry calls it as
`create(comObject, &proxyHandle)`. There is exactly **one** inbound pointer, and
today it is the consumer's object passed through untouched.

An earlier reading of that concluded the payload must **interpose**: pass an
object answering `QueryInterface` for both the sink and the contract, forwarding
the contract identifier onward. That would have been the largest piece of work in
this increment, because it means the payload owning a COM identity that forwards
an arbitrary interface identifier it does not know at compile time.

It is not necessary, and the reason is visible in how a pack actually consumes
the pointer: it projects it with its own `ComWrappers` and pattern-matches
against interfaces **it** knows, rather than being handed a specific interface.
So the pointer can simply *be* the sink. For a contract marked `BridgeContract`,
the payload passes a payload-owned sink, and the pack — which knows the sink
interface, having been generated against this protocol — matches it and asks the
sink for the consumer's contract object, then wraps that object exactly as it
wraps a v1 one today.

Nothing about this needs new machinery. The payload already exposes its own
managed objects as COM interfaces through `GetOrCreateComInterfaceForObject`
when it exports a live-object probe, so a payload-owned sink is an existing,
exercised shape rather than a new capability. Neither side ever forwards an
interface identifier it does not know: the payload exposes one fixed interface it
owns, and the pack consumes one fixed interface it was generated against.

**The handshake exchanges two fixed COM interfaces, not one.**
`IPowerShellBridgeContractSink`, which the payload owns, exposes
`GetRequestedContract`, `Declare`, and `GetConsumerContract`. The generated pack
projects that sink through its payload-local `ComWrappers`, verifies the
requested transport IID/version, creates its generated binding, and declares a
`[GeneratedComClass] IPowerShellBridgePayloadCallback`. The callback exposes
`Bind(out rootHandle)`, `Unbind`, and `ReleaseRoot(rootHandle)`.

The callback is necessary because the payload knows the invocation boundary, but
the pack owns the generated root wrapper. On `Bind`, the callback opens the
generated client, roots its root wrapper in one `GCHandle`, and returns that
owned handle. The payload installs the root in direct runspace variables. On
unbind it first publishes the unbound placeholder, then calls `Unbind` so the
generated client tombstones every escaped wrapper, and finally calls
`ReleaseRoot`. A close failure therefore cannot leave the live root reachable.
The callback permits only one active root handle; the sink owns the obligation to
release the handle after a successful bind. Both interfaces are fixed and known
to both sides at compile time, so no plain managed interface crosses the
payload-pack assembly boundary and no arbitrary-IID forwarding is required.

**The substitution point already exists and already has what it needs.** The
registry's `CreateLease` receives the contract descriptor alongside the pointer,
and it is the last place both are in scope before the pack is called. So the
branch is a single decision at a single site: a contract marked `BridgeContract`
gets a sink, and anything else gets the pointer it gets today.

**A v1 pack is unaffected, bit for bit.** It must keep receiving exactly what it
receives now, and interposing or substituting for every contract would be a
behaviour change to shipped packages dressed up as an internal refactor. Gating
on the marker is what prevents that, and it is why the marker landed first. It
also makes "this payload does not support v2" a property of the descriptor
rather than something a pack has to infer from a failed `QueryInterface`.

**Discovery and lifetime are separate mechanisms inside one increment.** The
proxy is created once, when the live-object variable is set; the lease is opened
at each invocation bind and closed at unbind. Discovery is a `QueryInterface`
plus a declaration over metadata that never changes — contract identity and
version — so repeating it per invocation would buy nothing and cost a round trip
per invocation per contract. The descriptor hash is compared during `Open`, not
doubled in this handshake. The two can hold different lifetimes because
`liveObjectVariables` retains a proxy across invocations and reconciles after
each one, releasing at session teardown.

Binding and cleanup retain their own unwind guarantees. A failed bind releases
the callback's root handle and returns the lease to `Unbound`; invocation cleanup
keeps variable restoration and `EndInvocation` in must-run `finally` paths.
`Stop` alone does not close the window, so only completion or disposal can run
the unbind transition.

**The declaration state machine.** The sink accepts exactly one matching
declaration, retains a projected callback COM object, and rejects traffic before
the declaration or outside a bind. The handshake therefore has exactly one legal
path — `Created -> Declared -> Bound -> Unbound` — and every other transition is
a defined failure rather than an unobserved one:

| From | Event | To | Otherwise |
| --- | --- | --- | --- |
| `Created` | `Declare` once, matching the requested identity and callback | `Declared` | A second `Declare`, a null callback, or a different identity fails the publish |
| `Created` | anything else | — | Traffic before declaration fails; the pack has not yet said what it is |
| `Declared` | `Bind` at invocation start | `Bound` | A bind naming a proxy that reconciliation already released fails with a named status; re-discovery is explicit |
| `Bound` | `Unbind` at invocation end | `Unbound` | The sink publishes the unbound state before calling pack code; a later duplicate unbind is first-caller-wins |
| `Unbound` | `Bind` at the next invocation | `Bound` | — |
| any live state | proxy release | `Disposed` | The consumer COM reference and projected callback are released |

A pack that never declares is not silently tolerated: the publish fails and the
variable is not set, because a proxy that never said what it is cannot be routed
to and would otherwise sit in the table looking healthy.

**Requested identity.** A pack holding two v2 contracts cannot tell which one it
is being asked for, because the registry stores one `create_payload_proxy` per
pack and calls it with only a pointer. The sink exposes the requested contract
identity for the pack to read **before** it declares, so selection is something
the pack does deliberately rather than something it guesses from the order it
was called in.

Identity is the interface identifier and version, and **not** the descriptor
hash. The second pass asked for the hash here and it cannot be had: the inbound
`NativeLiveObjectContractDescriptor` carries size, directions, interface
identifier and version, and no hash, and the hash exists only in generated pack
code. Supplying it before declaration would mean adding a field to the pack ABI,
which this decomposition exists to avoid. It is also unnecessary, because the
hash is already compared during `Open` and a mismatch is already rejected there
with a status that names it. Checking it twice would have bought a slightly
earlier message at the cost of a breaking ABI change.

**Several variables over one broker share one lease.** The same broker can be
assigned to more than one variable. `PowerShellLiveObject<TContract>` exports it
through its one static source-generated `ComWrappers` instance, so repeated
assignments for the same `TContract` and broker produce the same transport
pointer. The registry compares that pointer while the first sink retains an
owned COM reference, then reuses the existing bridge proxy instead of creating a
second one. The NativeAOT acceptance fixture counts `Open` frames and proves two
variables produce one open per invocation.

Sharing is coherent precisely because leases are invocation-scoped: every proxy
over one broker binds and unbinds inside the same invocation, so a first-wins
close at unbind ends a lease that all of them were finished with anyway. A bind
that fails after the state has moved must roll the state back, so that no proxy
is left reporting `Bound` over a lease that does not exist.

**The proxy a script discarded.** Discovery happens once, so the design owes an
answer to the case where the object is gone by the next invocation.
Reconciliation releases a lease when no variable holds a **direct** reference to
it, which ordinary script reaches without trying: assigning `$a = $null`,
letting the variable fall out of use, or wrapping it as `$b = @($a)` — the
wrapper holds the object, the variable does not, and the reference check does
not see through it.

Binding must fail loudly with a named status when the proxy it names is gone,
and re-discovery must be explicit. This is not a hypothetical preference. The
existing sample already probes the neighbouring case — an escaped child wrapper
used after its root lease was released — and records that PowerShell projects
the failed getter as **null**, so the object reads as present and empty rather
than as revoked. That is the exact failure mode Bridge Contract v2 exists to
remove, and it is why the answer here cannot be silence: the v2 lease runtime
already tombstones every handle at lease end and returns a deterministic revoked
error, and a bind that quietly attached to a released proxy would reintroduce
the wart one layer up.

**What is deliberately not in this increment.** The carrier does not move: COM
still carries `Invoke` throughout. No reply envelope, no routing prefix, no
transport-failure mapping, no pump. Those are the carrier move, and keeping them
out is what lets the handshake and the lease semantics be tested against a
transport that already works.

Two triggers still end this line. Scope leaking outside the increment's own
boundary is the leading indicator and is objectively checkable — a fourth
touchpoint outside payload and pack means stop and re-split. A design pass
failing to converge is the lagging one.

### Consumer dispatcher rules

These are normative for generated and application code reached through
`Invoke`, because backpressure only rejects a later FFI call and cannot see a
lock or dispatcher cycle that makes no FFI call at all:

1. A handler must not start a pipeline, invoke a session, or call any facade
   API that would re-enter the runtime.
2. A handler must not block on another thread that can block on the pipeline —
   in particular it must not marshal to a UI dispatcher and wait.
3. A handler must not wait at all. There is no deadline on the COM transport, so
   a bounded wait cannot be defined here; a handler that needs to wait belongs
   behind the duplex broker channel, not on this carrier.
4. A handler must not acquire a lock that any pipeline-blocking thread can
   hold.

Routing request/reply through the broker channel moves application code off the
pipeline thread and bounds the producer's wait by its deadline. It does **not**
make rules 1-4 structural: the delivery handle is released before `TryReceive`
returns, so the dispatch-violation guard does not cover a pump's later work.
These rules stay load-bearing and enforced by review either way.

### Failure semantics

The COM transport returns an `HRESULT`; typed application failures are `S_OK`
replies whose value is `Error`. Precedence is fixed: structural, then reply
capacity, then lease, then handle, then authorization, then dispatch.

| Condition | Result |
| --- | --- |
| malformed frame, truncated value, unknown ordinal, undeclared tag, shape violation, carrier/kind/argument-count disagreement, **or a declared cap exceeded by an inbound value** | `E_INVALIDARG` (`0x80070057`) |
| descriptor hash mismatch on `Open` | `ContractMismatch` (`0x8007075B`) |
| unknown, closed, or superseded lease; unknown, released, or cross-lease handle; authorization denied | `E_ACCESSDENIED` (`0x80070005`) |
| an application-returned value exceeds a declared cap while encoding the reply | `E_BOUNDS` (`0x8000000B`) |
| a runtime table is full | `E_OUTOFMEMORY` (`0x8007000E`) |
| `outputCapacity` is below the member's declared maximum reply size | `E_NOT_SUFFICIENT_BUFFER` (`0x8007007A`), `outputLength` set to that maximum |
| declared application failure | `S_OK`, `replyKind = Error` |

Inbound decode failures are **all** `E_INVALIDARG`: the codec cannot distinguish
a truncated value from an over-cap value without a classified decode error, and
inventing a distinction the implementation cannot make would be worse than one
honest status. `E_BOUNDS` is reserved for the outbound direction, where the
generator knows the declared cap and the application value separately.

Reply capacity is checked **before dispatch**, against the member's compile-time
maximum reply size from the static member table, not after the handler runs. A
handler therefore cannot perform a mutation and then fail on a buffer the caller
sized too small, which would make a retry duplicate the side effect.

Revoked handles and denied authorization deliberately share one status so a
caller cannot probe which handles exist. `outputLength` is meaningful only for
`S_OK` and `E_NOT_SUFFICIENT_BUFFER`.

### Lease, authorization, and staged mutation

A lease is `(leaseId, generation)`. It is allocated by the consumer during
`Open`, carried in every frame, and revalidated by generated consumer code
**immediately before** each application call — not once at handle creation.

1. Decode and structurally validate the frame. Reject unknown ordinals, wrong
   tags, out-of-bound lengths, a carrier/kind mismatch, and a header tuple that
   disagrees with the transport parameters. Nothing is dispatched.
2. Check `outputCapacity` against the member's declared maximum reply size.
3. **Admit the call**: resolve `(leaseId, generation)` and resolve `objectId`
   within that lease in one atomic step under the lease's own lock. Admission is
   atomic precisely so a call cannot pass the lease check, lose a race to
   closure, and then fail object resolution. A closed, superseded, or unknown
   lease, and a handle minted by another lease or forged, are all rejected here
   with the same revoked error. Object IDs are allocated monotonically per lease
   and never reused inside it, so a released wrapper can never target a later
   object.
4. Authorize. Getter, setter, and each method are authorized separately. The
   declared `Permission` is passed to the authorizer as input; it never stands
   in for the call. The authorizer is application code and is therefore called
   **outside** the lease lock, so it can never deadlock against a handler.
5. Dispatch and encode the reply.

An admitted call holds its resolved handler reference **by value**. A concurrent
closure clears the lease's object table but cannot revoke a reference the call
already holds, which is why closure never has to block on, interrupt, or wait
for work already in flight.

**The application never invents an object identifier.** A handler returns a
child *handler interface*, and the dispatcher registers it and allocates the
identifier; an inbound handle is resolved back to its registered handler within
its own lease before the handler sees it. Returning the same handler twice
yields the same identifier until it is released. This removes handle forgery
from the application's responsibility entirely.

**`Mutation.Staged` is rejected at compile time** (`MPWLC023`) until the staged
lifecycle is deliverable. `PowerShellStagedIntentCoordinator` exposes no
programmatic stage/validate/commit entry point — it is reachable only through
its capability set, driven from script — so a generated dispatcher has nothing
to call. Accepting the declaration and silently not staging would be worse than
refusing to compile it.

When it is delivered, **"invocation" means one member call**: a staged member
stages, validates, and commits within its own `Invoke`, any failure inside that
call aborts every intent it staged, and lease state, generation, and
authorization are re-checked immediately before commit. Multi-member
transactions stay out of scope, because the COM transport carries no invocation
ID, deadline, cancellation, or pipeline outcome; a cross-member transaction
needs explicit begin/commit ordinals and a correlation ID.

v2 will reuse the vocabulary of `PowerShellStagedIntentOperation`,
`PowerShellStagedIntentHandlerResult`, and `IPowerShellStagedIntentHandler`
rather than inventing a second one.

### Lease closure and tombstoning

A lease has exactly one idempotent `Active -> Closed` transition, and both
owners drive the same transition:

- the payload calls `CloseLease(leaseId, generation)` on the COM transport;
- the consumer ends the lease when it disposes its broker.

**First caller wins**, and the two owners are not symmetric. The payload's
`CloseLease` is a call that returns a status: the winner receives `S_OK`, and a
later or stale `CloseLease` receives `E_ACCESSDENIED`. The consumer's disposal
returns nothing and is idempotent, so repeating it is simply a no-op rather than
a failure.

The transition increments the generation, so every later frame carrying the old
generation fails admission. Closure does not interrupt a call that has already
been admitted; that call holds its resolved handler reference by value and runs
to completion. Closure takes effect for every call that has not yet been
admitted, and because admission is one atomic step there is no window in which a
call passes the lease check and then fails object resolution.

At closure every handle in the lease's object table is tombstoned in the same
locked transition, before the lease becomes unreachable. A payload wrapper that
escaped into a longer-lived script variable therefore observes a deterministic
revoked error and retains no application state — only an object ID that no
longer resolves.

Release is an explicit ordinal per object type (`[BridgeObject(..., ReleaseId =
n)]`), not an implicit finalizer.

### Runtime bounds

| Table | Bound | On overflow |
| --- | --- | --- |
| Active bridge leases per process | 16 | `Open` fails with `E_OUTOFMEMORY` |
| Live object handles per lease | 1024 | the allocating call fails with `E_OUTOFMEMORY` |
| Staged intents per member call | 16 | the call fails and aborts every intent it staged |
| Tombstoned object IDs retained per lease | none — IDs are monotonic, so a released ID is simply never re-allocated |

The channel's `MaximumBodyBytes` is validated against the contract's largest
frame **when the bridge variable is bound**, not at lease open: a channel is
attached per invocation, while today a lease is created when a session variable
is assigned, so no channel exists at that point.

### Lease lifetime

The existing consumer-to-session path creates a payload proxy when a live-object
**session variable** is assigned (`PowerShellSession.SetLiveObjectVariable`), so
a lease created there lives as long as that variable, across invocations. v2
does not change that, and does not claim an invocation-scoped lease it cannot
enforce: there is no pack callback marking invocation start and end.

Binding a lease to one invocation requires the same transport bind/unbind
addition as events. Until it exists, a bridge lease is **session-scoped and
consumer-terminated**, and the consumer is responsible for ending it. Session
and runtime lease lifecycles beyond that remain unqualified.

### What the compiler emits

| Emitted | `Host` | `Payload` |
| --- | --- | --- |
| contract constants, canonical descriptor bytes, SHA-256 hash | yes | yes |
| static member table plus generated `TryGetMember`/`TryGetReleaseOrdinal` switches | yes | yes |
| copied data classes and their typed codecs | yes | yes |
| closed enumeration allow-list guards | yes | yes |
| typed handler interfaces, call context, and authorizer interface | yes | no |
| the dispatcher: admission, authorization, decode, dispatch, encode | yes | no |
| CLR wrapper classes, lease client, and inline typed codecs | no | yes |

The dispatcher exposes two entry points. `Dispatch` is **transport-neutral** and
takes spans, so a later carrier can feed it without changing a line of generated
logic. `Invoke` is the COM-shaped entry point, and it copies through managed
buffers so a consumer project never needs `AllowUnsafeBlocks` to host a
dispatcher.

A source generator cannot see another generator's output, so the generator
cannot emit the `[GeneratedComClass]` that implements the contract's transport
interface. The application supplies it, and it is deliberately trivial:

```csharp
[GeneratedComClass]
internal sealed partial class RdmBroker : IRdmBridgeTransport, IPowerShellLiveObjectBroker
{
    private readonly RdmBridgeDispatcher dispatcher;

    internal RdmBroker(IRdmBridgeBridgeHandler root, IRdmBridgeAuthorizer authorizer)
        => dispatcher = new RdmBridgeDispatcher(root, authorizer);

    public int Invoke(ulong leaseId, uint generation, ulong objectId, uint memberId,
                      nint input, int inputLength, nint output, int outputCapacity, out int outputLength)
        => dispatcher.Invoke(leaseId, generation, objectId, memberId,
                             input, inputLength, output, outputCapacity, out outputLength);

    public int CloseLease(ulong leaseId, uint generation) => dispatcher.CloseLease(leaseId, generation);

    public void Dispose() => dispatcher.Dispose();
}
```

That is the whole hand-written surface, and it is pure forwarding: it holds no
lease state, decodes nothing, and authorizes nothing.

### Lock-step builds

Host and payload contract builds are **lock-step**. Both sides compile the same
declaration and therefore compute the same descriptor and the same SHA-256, and
the `Open` handshake rejects any difference on both sides.

There is **no contract-layer minor compatibility lane**: no descriptor
subsetting, no optional member negotiation, no additive-member tolerance, and no
"payload is newer" path. Introducing one changes what a revoked or unknown
ordinal means and needs separate architecture approval before it is designed.

### Rejected at compile time

Each of these is a generator error with a specific diagnostic and an actionable
message, not a runtime failure:

`object`, `dynamic`, `Type`, `PSObject`, any SMA type, delegates and function
pointers, `Task`/`ValueTask`, generics other than `IReadOnlyList<T>` and
`Nullable<T>`, `ref`/`out`/`in` parameters, pointers, `IntPtr`/`UIntPtr`,
`decimal`, `DateTime`/`DateTimeOffset`/`TimeSpan`, `SecureString`,
`PSCredential`, any type name containing `Credential`, arrays other than
`byte[]`, unannotated reference types, nested interfaces, static members,
interface inheritance across the contract boundary, cyclic data graphs, `Data`
inside `Data`, `Handle` inside `Data`, `List` inside `List`, `List` of `Data`
inside `Data`, unbounded strings, unbounded byte payloads, unbounded
collections, indexers without an explicit bound, `[Flags]` enums, aliased or
non-`Int32` enum members, `Mutation.Staged`, duplicate ordinals, zero ordinals,
names beginning with the reserved `__bridge` prefix, an object unreachable from
the root, and any member without an explicit attribute.

`[Flags]` is rejected because a combined flag value is not equal to any declared
member, so the closed `Enum32` allow-list cannot validate it.

| Rule | Meaning |
| --- | --- |
| `MPWLC011` | Bridge contract mode is required |
| `MPWLC012` | Invalid bridge contract root, transport interface, or mixed v1/v2 declaration |
| `MPWLC013` | Invalid bridge object |
| `MPWLC014` | Invalid bridge member |
| `MPWLC015` | Invalid bridge setter |
| `MPWLC016` | Bridge member requires an explicit bound |
| `MPWLC017` | Unsupported bridge type |
| `MPWLC018` | Unresolved bridge object reference |
| `MPWLC019` | Invalid bridge data contract |
| `MPWLC020` | Invalid bridge enumeration |
| `MPWLC021` | Invalid bridge event |
| `MPWLC022` | Bridge contract exceeds a structural or frame-size limit |
| `MPWLC023` | Invalid bridge mutation or authorization metadata |
| `MPWLC024` | Invalid bridge release ordinal |

### What Bridge Contract v2 deliberately does not carry

- no reflection, `IDispatch`, dynamic binder, expression trees, or
  `System.Text.Json`;
- no runtime member names, member discovery, or contract negotiation;
- no CLR object identity across the boundary — a `Handle` is a lease-scoped
  integer, not a pointer or a `GCHandle`;
- no `PSObject`, `ErrorRecord`, runspace, or any SMA type;
- no delegates, events with add/remove accessors, or callbacks from script;
- no credential, `SecureString`, or secret material;
- no injection into a remote runspace: a payload wrapper is a local managed
  object and would be serialized, stripping its members;
- no ambient authorization — every getter, setter, and method is authorized on
  every call.

## Explicit limitations

This is not binary or source compatible with the full PowerShell SDK. The
facade cannot transfer live `PSObject`, runspace, delegate, custom `PSHost`, or
arbitrary CLR object values. It exposes no SMA types. Secret transfer is limited
to `PowerShellSecret` and `PowerShellCredential` parameter leases and the
dedicated, explicitly shaped secret-result API. It does not make arbitrary
PowerShell safe or provide a universal zeroization guarantee. Do not put
secrets in tagged values or session variables; snapshots are copied general
output, not a secret store. Snapshot values are
intentionally copied data rather than live objects: they do not provide
arbitrary property access, opaque object handles, callbacks, pools, or generic
CLR object serialization.

Sessions do not accept live `PSHost` values, arbitrary callbacks/delegates,
runspace connection information, remoting transports/providers, nested live
PowerShell, steppable pipelines, generic CLR values, or arbitrary initial
session-state objects. `PowerShellRemoteSession` is a finite exception: it can
create and remove a payload-owned PSSession with fixed endpoint options and an
optional credential lease, but exposes no transport abstraction, remote object,
or bridge-object injection. Capability callbacks are registered immutable DTO
handlers only; there is no host callback vtable, callback rooting,
prompt/credential callback, or arbitrary delegate/object bridge. The Duplex
Broker Channel does not change this: it is a pull pump over bounded opaque byte
frames, so it adds no callback into the consumer, no delegate rooting, and no
object bridge. Custom
remoting and actual session pools remain permanent non-goals until a separate
bounded architecture proves their lifecycle and concurrency semantics.

Direct initialization resolves `pwsh` from `PATH` by default or accepts an
explicit payload root. The SDK does not use a payload metadata file or pin a
PowerShell version.

## Package preview

`Devolutions.MultiPwsh.Sdk` is a `net10.0` preview package with native assets for the
same release RIDs as `Devolutions.MultiPwsh.Cli`: `win-x64`, `win-arm64`,
`linux-x64`, `linux-arm64`, `linux-arm`, `osx-x64`, and `osx-arm64`. The Rust
cdylib is inert by default; consumers set a matching `RuntimeIdentifier` and
`DevolutionsMultiPwshSdkEnabled=true` to stage that RID's native library beside
the executable. The package deliberately does not carry a PowerShell payload.
Release CI consumes the package and ABI-smokes native staging/loading on
`win-x64`, `linux-x64`, `osx-x64`, and `osx-arm64`; this does not validate
PowerShell payload activation on those platforms.
Its NuGet version and Windows native DLL `FileVersion`/`ProductVersion` match
the `multi-pwsh` CLI release version. Only the `win-x64` RID currently has
end-to-end NativeAOT payload smoke coverage; publishing another asset is not a
claim that its payload activation topology has been validated.

### Deployment and rollout contract

The application must opt in to `win-x64` native staging, deploy the selected
PowerShell payload and any selected modules separately. The application must not add a
transitive `System.Management.Automation`, `Microsoft.PowerShell.SDK`, or
payload runtime asset to the NativeAOT facade path. Activation reports a
deterministic incompatibility if another selected payload/runtime already owns
the process. Structural payload checks ensure only that required host files
exist; they do not attest to payload provenance, integrity, module manifests,
or application deployment policy. Those remain application responsibilities.

`win-arm64`, Linux, and macOS have packaged native assets but remain unvalidated
for payload activation and must not be advertised as supported until each has a
real NativeAOT activation smoke test. Current coverage does not prove PowerCLI
or binary module support.

   Adopt this as an additive application feature-flagged deployment:

   1. Start with one-shot local administrative commands in safe
   `-WhatIf`/mocked contract tests.
   2. Move value-only resolver and script flows only after their state fits copied
   variables and their credential requirement is absent.
3. Add persistent approved-module local sessions only after their actual
   payload/module dependencies are exercised.
   4. Keep existing SMA-backed paths for live application object injection,
   `PSObject.BaseObject`, typed generic invoke/PowerCLI, remoting, SSH/WinRM,
   and process-host scenarios.

The non-negotiable non-goals are live SMA identity, `PSObject.BaseObject`,
generic typed invocation, `PSDataCollection` live events, custom
`PSHost`/`PSCmdlet`/provider/remoting inheritance, private
`RemoteSessionNamedPipeServer` reflection and `Enter-PSHostProcess`, PSRP/WSMan
or SSH custom transport, and transparent binary PowerShell-module proxies.

## Validation

Build the managed bindings before Rust because `pwsh-host` embeds the Release
bindings assembly:

```powershell
dotnet build dotnet/bindings/Devolutions.PowerShell.SDK.Bindings.csproj -c Release
cargo test -p pwsh-sdk-ffi --all-targets

$env:PWSH_FFI_PAYLOAD = 'C:\Program Files\PowerShell\7'
cargo test -p pwsh-sdk-ffi explicit_payload_round_trip_uses_the_exported_abi -- --ignored
cargo test -p pwsh-sdk-ffi explicit_payload_async_operations_are_terminal_and_lifetime_safe -- --ignored
cargo test -p pwsh-sdk-ffi explicit_payload_lifecycle_stress_enforces_serialization_and_lifetime_contracts -- --ignored
cargo test -p pwsh-sdk-ffi explicit_payload_increment_6_sessions_are_bounded_and_lifetime_safe -- --ignored

dotnet publish dotnet/nativeaot-sample/NativeAotFfiSample.csproj -c Release
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe
# Or select a payload explicitly:
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload>
# Contract-pack rejection fixtures (each must exit 0 by being rejected):
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:duplicate-across-packs
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:duplicate-within-pack
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:direction-violation
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:reserved-identifier
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:unsupported-pack-abi
./dotnet/nativeaot-sample/bin/Release/net10.0/win-x64/publish/NativeAotFfiSample.exe <payload> --expect-rejected-contract-pack:bridge-marker-without-direction

dotnet pack dotnet/sdk-ffi/Devolutions.MultiPwsh.Sdk.csproj -c Release -o artifacts/sdk-nuget
pwsh -NoLogo -NoProfile -File tests/Test-PwshFfiPackage.ps1 `
    -PackageSource artifacts/sdk-nuget `
    -PowerShellPayloadDirectory $env:PWSH_FFI_PAYLOAD
```

The ignored Rust test and NativeAOT sample require a real payload root containing
`pwsh.dll`, `pwsh.runtimeconfig.json`, and
`System.Management.Automation.dll`. The packaged Win-x64 consumer requires a
PowerShell 7.4 or 7.7 payload by default and verifies typed-result paging through
the managed facade, native host, and required V1 payload binding table.

### Recording the qualified PowerShell version

Qualification tracks the **latest released 7.4.x and 7.7.x** on Windows x64;
it is not pinned to fixed patches. The CI installer and package harness record
the exact builds exercised instead of asserting hardcoded patches:

- `scripts/Install-PowerShell74ForCi.ps1` defaults to the newest non-prerelease
  `v7.4.*` tag and exports it as `PwshQualifiedVersion`. CI also runs it with
  `-VersionLine 7.7` to install the newest released
  `v7.7.*` tag and export `Pwsh77QualifiedVersion`, without changing the 7.4
  binding-discovery path. It verifies that each extracted `pwsh` reports the
  requested version and records it in the job summary.
- `tests/Test-PwshFfiPackage.ps1` records the payload's `$PSVersionTable`
  version, and cross-checks it against the `PowerShellFileVersion` that
  `PowerShellRuntime.Diagnostics` reports through the packaged consumer. The
  file version carries an extra build component (for example payload `7.4.17`
  reports `7.4.17.500`), so the check is a prefix match and fails if the
  diagnostics report a different patch than the payload under test.

A release note or CI run therefore states which PowerShell builds the SDK was
qualified against; it never claims a patch that was not exercised.
