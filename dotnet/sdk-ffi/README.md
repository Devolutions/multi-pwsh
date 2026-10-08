# Devolutions.MultiPwsh.Sdk

Experimental `net10.0` NativeAOT facade for an in-process PowerShell payload
hosted by the `multi-pwsh-sdk.dll` Rust library.
Its public C# types remain in the `Devolutions.PowerShell.Ffi` namespace.

This package ships native FFI assets for `win-x64`, `win-arm64`, `linux-x64`,
`linux-arm64`, `linux-arm`, `osx-x64`, and `osx-arm64`. It does not include
PowerShell. Only `win-x64` currently has end-to-end NativeAOT payload smoke
coverage. Release CI also runs a package-consumer ABI smoke on `win-x64`,
`linux-x64`, `osx-x64`, and `osx-arm64`; that verifies native-library loading,
the ABI export, and RID-specific staging, not payload activation.

By default, activation locates `pwsh` on `PATH` and loads its containing
payload directory. Supply a directory to select a payload explicitly:

```csharp
using Devolutions.PowerShell.Ffi;

PowerShellRuntime defaultRuntime = PowerShellRuntime.Activate();
PowerShellRuntime selectedRuntime = PowerShellRuntime.Activate(payloadDirectory);
```

The application controls payload provenance and file integrity. The SDK does
not read, package, or validate a PowerShell payload manifest.

Enable native-asset staging in the consuming project:

```xml
<PropertyGroup>
  <RuntimeIdentifier>win-x64</RuntimeIdentifier>
  <DevolutionsMultiPwshSdkEnabled>true</DevolutionsMultiPwshSdkEnabled>
</PropertyGroup>
```

The facade deliberately exposes no SMA types and supports one selected
PowerShell/CoreCLR runtime per process. NativeAOT plus dynamically hosted
CoreCLR is an experimental deployment condition, not a generally supported
.NET hosting topology.

Native deployment is inert by default even for a matching RID. Set
`DevolutionsMultiPwshSdkEnabled=true` to stage the package's
RID-specific native library beside the application. The package never
embeds a PowerShell payload. Windows native assets use a static MSVC runtime
and the dedicated unwind-enabled `ffi-release` Rust profile, so native panic
containment remains effective at the ABI boundary.
The SDK NuGet version and the native DLL `FileVersion`/`ProductVersion` match
the `multi-pwsh` CLI release version.

## Live-contract generator preview

The package carries a `net8.0` compile surface for
`LiveContractAttribute`, `LiveObjectAttribute`, and `LiveMemberAttribute`, plus
the incremental generator under `analyzers/`. This lets a trusted `net8.0`
payload-pack project compile the same explicitly annotated contract declaration
as the `net10.0` NativeAOT host without referencing the NativeAOT facade
assembly. In `Payload` mode the package injects its small contract source files
into the payload compilation and removes the contracts DLL from the compiler
reference set. This is required because trusted packs are loaded from bytes and
cannot resolve a normal external contracts assembly. Do not manually link a
second copy of those contract source files in a Payload project. NuGet
auto-loads the analyzer; the package's transitive target makes the project
`LiveContractMode` visible to it:

```xml
<PropertyGroup>
  <LiveContractMode>Payload</LiveContractMode>
</PropertyGroup>
```

Compile the host declaration with `LiveContractMode=Host` and the trusted
payload declaration with `LiveContractMode=Payload`. Both sides must use the
same source-declared IDs and contract version. This remains a restricted preview: it does not expose a generic object bridge,
reflection dispatch, callbacks, credentials, `PSHost`, remoting, or arbitrary
CLR values. For the supported root/add/collection/string-property graph, the
generator emits a bounded staged host adapter and static public payload
wrappers over the single broker interface. The host must continue to end the
lease authoritatively; generated root disposal never releases a lease held by
potentially retained child wrappers.

The facade requires native ABI v2. The jointly shipped V1 payload binding table
includes both `ReadStreamBatch` and typed-result paging; use matching package,
native asset, and managed payload bindings. Its `SafeHandle` wrapper keeps a
native handle alive for each P/Invoke call, including concurrent disposal races.
Empty strings are valid UTF-8 inputs; embedded NUL characters are rejected.

`PowerShellValue` is the only way to pass non-string values. It supports
bounded primitive values, exact-tick `TimeSpan` values, bytes, arrays, and
property bags that become copied PSCustomObject-style snapshots.
`PowerShellValue.From` rejects delegates and unsupported CLR objects with
`PowerShellValueConversionException`; raw objects are never passed to
PowerShell. `AddInput` is synchronous and bounded to 64 values/64 KiB. Call
`CompleteInput` before invoking a started input collection, or use
`ResetInput`/`Clear` to discard it.

`Invoke` returns an immutable `PowerShellInvocationResult`. Its output and
standard PowerShell streams are bounded snapshots rather than SMA objects:
each stream retains at most 32 records and each text field is capped at 4,096
UTF-16 code units. Snapshot metadata reports truncated records/fields and a
global stream sequence, plus total/dropped record counts. Output snapshots can
carry a copied safe scalar and a bounded tagged property bag containing only
scalar `PSNoteProperty` values; complex values and enumerables are not
traversed. Error snapshots add copied category/target, command, source,
pipeline context, and the exception HResult. Per-error terminal status is
intentionally unavailable: `IsTerminatingFailure` is result-level only. A
terminating invocation throws `PowerShellInvocationException` with the same
result snapshot.
Results also expose a monotonic invocation ID, terminal state, and `HadErrors`
metadata without exposing any SMA type.

`runtime.ParseScriptParameters(script)` supports script-editor parameter
metadata without executing `script`. It returns copied parameter DTOs (name,
declared type/default-expression spelling, mandatory flag, description/help
text, and `ValidateSet` entries) or copied syntax-error DTOs; it never exposes
SMA AST/token objects. The API accepts at most 64 KiB of source and fails rather
than truncating more than 16 parameters, 16 `ValidateSet` values, or 16 parse
errors.

`PowerShellSnapshotSerializer` provides deterministic version-1 UTF-8 JSON for
storage or display of immutable invocation results. Documents are capped at
1 MiB and reject unknown members, invalid versions, malformed tagged values,
and invalid bounds. Deserialization only rebuilds copied facade DTOs; it never
creates PowerShell/SMA, live CLR objects, or object handles.

`BeginInvoke` returns a native-owned `PowerShellInvocationOperation`; use
`Poll`, bounded `Wait`, `Stop`, and `GetResult` for explicit lifecycle control.
`InvokeAsync(CancellationToken)` waits for native terminal cleanup before
completing a cancelled task. Cancellation is idempotent and wins over a
concurrent completion, so cancelled operations expose no partial result.
Async input is limited to the existing copied, bounded producer: add input,
call `CompleteInput`, then start the operation. Input cannot be fed after
start.

`ReadStreamBatch(afterSequence, maximumRecords)` polls an active operation's
immutable copied display records. Batches contain ordered stream kind/sequence/
text records, terminal state, and explicit cursor-loss and truncation counters.
The operation retains at most 32 records; pass the previous `NextSequence` to
advance the cursor and treat any loss or truncation as incomplete display data.
This is neither a live console, `PSHost`, callback/event API, nor a stream of
SMA objects, credentials, or other CLR references. Cancellation can expose
already-captured records, but still never produces a successful final result.

## Bridge Contract v2 preview

`BridgeContractAttribute`, `BridgeObjectAttribute`, `BridgeMemberAttribute`,
`BridgeEventAttribute`, `BridgeDataAttribute`, `BridgeFieldAttribute`, and
`BridgeEnumAttribute` opt an application object graph into the closed Bridge
Contract v2 compiler. It is a second, separate attribute family: the v1
`[LiveContract]` preview above is unchanged, and a compilation declares one root
or the other, never both.

v2 models a finite DAG of at most 64 object interfaces and depth 8, with
property getters and setters, bounded methods, explicitly bounded collections,
nullable and enumeration values, data-transfer interfaces, typed error data, and
one-way events. Events are generated one-way ordinals, not CLR `event`
accessors, so no delegate ever crosses the boundary.

Each contract declares the IID of a COM transport interface that the contract
author writes by hand, because a source generator cannot see another generator's
output and the payload pack registry keys on a unique interface identifier. The
generator verifies that interface exists with the required shape.

Both sides compile the same declaration — the consumer with
`LiveContractMode=Host` and the trusted payload pack with
`LiveContractMode=Payload`. The generator emits, identically in both modes, a
canonical descriptor byte sequence, its SHA-256 hash, static member tables, the
copied data classes, and their typed binary codecs. Host mode additionally emits
typed handler interfaces, a call context, an authorizer interface, and the
dispatcher; payload mode emits the CLR wrapper classes that script uses with
ordinary property and method syntax.

The dispatcher admits every frame against bounded lease and object tables,
resolving the lease and the object handle in one atomic step, then authorizes
the getter, the setter, or the method independently before the handler runs. The
application never invents an object identifier: a handler returns a child
handler interface and the dispatcher allocates the identifier, so a forged,
stale, or cross-lease handle can never reach application code. At lease closure
every handle is tombstoned in the same locked transition, so a payload wrapper
that escaped into a longer-lived script variable fails with a deterministic
revoked error.

A source generator cannot see another generator's output, so the application
supplies a small `[GeneratedComClass]` that implements its contract's transport
interface and forwards to the dispatcher. It holds no lease state, decodes
nothing, and authorizes nothing; `docs/in-process-ffi.md` shows it in full.

Everything the compiler accepts is enumerated in source. `object`, `dynamic`,
`Type`, `PSObject`, delegates, `Task`, generics other than `IReadOnlyList<T>`
and `Nullable<T>`, `ref`/`out`, arrays other than `byte[]`, pointers,
credentials, `[Flags]` enumerations, unannotated reference types, cyclic data
graphs, cross-boundary interface inheritance, unbounded strings and collections,
and members without an explicit attribute are compile errors with actionable
`MPWLC011`-`MPWLC024` diagnostics. Generated code contains no reflection,
`IDispatch`, dynamic binder, or JSON serializer path.

Bounds are per position and never inherited: a member-level cap applies to the
result, and every bounded parameter declares its own `[BridgeBound]`. Declaring
a member whose bounds could produce a frame above 64 KiB fails the build rather
than failing at run time.

`BridgeMutation.Staged` is rejected at compile time until the staged-intent
coordinator exposes a programmatic stage/validate/commit entry point.

Host and payload contract builds are **lock-step**. The lease handshake carries
the payload's descriptor hash, the consumer compares it before allocating a
lease, and the payload verifies the echoed hash in the reply, so neither side
accepts a mismatched artifact. There is no contract-layer minor compatibility
lane, no member negotiation, and no additive-member tolerance.

Request, release, and lease-open frames travel over the contract's COM transport
through the existing consumer-to-session pack registry. A v2 descriptor declares
`ConsumerToSession | BridgeContract`; only for that marker the payload supplies
its fixed `IPowerShellBridgeContractSink` to `CreatePayloadProxy`, leaving v1
packs bit-for-bit unchanged. The generated pack binding reads the requested
IID/version, declares its fixed `IPowerShellBridgePayloadCallback`, and receives
an invocation-scoped generated root through an owned GC handle. The payload
unbinds that root at completion before releasing the handle, so escaped wrappers
are deterministically revoked.

The host construction surface is already explicit:
`PowerShellRuntime.CreateBridgeChannel` creates a channel and
`PowerShellBridgeChannel.CreateBinding` associates one generated
`IPowerShellBridgeDispatcher` with it. A binding owns its dispatcher and the
channel owns its bindings. Session assignment and builder attachment deliberately
remain unavailable until generated frames use this channel; publishing either
against the current COM carrier would create a variable that fails at its first
member call.

**Duplex broker delivery of declared events is not wired yet**: a consumer event
sink is obtained by `QueryInterface` on the contract transport and a consumer
that supplies one must return without blocking. A lease with no sink fails an
event call deterministically instead of degrading silently.

### One channel, one purpose

When the bridge moves onto the duplex broker channel, an invocation will use its
channel for a generated bridge **or** for raw `$DpsBroker`, never both. Attaching
a second channel to one builder already fails today, so this is the behaviour the
runtime enforces rather than a new restriction. To use both, run two invocations.

This is a product statement, not an implementation detail. Under it, for a bridge
invocation, *every* application request the script can make goes through the
generated, authorized, leased contract surface. Allowing a raw frame channel
beside it would put a surface with no authorization, no lease validation, and no
staging next to one that has all three, in the same invocation — and a closed
surface loses most of its value when an open one sits beside it.

`docs/in-process-ffi.md` carries the normative wire, descriptor, dispatcher,
failure, lease, authorization, and staged-mutation rules.

### Finite operation and typed report-page preview

`PowerShellFiniteOperationRegistry<TPage>` in
`Devolutions.PowerShell.Ffi.LiveObjects.FiniteOperations` is an opt-in host-only state machine
for an application-selected generated Bridge Contract v2 page type. It has no
generic script or job dispatch surface: the application supplies a fixed schema
ID, a direct detached-copy codec, snapshot/permission validator, and only the
specific generated members it intends to expose.

Each operation ID is a random opaque `Guid`, but access always also requires a
host-only `PowerShellFiniteOperationOwner`; the ID alone cannot authorize or
probe an operation. Active deadlines are capped at one hour and terminal
retention at fifteen minutes. Deadline, cancellation, and already committed
terminal outcomes have deterministic precedence; cancellation is idempotent.
Terminal entries become explicit `Expired` tombstones after retention and keep
their bounded slot until `TryRelease` or owner disposal.

The supplied codec must make detached pages and report exact item and byte
counts. The registry enforces hard page, item, and byte bounds and revalidates
the original snapshot and permission revisions before every page read.
Snapshot/permission invalidation, bounds failure, expiry, and release are
deterministic terminal outcomes. It does not supply durable sessions or
checkpoints, persistence, generic targets/schemas, reflection/JSON, remoting,
credentials, UI, callbacks, pools, or staged mutation semantics.

## DTO projections and bounded paging

`PowerShellDtoContractAttribute` and `PowerShellDtoMemberAttribute` opt an
application DTO into the package's separate incremental source generator. The
generator emits direct `Read`, `TryRead`, and `Write` methods for a versioned
`PowerShellValue` property bag; no reflection or runtime type discovery is
used. Contracts require public settable properties and a public parameterless
constructor. Supported members include checked signed/unsigned integer widths,
nullable copied scalars, non-`[Flags]` enums with explicit underlying integer
conversion, `TimeSpan`, bounded annotated nested DTOs, and one-dimensional
arrays of supported scalars or nested DTOs. Cyclic graphs and graphs deeper
than the tagged-value depth bound are rejected at compile time. Every property
bag carries an exact `$version` value. By default unknown properties, missing
required properties, incorrect scalar kinds, and string/array limit violations
return a structured `PowerShellDtoProjectionError`; `Read` raises the
corresponding typed exception. This is an application DTO mapper, not a
serializer for arbitrary CLR graphs, PowerShell objects, credentials,
callbacks, or live-object contracts.

`PowerShellCompleteResultProjection.Read` connects an already completed
`PowerShellInvocationResult`, or the full ordered typed/observed result-page
sequence, to one application-selected generated mapper. Pass that mapper
explicitly (for example, `MyDtoPowerShellDtoProjection.Read`). It requires
exactly one copied result and rejects zero or multiple results,
incomplete/truncated/dropped page sequences, and mapper failures with
distinguishable `PowerShellCompleteResultProjectionFailure` values. It neither
invokes PowerShell nor performs reflection, object serialization, or arbitrary
`PSObject`/CLR transfer.

`PowerShellValuePager` is the reusable bounded acknowledgement state machine
for copied result values. Its caller-configured record and page bounds apply
backpressure to writers, and its pages expose ordered sequences plus an
acknowledgement cursor. Records remain retained only until
`Acknowledge(sequence)` removes them. `GetCompletion().IsComplete` is true
only after a successful terminal state and acknowledgement of every produced
record; cancellation, disposal, a terminal error, or unacknowledged records
are never silently complete. This primitive is deliberately distinct from
`ReadStreamBatch`: it does not turn the existing lossy display stream into a
typed data feed, and it never exposes SMA values or unbounded retention.

`BeginTypedResultInvocation(options)` connects that acknowledgement model to
an invocation's output through the required V1 payload binding table.
`Read(acknowledgedThrough, maximumRecords)` acknowledges the prior page and
returns only copied `PowerShellValue` records (including bounded arrays and
property bags), never SMA objects, `PSObject`, or display text. The configured
buffer is a hard producer backpressure limit; values are not dropped to make
room. Each page reports total, dropped, truncation, terminal, and completeness
metadata. Outputs that cannot be represented losslessly as a documented tagged
value terminate the typed operation with `UnsupportedValue` rather than being
converted to text or silently omitted.

`BeginObservedInvocation(options)` runs one pipeline with two independently
acknowledged bounded channels: `ReadResults` returns copied typed
`PowerShellValue` records and `ReadDiagnostics` returns lossless copied text
records for output, error, warning, verbose, debug, information, and progress.
Both buffers apply producer backpressure; neither drops records to make room.
Terminal success is reported only after both channels have reached successful
terminal states and every record in each has been acknowledged. Cancellation,
disposal, encoding failure, and any terminal failure are incomplete. This is a
single execution, not a pairing of separate result and diagnostic invocations,
and it never exposes SMA objects or callbacks.

`PowerShellRuntime.CreateSession(PowerShellSessionOptions)` creates a separate,
reusable local-runspace session. `PowerShellSessionConfiguration` supplies
copied tagged initial variables, module imports/paths, a working directory,
environment values, and the `Default`/`Restricted` execution-policy subset.
Paths are absolute existing directories, imports are names resolved beneath the
application-supplied module paths, and
current-runspace sessions reject every configuration field. This is not a
general `InitialSessionState` or arbitrary module-loading API. `GetSnapshot`
and `GetEvents` are bounded polling APIs; events retain at most 32 numeric
state records and report truncation. A session is not a pool. Normal pipeline
execution and builder mutation, including calls through independent sessions,
are process-globally serialized; only `Stop`/cancellation can run concurrently
with an active pipeline.

`ValidateSessionConfiguration` preflights the same module-root and import
resolution rules without creating a runspace, importing a module, or executing
PowerShell/module code. Its immutable copied report identifies invalid or
missing roots, unresolvable imports, and invalid/unreadable manifests. Module-
loading manifest declarations must be static and any path-like declaration must
resolve beneath its approved root (including through reparse-point ancestors);
otherwise preflight rejects it. For static manifests it reports a bounded
declared version and up to four bounded declared commands; declaration
extraction is informational and is not module authorization or execution.

`PowerShellRuntime.Diagnostics` is an immutable, descriptive deployment report.
It exposes the canonical active payload directory, an explicitly nullable
PowerShell file version, payload binding-table V1 size/slot/ABI facts, negotiated
feature flags, and registered contract-pack adapter type names. It does not
return environment data, user or machine identity beyond the canonicalized
payload path, assembly paths, hashes, integrity/attestation claims,
deployment-policy verdicts, or payload objects; the report does not change
runtime state. The payload directory is the runtime's canonicalized active
payload directory, not a verbatim echo of an activation argument: on Windows it
is an extended-length `\\?\` path, and `PATH`-based activation resolves it at
runtime. Do not compare it against an input string, and redact it before
writing the report to shared logs or external telemetry.

`CreateSessionPool` is an intentional rejection boundary: it validates a
maximum bound of 1–64 sessions then returns
`UnsupportedCapability`, rather than faking concurrent pool behavior.

Sessions can also retain copied declarative values without exposing
`SessionStateProxy` or SMA:

```csharp
session.SetVariable(
    "connection",
    PowerShellValue.PropertyBag(
    [
        new("ComputerName", PowerShellValue.String(computerName)),
        new("Port", PowerShellValue.SignedInteger(port)),
    ]));

if (session.TryGetVariable("connection", out PowerShellValue? connectionSnapshot))
{
    // This is a fresh bounded copied value, never a PSObject or a proxy.
}
```

`SetVariable`, `TryGetVariable`, and `RemoveVariable` accept only bounded
`PowerShellValue` graphs and ASCII identifier names. They reject a session with
a pending/running async invocation, and they return `UnsupportedValue` rather
than stringifying a value that cannot be copied. Do not use these APIs for
credentials, live application proxy objects, or callback objects.

`PowerShellCommandRecipe` and `PowerShellScriptRecipe` provide bounded,
declarative one-shot calls. An optional `PowerShellResultSchema` rejects output
that has the wrong count, copied scalar kind, required property, error state, or
truncation. `PowerShellRuntime.Invoke` and `InvokeAsync` apply recipe timeouts
through the existing cancellation model. `PowerShellCommandPolicy` is an
application allowlist/size guardrail only; it is not a PowerShell sandbox and
cannot make approved arbitrary script source safe.

`PowerShellSnapshotReader.GetCompleteProperties` rejects truncated snapshots
before returning their copied property bag, and
`CreateDisplaySnapshot` returns copied text for each stream plus an explicit
completeness flag. `PowerShellSession.SetPropertyBag`,
`TryGetPropertyBag`, and `InvokeAndReadVariable` support value-only session
result DTO flows; they never retrieve a live `PSObject`.

`ParseScriptParameters` also projects bounded aliases, parameter-set name and
position/pipeline flags, and `ValidatePattern`, `ValidateRange`,
`ValidateLength`, and `ValidateCount` argument spelling. The combined output
is capped at 32 metadata records, so it fails rather than silently truncating.

Applications define their own capability schemas using lowercase,
namespace-qualified names such as `app.get-label`; applications must still
register their own handlers.
`PowerShellHostInteraction.ParseProgressUpdate` validates an explicit copied
`host.report-progress` property bag (including ID and range fields), rather
than inferring progress from stream display text.

`PowerShellStagedIntentCoordinator` builds a generic four-operation lifecycle
on the same bounded capability dispatcher; it does not introduce another
callback or object bridge. A `PowerShellStagedIntentDefinition` has a canonical
operation name, copied property-bag schema, application handler, and stage
deadline. It exposes `<operation>.stage`, `.validate`, `.commit`, and `.abort`.
Stage accepts `{ stageId, intent }`; the caller-supplied `stageId` is opaque,
bounded, and unique while retained. The other operations accept that identifier.
Each response has copied `operation`, `status`, `stageId`, `expiresAt`, and
`message` fields. Statuses explicitly distinguish staged, validated, committed,
aborted, rejected, unknown, expired, terminal, cancelled, and busy states.

The coordinator validates the envelope, schema, and bounds before the
application handler runs; it rejects duplicate, expired, unknown, and
terminal stages, retains at most 64 active stages, and clears stages on their
deadline, cancellation, or disposal. Each retained stage has at most one
terminal transition: commit or abort; expiry, cancellation, and disposal abort
the coordinator's retained state. After that state is released, the coordinator
best-effort delivers `Abort` to the application handler so it can release its
own retained data. This notification is outside the coordinator lock, ignores
handler failures, carries no rollback guarantee, and requires an idempotent
handler. A `committed` status means only that the
host accepted the intent. It is not a cross-resource transaction, has no
rollback guarantee, and does not prove a side effect occurred. Applications
own authorization, review UI, persistence, actual effects, and compensating
action. Do not put secrets in staged `PowerShellValue` data.

Raw ABI releases consume their handles, so a repeated raw release or later use
returns `InvalidHandle`. Public `Dispose` methods are idempotent; their
`SafeHandle` leases defer native release until any in-flight facade call exits.

The facade never accepts `PSHost`, delegates/callbacks, generic CLR values, or
live runspace/PowerShell objects. `PowerShellSecretTransfer.Policy` remains
`Rejected` for legacy implicit-transfer callers. Separately, a
`PowerShellSecret` lease may be bound as a payload-local
`SecureString`, and `PowerShellCredential` constructs a payload-local
`PSCredential`. A secret-bound pipeline must use `InvokeWithSecretBindings()`
or `InvokeSecretResult(...)`; normal snapshots, paging, async/live operations,
and diagnostics reject it. The only return shapes are no result, one
`SecureString`, or one `PSCredential`, exposed through a disposable redacted
lease. The payload disposes its copied secure strings when the builder clears,
releases, or completes a secret invocation; the public lease zeroes its managed
character buffer on disposal.

This is an accidental-disclosure boundary, not a sandbox for malicious scripts:
a script that receives a credential can deliberately disclose it. Do not place
secrets in `PowerShellValue`, session variables, bridge/DBC frames, result
snapshots, or serializers. There is no prompt or callback channel. The generic
`PowerShellRemoteSession` keeps an optional `PSSession` only inside a supplied
persistent local session, exposes fixed copied metadata and copied invocation
results, and removes it deterministically. It does not transfer remote object
identity, arbitrary `PSObject` graphs, transports, or local bridge objects.
Actual remoting and module availability remain the installed payload's
responsibility.

`PowerShellModuleImport` provides bounded name-or-path, required-version, and
finite import-option declarations for persistent sessions. It does not install
or resolve modules outside the configured approved roots. Use
`PowerShellSessionConfiguration.CreateWithModuleImports(...)` to construct a
configuration that includes these declarations without changing the established
constructor contract.

The package smoke exercises the local secret and declared-module paths. Set
`PWSH_FFI_REMOTE_COMPUTER` only in a controlled test environment to additionally
exercise `PowerShellRemoteSession`; it uses the installed payload's remoting
configuration and default authentication, and is not required for ordinary
local package validation.

For an additive one-shot administrative pilot, build a command with
`AddCommand("Restart-Computer")`, use `AddParameter("WhatIf")` for tests, and
call `InvokeAsync` with a `CancellationTokenSource.CancelAfter` timeout.
Inspect `PowerShellInvocationResult`/`PowerShellInvocationException` stream
snapshots; do not use `InvokeText()` as a success-only path. Actual shutdown or
restart execution must be application-gated and is never exercised by package
tests.

## Duplex Broker Channel

`PowerShellBrokerChannel` is an opt-in, strictly dispatch-only request/reply and
one-way-event primitive. It lets a running pipeline ask the application for work
**without running application code on the pipeline thread**. It is a separate
facility from `PowerShellCapabilitySet`, whose direct-callback behaviour is
unchanged.

**A builder with a broker attached must be invoked asynchronously.** `Invoke()`
and `InvokeText()` reject it with
`PowerShellFfiStatus.UnsupportedCapability`. This is a liveness precondition,
not a preference: a synchronous invocation from a UI thread whose dispatcher
also services the pump would deadlock without any FFI call occurring, so no
guard could catch it.

```csharp
using PowerShellBrokerChannel channel = runtime.CreateBrokerChannel(
    new PowerShellBrokerChannelOptions(
        maximumInflightFrames: 32,
        maximumBodyBytes: 64 * 1024,
        defaultDeadline: TimeSpan.FromSeconds(30)));

// One dedicated pump thread. It must never do application work inline.
using var pumpReady = new ManualResetEventSlim();
var pump = new Thread(() =>
{
    // Thread.Start only schedules work. This zero-time wait registers the
    // consumer before the payload can issue its first request.
    if (!channel.TryReceive(TimeSpan.Zero, out _))
    {
        pumpReady.Set();
        return;
    }

    pumpReady.Set();
    while (channel.TryReceive(TimeSpan.FromMilliseconds(250), out PowerShellBrokerRequest? request))
    {
        if (request is not null)
        {
            // 'request' is fully copied; the delivery handle is already gone.
            dispatcher.Post(() => HandleAsync(channel, request));
        }
    }
});
pump.IsBackground = true;
pump.Start();
if (!pumpReady.Wait(TimeSpan.FromSeconds(5)))
{
    throw new TimeoutException("The broker pump did not attach.");
}

using PowerShell command = session.CreatePowerShell()
    .AddScript("$DpsBroker.Request(1, [byte[]]@(1,2,3))")
    .WithBroker(channel);
PowerShellInvocationResult result = await command.InvokeAsync(cancellationToken);
```

The handler replies later, from any thread, using only the correlation ID:

```csharp
static async Task HandleAsync(PowerShellBrokerChannel channel, PowerShellBrokerRequest request)
{
    try
    {
        byte[] reply = await ComputeAsync(request.Body).ConfigureAwait(false);
        if (!channel.TryReply(request.CorrelationId, reply))
        {
            // The frame was cancelled, timed out, or the channel closed.
            // Nobody is waiting any more; stop doing the work.
        }
    }
    catch (Exception exception)
    {
        channel.TryReplyError(request.CorrelationId, code: 1, exception.Message);
    }
}
```

### Rules the facade enforces

- **The delivery handle never escapes.** `TryReceive` performs wait, inspect,
  copy, and release inside one call on one thread, then hands back an immutable
  `PowerShellBrokerRequest`. No native pointer, buffer lifetime, or Rust-owned
  memory reaches consumer code, and there is no finalizer that could release on
  the wrong thread.
- **Releasing is not abandoning.** The request stays outstanding after
  `TryReceive` returns; only `TryReply`, `TryReplyError`, `Cancel`, its
  deadline, or channel close makes it terminal.
- **Replies are correlation-scoped.** They work from any thread until the frame
  is terminal. A duplicate or late reply returns `false` and can never
  resurrect a frame or reach a different one.
- **Dispatch-only.** A pump must copy and hand off. If consumer code holds a
  delivery handle at the ABI level and makes any other FFI call, that call
  fails with `PowerShellFfiStatus.BrokerDispatchViolation`. The facade's
  composed `TryReceive` makes this unreachable through normal use; it remains a
  backstop for direct native consumers.
- **Closing wakes everything.** `Dispose()` wakes every waiter and fails every
  outstanding request deterministically.
- **Events never block the pipeline.** `$DpsBroker.Post` is one-way; under
  pressure the channel evicts older one-way frames and reports the loss in the
  next request's `DroppedBefore`.
- **One active mutating frame per ordering key**, so side effects for a key
  cannot interleave.

There is no consumer-side cancellation notification in this version. A handler
discovers that its work is unwanted when `TryReply` returns `false`.

### Explicit non-goals

The broker carries bounded opaque byte frames and fixed-width metadata only. It
is not an object bridge and never becomes one by configuration: no dynamic or
reflective member access, no JSON or self-describing wire format, no `PSObject`
or SMA type, no CLR object identity, no delegates or arbitrary script callbacks,
no credential or `SecureString` material, and no bridge object injected into a
remote runspace. There is no synchronous nested broker path in this version, and
adding one is not a configuration option.
