# MultiPwsh-managed WSMan diagnostics

The NativeAOT facade describes the availability of the single MultiPwsh-managed
WSMan provider, `devolutions.wsman`, in the managed payload bindings. Provider
availability is separate from the FFI feature mask: the feature mask reports
that the bindings can expose diagnostics, not that a transport is implemented.
The current implementation always reports the provider as unavailable with
zero capabilities. This diagnostic surface provides no MultiPwsh-managed WSMan
connection/pool options, remote-pool preflight, or host-stream connector API;
those contracts are deferred until an operational transport is implemented.

Vanilla PowerShell remains a supported payload. It does not contain the
Devolutions client-transport provider registry, so
`PowerShellRuntime.Activate` succeeds and reports `devolutions.wsman` as
unavailable with `PayloadHooksMissing`. Local invocation and PowerShell's
built-in remoting remain available.

A future API selecting the Devolutions managed WSMan provider must reject
unavailability rather than fall back to built-in WSMan. Gateway routing, SSPI
selection, KDC proxying, and certificate behavior would otherwise change
silently.

## PowerShell integration

The Devolutions PowerShell source already exposes the integration surface:

- `IClientRemotingTransportProvider`
- `ClientRemotingTransportProviderOptions`
- `ClientRemotingTransportProviderRegistry.Register`
- `ClientRemotingTransportCreationContext`
- the protected transport-manager extension surface
- `RunspaceFactory.CreateRunspacePool` routing through the registry

MultiPwsh probes that exact public shape without loading an extra provider
assembly. When the shape is absent, the selected payload is treated as vanilla.
When the shape is present but MultiPwsh does not contain an operational managed
WSMan transport, diagnostics report `TransportImplementationMissing`; no
capability is advertised.

The eventual implementation belongs in
`Devolutions.PowerShell.SDK.Bindings`, the managed component already loaded into
the selected PowerShell runtime. It must register an
`IClientRemotingTransportProvider` against the same
`System.Management.Automation` assembly identity used by the runspace objects,
set `ReplaceBuiltInTransports` when intercepting `WSManConnectionInfo`, retain
the returned `IDisposable` registration token for the remoting lifecycle, and
dispose it during runtime shutdown.

The current generic bindings target stock PowerShell 7.4 on `net8.0`; they
cannot directly implement an interface that exists only in the Devolutions
fork. Operational support therefore requires a Devolutions binding flavor
compiled against the patched SMA contract and selected only for a compatible
payload. The generic binding remains the vanilla path. Both flavors continue
to expose the same required append-only V1 native table, so no SMA type,
managed object, delegate, or `Stream` crosses NativeAOT.

No PowerShell distribution manifest, adapter DLL, or provider-specific
packaging is required. A future PowerShell source change is justified only if
a concrete MultiPwsh implementation cannot compile or operate through the
already-public registry and protected transport surface.

## Diagnostics

`PowerShellRuntime.Diagnostics.ManagedWsManProvider` returns copied, non-secret
metadata. The current implementation reports:

- `PayloadHooksMissing` when the loaded SMA lacks the expected public hooks.
- `TransportImplementationMissing` when the hooks are present but MultiPwsh
  lacks an operational transport.

The diagnostic contract also reserves `RegistrationFailed` and
`UnsupportedPlatform` for a future implementation. An unavailable provider
reports zero capabilities. A future available provider must report
`CustomClientTransport` and `RemoteRunspacePool`; optional capabilities include
`HostStreamRelay`, authentication mechanisms, and KDC proxy support. These
reserved values do not imply current transport support.

Reading these diagnostics performs no credential transfer, provider
registration, or network access. No managed WSMan invocation or gateway relay
is implemented by this branch.

Provider diagnostics describe capability only. They do not attest payload
integrity or provenance, and applications remain responsible for selecting and
validating their PowerShell payload.
