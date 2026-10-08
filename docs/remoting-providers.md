# MultiPwsh-managed remoting providers

The NativeAOT facade can describe optional remoting providers implemented by
MultiPwsh's managed payload bindings. Provider availability is separate from
the FFI feature mask: the feature mask reports that the bindings can describe
providers, while runtime diagnostics report whether the selected provider can
actually create a remote runspace pool.

Vanilla PowerShell remains a supported payload. It does not contain the
Devolutions client-transport provider registry, so
`PowerShellRuntime.Activate` succeeds and reports `devolutions.wsman` as
unavailable with `PayloadHooksMissing`. Local invocation and PowerShell's
built-in remoting remain available.

Selecting the Devolutions managed WSMan provider never falls back to built-in
WSMan. Gateway routing, SSPI selection, KDC proxying, and certificate behavior
would otherwise change silently.

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
metadata and reports one of these unavailable reasons:

- `PayloadHooksMissing`
- `TransportImplementationMissing`
- `RegistrationFailed`
- `UnsupportedPlatform`

Unavailable providers report no version or capabilities. An available provider
must report `CustomClientTransport` and `RemoteRunspacePool`; optional
capabilities include `HostStreamRelay`, authentication mechanisms, and KDC
proxy support.

`PowerShellRuntime.ValidateRemoteSessionPool` performs no credential transfer,
connector invocation, or network access. It rejects an unavailable provider or
missing transport, pool, authentication, KDC-proxy, or stream-relay capability
before a remote pool can be created.

`DevolutionsManagedWsManConnectionOptions` uses one absolute HTTP or HTTPS
`EndpointUri`; its path becomes the WSMan application name. The copied options
also include the shell URI, `Negotiate`/`Kerberos`/`Ntlm` authentication,
runspace bounds, open/operation/cancel timeouts, maximum envelope size,
connection bound, certificate policy, SSPI provider, domain, KDC proxy name,
and optional SPN. The host-owned connector remains separate from these copied
options, and its returned `Stream` never crosses the native boundary.

Provider diagnostics describe capability only. They do not attest payload
integrity or provenance, and applications remain responsible for selecting and
validating their PowerShell payload.
