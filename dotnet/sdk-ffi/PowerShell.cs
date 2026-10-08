using System.Buffers.Binary;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Devolutions.PowerShell.Ffi.LiveObjects;

namespace Devolutions.PowerShell.Ffi;

public sealed unsafe class PowerShell : IDisposable
{
    private const uint RequiredAbiVersion = 2;
    private const int Success = (int)PowerShellFfiStatus.Success;
    private const int BufferTooSmall = (int)PowerShellFfiStatus.BufferTooSmall;
    private const ulong StructuredInvocationErrorsFeature = 1UL << 0;
    private const ulong PerCallDiagnosticsFeature = 1UL << 1;
    private const ulong Utf8SpansFeature = 1UL << 2;
    private const ulong ImmutableResultsFeature = 1UL << 3;
    private const ulong TaggedValuesFeature = 1UL << 4;
    private const ulong CommandOptionsFeature = 1UL << 5;
    private const ulong BoundedInputFeature = 1UL << 6;
    private const ulong InvocationMetadataFeature = 1UL << 7;
    private const ulong AsyncOperationsFeature = 1UL << 8;
    private const ulong SessionsFeature = 1UL << 10;
    private const ulong SessionPollingFeature = 1UL << 11;
    private const ulong SessionPoolRejectionFeature = 1UL << 12;
    private const ulong SnapshotProjectionsFeature = 1UL << 13;
    private const ulong SessionConfigurationFeature = 1UL << 14;
    private const ulong SessionVariablesFeature = 1UL << 15;
    private const ulong CapabilityRpcFeature = 1UL << 16;
    private const ulong DuplexBrokerChannelFeature = 1UL << 25;
    private const ulong GeneratedBridgeAttachmentFeature = 1UL << 26;
    private const ulong BrokerTerminalObservationFeature = 1UL << 27;
    private const ulong ReliableBridgeEventsFeature = 1UL << 28;
    private const ulong ObservedPresentationFeature = 1UL << 29;
    private const ulong SecretAdaptersFeature = 1UL << 30;
    private const ulong CredentialResultFeature = 1UL << 31;
    private const ulong RemoteProviderDiagnosticsFeature = 1UL << 32;
    private const ulong LiveObjectProbeFeature = 1UL << 17;
    private const ulong LiveSessionObjectProbeFeature = 1UL << 18;
    private const ulong LiveObjectContractsFeature = 1UL << 19;
    private const ulong LiveStreamPollingFeature = 1UL << 20;
    private const ulong TypedResultPagingFeature = 1UL << 21;
    private const ulong ObservedInvocationFeature = 1UL << 22;
    private const ulong SessionPreflightFeature = 1UL << 23;
    private const ulong RuntimeDiagnosticsFeature = 1UL << 24;
    private const ulong RequiredFeatures =
        StructuredInvocationErrorsFeature | PerCallDiagnosticsFeature | Utf8SpansFeature |
        ImmutableResultsFeature | TaggedValuesFeature | CommandOptionsFeature | BoundedInputFeature |
        InvocationMetadataFeature | AsyncOperationsFeature | SessionsFeature |
        SessionPollingFeature | SessionPoolRejectionFeature | SnapshotProjectionsFeature |
        SessionConfigurationFeature | SessionVariablesFeature | CapabilityRpcFeature |
        LiveObjectProbeFeature | LiveSessionObjectProbeFeature | LiveObjectContractsFeature |
        LiveStreamPollingFeature | TypedResultPagingFeature | ObservedInvocationFeature |
        SessionPreflightFeature | RuntimeDiagnosticsFeature | SecretAdaptersFeature |
        CredentialResultFeature | RemoteProviderDiagnosticsFeature;
    private const uint ResultTerminatingFailure = 1;
    private const uint ResultSequenceTruncated = 1 << 1;
    private const uint StreamTruncated = 1;
    private const uint RecordFieldsTruncated = 1;
    private const uint RecordScalarValuePresent = 1 << 1;
    private const uint RecordPropertyBagPresent = 1 << 2;
    private const uint RecordPropertyBagTruncated = 1 << 3;
    private const uint RecordTypeNamesTruncated = 1 << 4;
    private const uint RecordErrorTargetValuePresent = 1 << 5;
    private const uint StreamCount = 7;
    private const uint RecordFieldCount = 21;
    private const uint MaxRecordsPerStream = 32;
    private const uint MaxSequenceRecords = MaxRecordsPerStream * StreamCount;
    private const nuint MaxResultFieldUtf8Bytes = 16 * 1024;
    private const nuint MaxResultValueBytes = 16 * 1024;
    private const nuint MaxPayloadPathUtf8Bytes = 32 * 1024;
    private const int MaxCredentialTextUtf8Bytes = 4 * 1024;
    private const int MaxCredentialMessages = 16;
    private const int MaxCredentialMessageBytes = 4 * 1024;
    private const int MaxCredentialMessageBufferBytes =
        sizeof(uint) + (MaxCredentialMessages * (sizeof(uint) + MaxCredentialMessageBytes));
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);

    private readonly PowerShellHandle handle;

    internal PowerShell(PowerShellHandle handle)
    {
        this.handle = handle;
    }

    public static uint AbiVersion => GetAbiInfo().AbiVersion;

    public static ulong FeatureFlags => GetAbiInfo().FeatureFlags;

    public static void Initialize()
    {
        EnsureSupportedAbi();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        int status = NativeMethods.InitializeFromPath(&result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
    }

    public static void Initialize(string payloadDirectory)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(payloadDirectory);
        EnsureSupportedAbi();
        byte[] payloadBytes = EncodeUtf8(payloadDirectory);
        fixed (byte* payloadPointer = payloadBytes)
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = NativeMethods.Initialize(
                new NativeUtf8Span
                {
                    Data = payloadBytes.Length == 0 ? null : payloadPointer,
                    Length = (nuint)payloadBytes.Length,
                },
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }
    }

    public static void Initialize(IReadOnlyList<PowerShellLiveObjectContractPack> contractPacks)
    {
        ArgumentNullException.ThrowIfNull(contractPacks);
        if (contractPacks.Count == 0)
        {
            Initialize();
            return;
        }

        EnsureLiveObjectContractsSupported();
        var assemblyPathHandles = new GCHandle[contractPacks.Count];
        var typeNameHandles = new GCHandle[contractPacks.Count];
        var nativePacks = new NativeLiveObjectContractPack[contractPacks.Count];
        try
        {
            for (int index = 0; index < contractPacks.Count; index++)
            {
                PowerShellLiveObjectContractPack pack = contractPacks[index]
                    ?? throw new ArgumentException("Live object contract packs cannot contain null.", nameof(contractPacks));
                if (!File.Exists(pack.PayloadAdapterAssemblyPath))
                {
                    throw new FileNotFoundException(
                        "The live object contract pack payload adapter assembly does not exist.",
                        pack.PayloadAdapterAssemblyPath);
                }

                byte[] assemblyPath = EncodeUtf8(pack.PayloadAdapterAssemblyPath);
                byte[] typeName = EncodeUtf8(pack.PayloadAdapterTypeName);
                assemblyPathHandles[index] = GCHandle.Alloc(assemblyPath, GCHandleType.Pinned);
                typeNameHandles[index] = GCHandle.Alloc(typeName, GCHandleType.Pinned);
                nativePacks[index] = new NativeLiveObjectContractPack
                {
                    Size = checked((uint)sizeof(NativeLiveObjectContractPack)),
                    PayloadAdapterAssemblyPath = new NativeUtf8Span
                    {
                        Data = (byte*)assemblyPathHandles[index].AddrOfPinnedObject(),
                        Length = checked((nuint)assemblyPath.Length),
                    },
                    PayloadAdapterTypeName = new NativeUtf8Span
                    {
                        Data = (byte*)typeNameHandles[index].AddrOfPinnedObject(),
                        Length = checked((nuint)typeName.Length),
                    },
                };
            }

            fixed (NativeLiveObjectContractPack* nativePacksPointer = nativePacks)
            {
                byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
                NativeCallResult result = NativeCall.CreateResult(diagnostic);
                int status = NativeMethods.InitializeFromPathWithContractPacks(
                    nativePacksPointer,
                    checked((nuint)nativePacks.Length),
                    &result);
                NativeCall.ThrowIfFailed(status, result, diagnostic);
            }
        }
        finally
        {
            foreach (GCHandle handle in assemblyPathHandles)
            {
                if (handle.IsAllocated)
                {
                    handle.Free();
                }
            }

            foreach (GCHandle handle in typeNameHandles)
            {
                if (handle.IsAllocated)
                {
                    handle.Free();
                }
            }
        }
    }

    public static void Initialize(
        string payloadDirectory,
        IReadOnlyList<PowerShellLiveObjectContractPack> contractPacks)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(payloadDirectory);
        ArgumentNullException.ThrowIfNull(contractPacks);
        if (contractPacks.Count == 0)
        {
            Initialize(payloadDirectory);
            return;
        }

        EnsureLiveObjectContractsSupported();
        byte[] payloadBytes = EncodeUtf8(payloadDirectory);
        var assemblyPathHandles = new GCHandle[contractPacks.Count];
        var typeNameHandles = new GCHandle[contractPacks.Count];
        var nativePacks = new NativeLiveObjectContractPack[contractPacks.Count];
        try
        {
            for (int index = 0; index < contractPacks.Count; index++)
            {
                PowerShellLiveObjectContractPack pack = contractPacks[index]
                    ?? throw new ArgumentException("Live object contract packs cannot contain null.", nameof(contractPacks));
                if (!File.Exists(pack.PayloadAdapterAssemblyPath))
                {
                    throw new FileNotFoundException(
                        "The live object contract pack payload adapter assembly does not exist.",
                        pack.PayloadAdapterAssemblyPath);
                }

                byte[] assemblyPath = EncodeUtf8(pack.PayloadAdapterAssemblyPath);
                byte[] typeName = EncodeUtf8(pack.PayloadAdapterTypeName);
                assemblyPathHandles[index] = GCHandle.Alloc(assemblyPath, GCHandleType.Pinned);
                typeNameHandles[index] = GCHandle.Alloc(typeName, GCHandleType.Pinned);
                nativePacks[index] = new NativeLiveObjectContractPack
                {
                    Size = checked((uint)sizeof(NativeLiveObjectContractPack)),
                    PayloadAdapterAssemblyPath = new NativeUtf8Span
                    {
                        Data = (byte*)assemblyPathHandles[index].AddrOfPinnedObject(),
                        Length = checked((nuint)assemblyPath.Length),
                    },
                    PayloadAdapterTypeName = new NativeUtf8Span
                    {
                        Data = (byte*)typeNameHandles[index].AddrOfPinnedObject(),
                        Length = checked((nuint)typeName.Length),
                    },
                };
            }

            fixed (byte* payloadPointer = payloadBytes)
            fixed (NativeLiveObjectContractPack* nativePacksPointer = nativePacks)
            {
                byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
                NativeCallResult result = NativeCall.CreateResult(diagnostic);
                int status = NativeMethods.InitializeWithContractPacks(
                    new NativeUtf8Span
                    {
                        Data = payloadPointer,
                        Length = checked((nuint)payloadBytes.Length),
                    },
                    nativePacksPointer,
                    checked((nuint)nativePacks.Length),
                    &result);
                NativeCall.ThrowIfFailed(status, result, diagnostic);
            }
        }
        finally
        {
            foreach (GCHandle handle in assemblyPathHandles)
            {
                if (handle.IsAllocated)
                {
                    handle.Free();
                }
            }

            foreach (GCHandle handle in typeNameHandles)
            {
                if (handle.IsAllocated)
                {
                    handle.Free();
                }
            }
        }
    }

    internal static string GetActivePayloadDirectory()
    {
        EnsureSupportedAbi();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        nuint requiredLength = 0;
        int status = NativeMethods.GetPayloadPath(null, 0, &requiredLength, &result);
        if (status != Success && status != BufferTooSmall)
        {
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        if (requiredLength > MaxPayloadPathUtf8Bytes)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded payload path.");
        }

        byte[] payloadPath = new byte[checked((int)requiredLength)];
        fixed (byte* payloadPathPointer = payloadPath)
        {
            result = NativeCall.CreateResult(diagnostic);
            status = NativeMethods.GetPayloadPath(
                payloadPathPointer,
                (nuint)payloadPath.Length,
                &requiredLength,
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        if (requiredLength != (nuint)payloadPath.Length)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI changed the active payload path while it was being read.");
        }

        return Encoding.UTF8.GetString(payloadPath);
    }

    public static PowerShell Create()
    {
        EnsureSupportedAbi();
        ulong nativeHandle = 0;
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        int status = NativeMethods.Create(&nativeHandle, &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
        return CreateFromNative(nativeHandle);
    }

    public PowerShell AddCommand(string command)
    {
        return AddCommand(command, false);
    }

    public PowerShell AddCommand(string command, bool useLocalScope)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(command);
        InvokeUtf8ForHandle(
            command,
            (nativeHandle, value, result) => NativeMethods.AddCommandWithLocalScope(
                nativeHandle,
                value,
                useLocalScope ? 1u : 0u,
                result));
        return this;
    }

    public PowerShell AddScript(string script)
    {
        return AddScript(script, false);
    }

    public PowerShell AddScript(string script, bool useLocalScope)
    {
        ArgumentNullException.ThrowIfNull(script);
        InvokeUtf8ForHandle(
            script,
            (nativeHandle, value, result) => NativeMethods.AddScriptWithLocalScope(
                nativeHandle,
                value,
                useLocalScope ? 1u : 0u,
                result));
        return this;
    }

    public PowerShell AddArgument(string argument)
    {
        ArgumentNullException.ThrowIfNull(argument);
        InvokeUtf8ForHandle(argument, static (nativeHandle, value, result) => NativeMethods.AddArgument(nativeHandle, value, result));
        return this;
    }

    public PowerShell AddArgument(PowerShellValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        InvokeValueForHandle(value, static (nativeHandle, nativeValue, result) => NativeMethods.AddArgumentValue(nativeHandle, nativeValue, result));
        return this;
    }

    public PowerShell AddArgument(PowerShellLiveObjectProbe value)
    {
        ArgumentNullException.ThrowIfNull(value);
        value.AddTo(this);
        return this;
    }

    public PowerShell AddArguments(IEnumerable<PowerShellValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        foreach (PowerShellValue value in values)
        {
            AddArgument(value);
        }

        return this;
    }

    public PowerShell AddParameter(string name, string value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        ArgumentNullException.ThrowIfNull(value);

        byte[] nameBytes = EncodeUtf8(name);
        byte[] valueBytes = EncodeUtf8(value);
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        fixed (byte* namePointer = nameBytes)
        fixed (byte* valuePointer = valueBytes)
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = NativeMethods.AddParameterString(
                lease.Value,
                new NativeUtf8Span { Data = namePointer, Length = (nuint)nameBytes.Length },
                new NativeUtf8Span { Data = valuePointer, Length = (nuint)valueBytes.Length },
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        return this;
    }

    public PowerShell AddParameter(string name, long value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        InvokeUtf8ForHandle(
            name,
            (nativeHandle, valueSpan, result) => NativeMethods.AddParameterInt64(nativeHandle, valueSpan, value, result));
        return this;
    }

    public PowerShell AddParameter(string name)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        InvokeUtf8ForHandle(name, static (nativeHandle, value, result) => NativeMethods.AddParameterSwitch(nativeHandle, value, result));
        return this;
    }

    public PowerShell AddParameter(string name, PowerShellValue value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        ArgumentNullException.ThrowIfNull(value);

        byte[] nameBytes = EncodeUtf8(name);
        byte[] payload = value.Payload;
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        fixed (byte* namePointer = nameBytes)
        fixed (byte* payloadPointer = payload)
        {
            NativeDataValue nativeValue = CreateNativeValue(value, payloadPointer);
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = NativeMethods.AddParameterValue(
                lease.Value,
                new NativeUtf8Span { Data = namePointer, Length = (nuint)nameBytes.Length },
                &nativeValue,
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        return this;
    }

    /// <summary>
    /// Binds a payload-side <see cref="System.Security.SecureString"/> through
    /// the explicit secret lease boundary.
    /// </summary>
    public PowerShell AddParameter(string name, PowerShellSecret value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        ArgumentNullException.ThrowIfNull(value);
        AddSecretParameter(name, SecretSecureStringKind, value, userName: null);
        return this;
    }

    /// <summary>
    /// Binds a payload-side <c>PSCredential</c> through the explicit secret
    /// lease boundary.
    /// </summary>
    public PowerShell AddParameter(string name, PowerShellCredential value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        ArgumentNullException.ThrowIfNull(value);
        AddSecretParameter(name, SecretCredentialKind, value.Password, value.UserName);
        return this;
    }

    public PowerShell AddParameters(IEnumerable<KeyValuePair<string, PowerShellValue>> parameters)
    {
        ArgumentNullException.ThrowIfNull(parameters);
        foreach (KeyValuePair<string, PowerShellValue> parameter in parameters)
        {
            AddParameter(parameter.Key, parameter.Value);
        }

        return this;
    }

    public PowerShell AddStatement()
    {
        InvokeForHandle(static (nativeHandle, result) => NativeMethods.AddStatement(nativeHandle, result));
        return this;
    }

    public PowerShell WithCapabilities(PowerShellCapabilitySet capabilities)
    {
        ArgumentNullException.ThrowIfNull(capabilities);
        EnsureCapabilityRpcSupported();
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        capabilities.AttachTo(lease.Value);
        return this;
    }

    /// <summary>
    /// Attaches a duplex broker channel to this builder for one invocation.
    /// A builder with a broker attached must be invoked asynchronously; the
    /// synchronous paths reject it with
    /// <see cref="PowerShellFfiStatus.UnsupportedCapability"/>.
    /// </summary>
    public PowerShell WithBroker(PowerShellBrokerChannel broker)
    {
        ArgumentNullException.ThrowIfNull(broker);
        EnsureDuplexBrokerChannelSupported();
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        unsafe
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = NativeMethods.SetBroker(lease.Value, broker.Handle, &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }

        return this;
    }

    /// <summary>
    /// Attaches one generated, bounded bridge proxy to this builder for its next
    /// asynchronous invocation. The payload exposes it only through
    /// <paramref name="variableName"/>; raw <c>$DpsBroker</c> is not injected.
    /// </summary>
    public PowerShell WithBridge(PowerShellBridgeBinding binding, string variableName)
    {
        ArgumentNullException.ThrowIfNull(binding);
        ArgumentException.ThrowIfNullOrEmpty(variableName);
        EnsureGeneratedBridgeAttachmentSupported();
        if (!IsBridgeVariableName(variableName))
        {
            throw new ArgumentException("The bridge variable name must be an ASCII PowerShell identifier.", nameof(variableName));
        }

        IPowerShellBridgeDispatcher dispatcher = binding.GetDispatcher();
        Guid interfaceId = dispatcher.ContractInterfaceId;
        Span<byte> interfaceBytes = stackalloc byte[16];
        interfaceId.TryWriteBytes(interfaceBytes);
        ulong interfaceIdLow = BitConverter.ToUInt64(interfaceBytes);
        ulong interfaceIdHigh = BitConverter.ToUInt64(interfaceBytes[8..]);
        byte[] encodedName = EncodeUtf8(variableName);
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        unsafe
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            fixed (byte* name = encodedName)
            {
                NativeUtf8Span nativeName = new()
                {
                    Data = name,
                    Length = (nuint)encodedName.Length,
                };
                int status = NativeMethods.SetBridge(
                    lease.Value,
                    binding.Channel.Broker.Handle,
                    binding.BindingId,
                    interfaceIdLow,
                    interfaceIdHigh,
                    dispatcher.ContractMajorVersion,
                    dispatcher.ContractMinorVersion,
                    checked((uint)dispatcher.MaximumRequestBytes),
                    checked((uint)dispatcher.MaximumReplyBytes),
                    nativeName,
                    &result);
                NativeCall.ThrowIfFailed(status, result, diagnostic);
            }
        }

        return this;
    }

    public PowerShell Clear()
    {
        InvokeForHandle(static (nativeHandle, result) => NativeMethods.Clear(nativeHandle, result));
        return this;
    }

    public PowerShell AddInput(PowerShellValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        InvokeValueForHandle(value, static (nativeHandle, nativeValue, result) => NativeMethods.AddInputValue(nativeHandle, nativeValue, result));
        return this;
    }

    public PowerShell AddInputs(IEnumerable<PowerShellValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        foreach (PowerShellValue value in values)
        {
            AddInput(value);
        }

        return this;
    }

    public PowerShell CompleteInput()
    {
        InvokeForHandle(static (nativeHandle, result) => NativeMethods.CompleteInput(nativeHandle, result));
        return this;
    }

    public PowerShell ResetInput()
    {
        InvokeForHandle(static (nativeHandle, result) => NativeMethods.ResetInput(nativeHandle, result));
        return this;
    }

    public PowerShellInvocationResult Invoke()
    {
        return InvokeWithDiagnostics();
    }

    /// <summary>
    /// Invokes a secret-bound pipeline without returning normal output,
    /// snapshots, or diagnostics.
    /// </summary>
    public PowerShellSecretResult InvokeWithSecretBindings()
    {
        return InvokeSecretResult(PowerShellSecretResultKind.None);
    }

    /// <summary>
    /// Invokes a secret-bound pipeline and returns only one explicitly approved
    /// <see cref="System.Security.SecureString"/> or <c>PSCredential</c> shape.
    /// </summary>
    public PowerShellSecretResult InvokeSecretResult(PowerShellSecretResultKind expectedKind)
    {
        if (!Enum.IsDefined(expectedKind))
        {
            throw new ArgumentOutOfRangeException(nameof(expectedKind));
        }

        EnsureSecretAdaptersSupported();
        byte[] userName = new byte[1_024];
        char[] secret = new char[PowerShellSecret.MaximumLength];
        try
        {
            using PowerShellHandle.HandleLease lease = handle.Borrow();
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            nuint userNameLength = 0;
            nuint secretLength = 0;
            fixed (byte* userNamePointer = userName)
            fixed (char* secretPointer = secret)
            {
                int status = NativeMethods.InvokeSecretResult(
                    lease.Value,
                    checked((uint)expectedKind),
                    userNamePointer,
                    (nuint)userName.Length,
                    &userNameLength,
                    secretPointer,
                    (nuint)secret.Length,
                    &secretLength,
                    &result);
                NativeCall.ThrowIfFailed(status, result, diagnostic);
            }

            if (expectedKind == PowerShellSecretResultKind.None)
            {
                if (userNameLength != 0 || secretLength != 0)
                {
                    throw new PowerShellFfiException(
                        PowerShellFfiStatus.ManagedFailure,
                        "Native PowerShell FFI returned an invalid empty secret result.");
                }

                return new PowerShellSecretResult(expectedKind, null, null);
            }

            if (secretLength is 0 or > PowerShellSecret.MaximumLength ||
                userNameLength > (nuint)userName.Length)
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned an invalid secret result length.");
            }

            char[] ownedSecret = secret[..checked((int)secretLength)];
            CryptographicOperations.ZeroMemory(MemoryMarshal.AsBytes(secret.AsSpan()));
            secret = Array.Empty<char>();
            PowerShellSecret secretLease = PowerShellSecret.TakeOwnership(ownedSecret);
            if (expectedKind == PowerShellSecretResultKind.SecureString)
            {
                if (userNameLength != 0)
                {
                    secretLease.Dispose();
                    throw new PowerShellFfiException(
                        PowerShellFfiStatus.ManagedFailure,
                        "Native PowerShell FFI returned a credential user name for a SecureString result.");
                }

                return new PowerShellSecretResult(expectedKind, secretLease, null);
            }

            string userNameText = Encoding.UTF8.GetString(userName, 0, checked((int)userNameLength));
            if (string.IsNullOrWhiteSpace(userNameText) || userNameText.IndexOf('\0') >= 0)
            {
                secretLease.Dispose();
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned an invalid credential user name.");
            }

            return new PowerShellSecretResult(
                expectedKind,
                null,
                new PowerShellCredential(userNameText, secretLease));
        }
        finally
        {
            CryptographicOperations.ZeroMemory(userName);
            CryptographicOperations.ZeroMemory(MemoryMarshal.AsBytes(secret.AsSpan()));
        }
    }

    /// <summary>
    /// Invokes a credential resolver pipeline. The script must assign the
    /// supported values to <c>$Result</c> and must not emit pipeline output.
    /// </summary>
    public PowerShellCredentialResult InvokeCredentialResult()
    {
        EnsureCredentialResultSupported();
        byte[] username = new byte[MaxCredentialTextUtf8Bytes];
        byte[] domain = new byte[MaxCredentialTextUtf8Bytes];
        byte[] outputMessages = new byte[MaxCredentialMessageBufferBytes];
        byte[] errorMessages = new byte[MaxCredentialMessageBufferBytes];
        byte[] logMessage = new byte[MaxCredentialTextUtf8Bytes];
        char[] password = new char[PowerShellSecret.MaximumLength];
        try
        {
            using PowerShellHandle.HandleLease lease = handle.Borrow();
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult callResult = NativeCall.CreateResult(diagnostic);
            fixed (byte* usernamePointer = username)
            fixed (byte* domainPointer = domain)
            fixed (byte* outputMessagesPointer = outputMessages)
            fixed (byte* errorMessagesPointer = errorMessages)
            fixed (byte* logMessagePointer = logMessage)
            fixed (char* passwordPointer = password)
            {
                var nativeResult = new NativeCredentialResult
                {
                    Size = checked((uint)sizeof(NativeCredentialResult)),
                    Username = usernamePointer,
                    UsernameCapacity = username.Length,
                    Domain = domainPointer,
                    DomainCapacity = domain.Length,
                    Password = passwordPointer,
                    PasswordCapacity = password.Length,
                    OutputMessages = outputMessagesPointer,
                    OutputMessagesCapacity = outputMessages.Length,
                    ErrorMessages = errorMessagesPointer,
                    ErrorMessagesCapacity = errorMessages.Length,
                    LogMessage = logMessagePointer,
                    LogMessageCapacity = logMessage.Length,
                };
                int status = NativeMethods.InvokeCredentialResult(lease.Value, &nativeResult, &callResult);
                NativeCall.ThrowIfFailed(status, callResult, diagnostic);

                if (nativeResult.IsCancelled > 1 ||
                    nativeResult.UsernameLength is < 0 or > MaxCredentialTextUtf8Bytes ||
                    nativeResult.DomainLength is < 0 or > MaxCredentialTextUtf8Bytes ||
                    nativeResult.LogMessageLength is < 0 or > MaxCredentialTextUtf8Bytes ||
                    nativeResult.PasswordLength is < 0 or > PowerShellSecret.MaximumLength ||
                    nativeResult.OutputMessagesLength is < 0 or > MaxCredentialMessageBufferBytes ||
                    nativeResult.ErrorMessagesLength is < 0 or > MaxCredentialMessageBufferBytes)
                {
                    throw new PowerShellFfiException(
                        PowerShellFfiStatus.ManagedFailure,
                        "Native PowerShell FFI returned an invalid credential result.");
                }

                PowerShellSecret? secret = null;
                if (nativeResult.PasswordLength != 0)
                {
                    if (nativeResult.IsCancelled != 0)
                    {
                        throw new PowerShellFfiException(
                            PowerShellFfiStatus.ManagedFailure,
                            "Native PowerShell FFI returned a password for a cancelled credential result.");
                    }

                    char[] ownedSecret = password[..nativeResult.PasswordLength];
                    CryptographicOperations.ZeroMemory(MemoryMarshal.AsBytes(password.AsSpan()));
                    password = Array.Empty<char>();
                    secret = PowerShellSecret.TakeOwnership(ownedSecret);
                }

                try
                {
                    return new PowerShellCredentialResult(
                        DecodeCredentialText(username.AsSpan(0, nativeResult.UsernameLength)),
                        DecodeCredentialText(domain.AsSpan(0, nativeResult.DomainLength)),
                        secret,
                        DecodeCredentialMessages(outputMessages, nativeResult.OutputMessagesLength),
                        DecodeCredentialMessages(errorMessages, nativeResult.ErrorMessagesLength),
                        DecodeCredentialText(logMessage.AsSpan(0, nativeResult.LogMessageLength)),
                        nativeResult.IsCancelled != 0);
                }
                catch
                {
                    secret?.Dispose();
                    throw;
                }
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(username);
            CryptographicOperations.ZeroMemory(domain);
            CryptographicOperations.ZeroMemory(outputMessages);
            CryptographicOperations.ZeroMemory(errorMessages);
            CryptographicOperations.ZeroMemory(logMessage);
            CryptographicOperations.ZeroMemory(MemoryMarshal.AsBytes(password.AsSpan()));
        }
    }

    private static string? DecodeCredentialText(ReadOnlySpan<byte> buffer)
    {
        if (buffer.IsEmpty)
        {
            return null;
        }

        string value;
        try
        {
            value = StrictUtf8.GetString(buffer);
        }
        catch (DecoderFallbackException)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid credential text.");
        }

        if (value.IndexOf('\0') >= 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid credential text.");
        }

        return value;
    }

    private static IReadOnlyList<string> DecodeCredentialMessages(byte[] buffer, int length)
    {
        if (length < sizeof(uint))
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid credential messages.");
        }

        ReadOnlySpan<byte> source = buffer.AsSpan(0, length);
        int offset = 0;
        uint count = BinaryPrimitives.ReadUInt32LittleEndian(source);
        offset += sizeof(uint);
        if (count > MaxCredentialMessages)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded credential message count.");
        }

        var messages = new string[checked((int)count)];
        for (int index = 0; index < messages.Length; index++)
        {
            if (source.Length - offset < sizeof(uint))
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned invalid credential messages.");
            }

            uint messageLength = BinaryPrimitives.ReadUInt32LittleEndian(source[offset..]);
            offset += sizeof(uint);
            if (messageLength is 0 or > MaxCredentialMessageBytes ||
                messageLength > source.Length - offset)
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned invalid credential messages.");
            }

            string? message = DecodeCredentialText(source.Slice(offset, checked((int)messageLength)));
            messages[index] = message ?? throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid credential messages.");
            offset += checked((int)messageLength);
        }

        if (offset != source.Length)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid credential messages.");
        }

        return Array.AsReadOnly(messages);
    }

    public PowerShellInvocationResult InvokeWithDiagnostics()
    {
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        ulong nativeResultHandle = 0;
        int status = NativeMethods.InvokeToResult(lease.Value, &nativeResultHandle, &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);

        using var invocationResultHandle = new PowerShellInvocationResultHandle(nativeResultHandle);
        PowerShellInvocationResult invocationResult = ReadInvocationResult(invocationResultHandle);
        if (invocationResult.IsTerminatingFailure)
        {
            throw new PowerShellInvocationException(
                PowerShellFfiStatus.ManagedFailure,
                "PowerShell invocation terminated.",
                invocationResult);
        }

        return invocationResult;
    }

    public PowerShellInvocationOperation BeginInvoke()
    {
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        ulong nativeOperationHandle = 0;
        int status = NativeMethods.InvokeAsync(lease.Value, &nativeOperationHandle, &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
        return new PowerShellInvocationOperation(new PowerShellOperationHandle(nativeOperationHandle));
    }

    /// <summary>
    /// Starts a bounded invocation whose output can be read as copied tagged
    /// values with an explicit acknowledgement cursor.
    /// </summary>
    public PowerShellTypedResultInvocation BeginTypedResultInvocation(
        PowerShellValuePagerOptions? options = null)
    {
        EnsureTypedResultPagingSupported();
        options ??= new PowerShellValuePagerOptions();
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        ulong nativeTypedResultHandle = 0;
        int status = NativeMethods.BeginTypedResultInvocation(
            lease.Value,
            checked((uint)options.MaximumBufferedRecords),
            checked((uint)options.MaximumPageRecords),
            &nativeTypedResultHandle,
            &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
        if (nativeTypedResultHandle == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an invalid typed result invocation handle.");
        }

        return new PowerShellTypedResultInvocation(nativeTypedResultHandle, options);
    }

    /// <summary>
    /// Starts one bounded invocation with independently acknowledged copied result and diagnostic pages.
    /// </summary>
    public PowerShellObservedInvocation BeginObservedInvocation(
        PowerShellObservedInvocationOptions? options = null)
    {
        EnsureObservedInvocationSupported();
        options ??= new PowerShellObservedInvocationOptions();
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        ulong nativeObservedHandle = 0;
        int status = NativeMethods.BeginObservedInvocation(
            lease.Value,
            checked((uint)options.MaximumBufferedResultRecords),
            checked((uint)options.MaximumResultPageRecords),
            checked((uint)options.MaximumBufferedDiagnosticRecords),
            checked((uint)options.MaximumDiagnosticPageRecords),
            &nativeObservedHandle,
            &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
        if (nativeObservedHandle == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an invalid observed invocation handle.");
        }

        return new PowerShellObservedInvocation(nativeObservedHandle, options);
    }

    public Task<PowerShellInvocationResult> InvokeAsync(CancellationToken cancellationToken = default)
    {
        return PowerShellAsyncOperationAwaiter.GetResultAsync(BeginInvoke(), cancellationToken, releaseWhenComplete: true);
    }

    public string InvokeText()
    {
        PowerShellInvocationResult invocationResult = InvokeWithDiagnostics();
        var output = new StringBuilder();
        foreach (PowerShellObjectSnapshot record in invocationResult.Output.Records)
        {
            output.AppendLine(record.DisplayText);
        }

        return output.ToString();
    }

    public void Stop()
    {
        InvokeForHandle(static (nativeHandle, result) => NativeMethods.Stop(nativeHandle, result));
    }

    public void Dispose()
    {
        handle.Dispose();
    }

    private static NativeAbiInfo GetAbiInfo()
    {
        NativeAbiInfo info = new() { Size = checked((uint)sizeof(NativeAbiInfo)) };
        int status = NativeMethods.GetAbiInfo(&info);
        if (status != Success)
        {
            throw new PowerShellFfiException((PowerShellFfiStatus)status, "Native PowerShell FFI ABI metadata is unavailable.");
        }

        return info;
    }

    internal static void EnsureSupportedAbi()
    {
        EnsureSupportedAbi(GetAbiInfo());
    }

    internal static void EnsureCapabilityRpcSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & CapabilityRpcFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support bounded capability RPC.");
        }
    }

    internal static void EnsureDuplexBrokerChannelSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & DuplexBrokerChannelFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected native asset does not support the duplex broker channel.");
        }
    }

    internal static void EnsureGeneratedBridgeAttachmentSupported()
    {
        EnsureDuplexBrokerChannelSupported();
        if ((FeatureFlags & GeneratedBridgeAttachmentFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The loaded native runtime does not support generated bridge attachment.");
        }
    }

    internal static void EnsureBrokerTerminalObservationSupported()
    {
        EnsureDuplexBrokerChannelSupported();
        if ((FeatureFlags & BrokerTerminalObservationFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The loaded native runtime does not support broker terminal observation.");
        }
    }

    internal static void EnsureReliableBridgeEventsSupported()
    {
        EnsureGeneratedBridgeAttachmentSupported();
        if ((FeatureFlags & ReliableBridgeEventsFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support reliable generated bridge events.");
        }
    }

    internal static void EnsureLiveObjectProbeSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & LiveObjectProbeFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support the live object probe.");
        }
    }

    internal static void EnsureLiveSessionObjectProbeSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & LiveSessionObjectProbeFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support live session object probes.");
        }
    }

    internal static void EnsureLiveObjectContractsSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & LiveObjectContractsFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support registered live object contracts.");
        }
    }

    internal static void EnsureLiveStreamPollingSupported()
    {
        NativeAbiInfo info = GetAbiInfo();
        EnsureSupportedAbi(info);
        if ((info.FeatureFlags & LiveStreamPollingFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell native asset does not support live stream polling.");
        }
    }

    internal static void EnsureTypedResultPagingSupported()
    {
        EnsureTypedResultPagingSupported(GetAbiInfo());
    }

    internal static void EnsureSecretAdaptersSupported()
    {
        EnsureSupportedAbi();
        if ((FeatureFlags & SecretAdaptersFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support explicit secret adapters.");
        }
    }

    private static void EnsureCredentialResultSupported()
    {
        EnsureSecretAdaptersSupported();
        if ((FeatureFlags & CredentialResultFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support bounded credential results.");
        }
    }

    internal static void EnsureTypedResultPagingSupported(NativeAbiInfo info)
    {
        EnsureSupportedAbi(info);
        if ((info.FeatureFlags & TypedResultPagingFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell native asset does not support typed result paging.");
        }
    }

    internal static void EnsureObservedInvocationSupported()
    {
        NativeAbiInfo info = GetAbiInfo();
        EnsureSupportedAbi(info);
        if ((info.FeatureFlags & ObservedInvocationFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell native asset does not support observed invocations.");
        }
    }

    internal static void EnsureObservedPresentationSupported()
    {
        EnsureObservedInvocationSupported();
        if ((FeatureFlags & ObservedPresentationFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support structured observed presentation records.");
        }
    }

    internal static void EnsureSessionPreflightSupported()
    {
        NativeAbiInfo info = GetAbiInfo();
        EnsureSupportedAbi(info);
        if ((info.FeatureFlags & SessionPreflightFeature) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell native asset does not support session preflight.");
        }
    }

    private static void EnsureSupportedAbi(NativeAbiInfo info)
    {
        if (info.AbiVersion != RequiredAbiVersion ||
            info.MinimumCompatibleAbiVersion > RequiredAbiVersion ||
            (info.FeatureFlags & RequiredFeatures) != RequiredFeatures)
        {
            throw new NotSupportedException(
                $"Native PowerShell FFI ABI {info.AbiVersion} does not support facade ABI {RequiredAbiVersion} structured errors, diagnostics, UTF-8, value, command, input, result, async operation, session, and credential-result features.");
        }
    }

    internal static PowerShell CreateFromNative(ulong nativeHandle)
    {
        if (nativeHandle == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an invalid builder handle.");
        }

        return new PowerShell(new PowerShellHandle(nativeHandle));
    }

    internal static PowerShellInvocationResult ReadInvocationResult(PowerShellInvocationResultHandle resultHandle)
    {
        using PowerShellInvocationResultHandle.HandleLease lease = resultHandle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult callResult = NativeCall.CreateResult(diagnostic);
        uint resultFlags = 0;
        uint sequenceCount = 0;
        int status = NativeMethods.GetInvocationResultInfo(lease.Value, &resultFlags, &sequenceCount, &callResult);
        NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        uint nativeState = 0;
        ulong invocationId = 0;
        uint hadErrors = 0;
        callResult = NativeCall.CreateResult(diagnostic);
        status = NativeMethods.GetInvocationResultMetadata(
            lease.Value,
            &nativeState,
            &invocationId,
            &hadErrors,
            &callResult);
        NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        if (nativeState is not (uint)PowerShellInvocationState.Completed and not (uint)PowerShellInvocationState.Terminated ||
            hadErrors > 1)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid invocation metadata.");
        }
        if (sequenceCount > MaxSequenceRecords)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded invocation sequence count.");
        }

        var streams = new NativeStreamSnapshot[checked((int)StreamCount)];
        for (uint stream = 0; stream < StreamCount; stream++)
        {
            streams[checked((int)stream)] = ReadStreamSnapshot(lease.Value, stream);
        }

        var sequence = new List<PowerShellStreamSequenceRecord>(checked((int)sequenceCount));
        for (uint index = 0; index < sequenceCount; index++)
        {
            callResult = NativeCall.CreateResult(diagnostic);
            uint stream = 0;
            uint recordIndex = 0;
            ulong recordSequence = 0;
            status = NativeMethods.GetInvocationResultSequenceRecord(
                lease.Value,
                index,
                &stream,
                &recordIndex,
                &recordSequence,
                &callResult);
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);
            if (stream >= StreamCount || recordIndex >= streams[checked((int)stream)].Records.Count)
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned an invalid invocation sequence record.");
            }

            sequence.Add(new PowerShellStreamSequenceRecord(
                (PowerShellStreamKind)stream,
                recordIndex,
                recordSequence));
        }

        return new PowerShellInvocationResult(
            CreateOutputSnapshot(streams[(int)PowerShellStreamKind.Output]),
            CreateErrorSnapshot(streams[(int)PowerShellStreamKind.Error]),
            CreateTextSnapshot(streams[(int)PowerShellStreamKind.Warning], PowerShellStreamKind.Warning),
            CreateTextSnapshot(streams[(int)PowerShellStreamKind.Verbose], PowerShellStreamKind.Verbose),
            CreateTextSnapshot(streams[(int)PowerShellStreamKind.Debug], PowerShellStreamKind.Debug),
            CreateTextSnapshot(streams[(int)PowerShellStreamKind.Information], PowerShellStreamKind.Information),
            CreateTextSnapshot(streams[(int)PowerShellStreamKind.Progress], PowerShellStreamKind.Progress),
            sequence.AsReadOnly(),
            (PowerShellInvocationState)nativeState,
            invocationId,
            hadErrors != 0,
            (resultFlags & ResultTerminatingFailure) != 0,
            (resultFlags & ResultSequenceTruncated) != 0);
    }

    private static NativeStreamSnapshot ReadStreamSnapshot(ulong resultHandle, uint stream)
    {
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult callResult = NativeCall.CreateResult(diagnostic);
        uint recordCount = 0;
        uint streamFlags = 0;
        int status = NativeMethods.GetInvocationResultStreamInfo(
            resultHandle,
            stream,
            &recordCount,
            &streamFlags,
            &callResult);
        NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        if (recordCount > MaxRecordsPerStream)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded invocation stream count.");
        }

        callResult = NativeCall.CreateResult(diagnostic);
        ulong totalRecordCount = 0;
        ulong droppedRecordCount = 0;
        status = NativeMethods.GetInvocationResultStreamTotals(
            resultHandle,
            stream,
            &totalRecordCount,
            &droppedRecordCount,
            &callResult);
        NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        if (totalRecordCount < recordCount || droppedRecordCount > totalRecordCount)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned invalid invocation stream totals.");
        }

        var records = new List<NativeStreamRecord>(checked((int)recordCount));
        for (uint index = 0; index < recordCount; index++)
        {
            callResult = NativeCall.CreateResult(diagnostic);
            ulong sequence = 0;
            uint recordFlags = 0;
            status = NativeMethods.GetInvocationResultStreamRecordInfo(
                resultHandle,
                stream,
                index,
                &sequence,
                &recordFlags,
                &callResult);
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);

            var fields = new string[checked((int)RecordFieldCount)];
            for (uint field = 0; field < RecordFieldCount; field++)
            {
                fields[field] = ReadStreamRecordField(resultHandle, stream, index, field);
            }

            callResult = NativeCall.CreateResult(diagnostic);
            uint propertyEntryCount = 0;
            uint droppedPropertyEntryCount = 0;
            uint typeNameCount = 0;
            uint droppedTypeNameCount = 0;
            uint projectionFlags = 0;
            status = NativeMethods.GetInvocationResultStreamRecordProjectionInfo(
                resultHandle,
                stream,
                index,
                &propertyEntryCount,
                &droppedPropertyEntryCount,
                &typeNameCount,
                &droppedTypeNameCount,
                &projectionFlags,
                &callResult);
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);

            PowerShellValue? scalarValue = (projectionFlags & RecordScalarValuePresent) != 0
                ? ReadStreamRecordValue(resultHandle, stream, index, 0)
                : null;
            PowerShellValue? propertyBag = (projectionFlags & RecordPropertyBagPresent) != 0
                ? ReadStreamRecordValue(resultHandle, stream, index, 1)
                : null;
            PowerShellValue? errorTargetValue = (projectionFlags & RecordErrorTargetValuePresent) != 0
                ? ReadStreamRecordValue(resultHandle, stream, index, 2)
                : null;

            records.Add(new NativeStreamRecord(
                sequence,
                recordFlags,
                projectionFlags,
                propertyEntryCount,
                droppedPropertyEntryCount,
                typeNameCount,
                droppedTypeNameCount,
                scalarValue,
                propertyBag,
                errorTargetValue,
                fields));
        }

        return new NativeStreamSnapshot(streamFlags, totalRecordCount, droppedRecordCount, records);
    }

    private static string ReadStreamRecordField(ulong resultHandle, uint stream, uint recordIndex, uint field)
    {
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult callResult = NativeCall.CreateResult(diagnostic);
        nuint requiredLength = 0;
        int status = NativeMethods.CopyInvocationResultStreamRecordField(
            resultHandle,
            stream,
            recordIndex,
            field,
            null,
            0,
            &requiredLength,
            &callResult);
        if (status != Success && status != BufferTooSmall)
        {
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        }

        if (requiredLength > MaxResultFieldUtf8Bytes)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded invocation stream field.");
        }

        byte[] value = new byte[checked((int)requiredLength)];
        fixed (byte* valuePointer = value)
        {
            callResult = NativeCall.CreateResult(diagnostic);
            status = NativeMethods.CopyInvocationResultStreamRecordField(
                resultHandle,
                stream,
                recordIndex,
                field,
                valuePointer,
                (nuint)value.Length,
                &requiredLength,
                &callResult);
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        }

        return Encoding.UTF8.GetString(value, 0, checked((int)requiredLength));
    }

    private static PowerShellValue ReadStreamRecordValue(ulong resultHandle, uint stream, uint recordIndex, uint valueSlot)
    {
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult callResult = NativeCall.CreateResult(diagnostic);
        uint kind = 0;
        nuint requiredLength = 0;
        int status = NativeMethods.CopyInvocationResultStreamRecordValue(
            resultHandle,
            stream,
            recordIndex,
            valueSlot,
            &kind,
            null,
            0,
            &requiredLength,
            &callResult);
        if (status != Success && status != BufferTooSmall)
        {
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        }

        if (requiredLength > MaxResultValueBytes)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.ManagedFailure,
                "Native PowerShell FFI returned an unbounded invocation snapshot value.");
        }

        byte[] payload = new byte[checked((int)requiredLength)];
        fixed (byte* payloadPointer = payload)
        {
            callResult = NativeCall.CreateResult(diagnostic);
            status = NativeMethods.CopyInvocationResultStreamRecordValue(
                resultHandle,
                stream,
                recordIndex,
                valueSlot,
                &kind,
                payloadPointer,
                (nuint)payload.Length,
                &requiredLength,
                &callResult);
            NativeCall.ThrowIfFailed(status, callResult, diagnostic);
        }

        return PowerShellValue.FromNative(kind, payload);
    }

    private static PowerShellStreamSnapshot<PowerShellObjectSnapshot> CreateOutputSnapshot(NativeStreamSnapshot snapshot)
    {
        var output = new PowerShellObjectSnapshot[snapshot.Records.Count];
        for (int index = 0; index < output.Length; index++)
        {
            NativeStreamRecord record = snapshot.Records[index];
            string[] typeNames = record.Fields[1].Length == 0
                ? Array.Empty<string>()
                : record.Fields[1].Split('\n');
            output[index] = new PowerShellObjectSnapshot(
                record.Fields[0],
                typeNames,
                record.Sequence,
                (record.Flags & RecordFieldsTruncated) != 0,
                record.ScalarValue,
                record.PropertyBag,
                record.PropertyEntryCount,
                record.DroppedPropertyEntryCount,
                record.TypeNameCount,
                record.DroppedTypeNameCount,
                (record.ProjectionFlags & RecordPropertyBagTruncated) != 0);
        }

        return new PowerShellStreamSnapshot<PowerShellObjectSnapshot>(
            PowerShellStreamKind.Output,
            output,
            (snapshot.Flags & StreamTruncated) != 0,
            snapshot.TotalRecordCount,
            snapshot.DroppedRecordCount);
    }

    private static PowerShellStreamSnapshot<PowerShellInvocationError> CreateErrorSnapshot(NativeStreamSnapshot snapshot)
    {
        var errors = new PowerShellInvocationError[snapshot.Records.Count];
        for (int index = 0; index < errors.Length; index++)
        {
            NativeStreamRecord record = snapshot.Records[index];
            if (!int.TryParse(
                record.Fields[20],
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out int exceptionHResult))
            {
                throw new PowerShellFfiException(
                    PowerShellFfiStatus.ManagedFailure,
                    "Native PowerShell FFI returned an invalid exception HResult.");
            }

            errors[index] = new PowerShellInvocationError(
                record.Fields[0],
                record.Fields[2],
                record.Fields[3],
                record.Fields[4],
                exceptionHResult,
                record.Fields[5],
                record.Fields[6],
                record.Fields[7],
                record.Fields[8],
                record.Fields[9],
                record.Fields[10],
                record.Fields[11],
                record.Fields[12],
                record.Fields[13],
                record.Fields[14],
                record.Fields[15],
                record.Fields[16],
                record.Fields[17],
                record.Fields[18],
                record.Fields[19],
                record.ErrorTargetValue,
                record.Sequence,
                (record.Flags & RecordFieldsTruncated) != 0);
        }

        return new PowerShellStreamSnapshot<PowerShellInvocationError>(
            PowerShellStreamKind.Error,
            errors,
            (snapshot.Flags & StreamTruncated) != 0,
            snapshot.TotalRecordCount,
            snapshot.DroppedRecordCount);
    }

    private static PowerShellStreamSnapshot<PowerShellStreamRecord> CreateTextSnapshot(
        NativeStreamSnapshot snapshot,
        PowerShellStreamKind stream)
    {
        var records = new PowerShellStreamRecord[snapshot.Records.Count];
        for (int index = 0; index < records.Length; index++)
        {
            NativeStreamRecord record = snapshot.Records[index];
            records[index] = new PowerShellStreamRecord(
                record.Fields[0],
                record.Sequence,
                (record.Flags & RecordFieldsTruncated) != 0);
        }

        return new PowerShellStreamSnapshot<PowerShellStreamRecord>(
            stream,
            records,
            (snapshot.Flags & StreamTruncated) != 0,
            snapshot.TotalRecordCount,
            snapshot.DroppedRecordCount);
    }

    private sealed class NativeStreamSnapshot
    {
        internal NativeStreamSnapshot(uint flags, ulong totalRecordCount, ulong droppedRecordCount, List<NativeStreamRecord> records)
        {
            Flags = flags;
            TotalRecordCount = totalRecordCount;
            DroppedRecordCount = droppedRecordCount;
            Records = records;
        }

        internal uint Flags { get; }

        internal ulong TotalRecordCount { get; }

        internal ulong DroppedRecordCount { get; }

        internal List<NativeStreamRecord> Records { get; }
    }

    private sealed class NativeStreamRecord
    {
        internal NativeStreamRecord(
            ulong sequence,
            uint flags,
            uint projectionFlags,
            uint propertyEntryCount,
            uint droppedPropertyEntryCount,
            uint typeNameCount,
            uint droppedTypeNameCount,
            PowerShellValue? scalarValue,
            PowerShellValue? propertyBag,
            PowerShellValue? errorTargetValue,
            string[] fields)
        {
            Sequence = sequence;
            Flags = flags;
            ProjectionFlags = projectionFlags;
            PropertyEntryCount = propertyEntryCount;
            DroppedPropertyEntryCount = droppedPropertyEntryCount;
            TypeNameCount = typeNameCount;
            DroppedTypeNameCount = droppedTypeNameCount;
            ScalarValue = scalarValue;
            PropertyBag = propertyBag;
            ErrorTargetValue = errorTargetValue;
            Fields = fields;
        }

        internal ulong Sequence { get; }

        internal uint Flags { get; }

        internal uint ProjectionFlags { get; }

        internal uint PropertyEntryCount { get; }

        internal uint DroppedPropertyEntryCount { get; }

        internal uint TypeNameCount { get; }

        internal uint DroppedTypeNameCount { get; }

        internal PowerShellValue? ScalarValue { get; }

        internal PowerShellValue? PropertyBag { get; }

        internal PowerShellValue? ErrorTargetValue { get; }

        internal string[] Fields { get; }
    }

    private void InvokeValueForHandle(PowerShellValue value, NativeValueHandleOperation operation)
    {
        byte[] payload = value.Payload;
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        fixed (byte* payloadPointer = payload)
        {
            NativeDataValue nativeValue = CreateNativeValue(value, payloadPointer);
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = operation(lease.Value, &nativeValue, &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }
    }

    private void AddSecretParameter(string name, uint kind, PowerShellSecret secret, string? userName)
    {
        EnsureSecretAdaptersSupported();
        byte[] nameBytes = EncodeUtf8(name);
        byte[]? userNameBytes = userName is null ? null : EncodeUtf8(userName);
        int userNameLength = userNameBytes?.Length ?? 0;
        int secretLength = secret.Length;
        byte[] payload = new byte[checked((userName is null ? 0 : sizeof(int) + userNameLength) + secretLength * sizeof(char))];
        try
        {
            if (userNameBytes is not null)
            {
                BitConverter.TryWriteBytes(payload, userNameLength);
                userNameBytes.CopyTo(payload, sizeof(int));
            }

            secret.CopyTo(MemoryMarshal.Cast<byte, char>(payload.AsSpan(userName is null ? 0 : sizeof(int) + userNameLength)));
            using PowerShellHandle.HandleLease lease = handle.Borrow();
            fixed (byte* namePointer = nameBytes)
            fixed (byte* payloadPointer = payload)
            {
                NativeDataValue nativeValue = new()
                {
                    Size = checked((uint)sizeof(NativeDataValue)),
                    Kind = kind,
                    Data = payloadPointer,
                    DataLength = (nuint)payload.Length,
                };
                byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
                NativeCallResult result = NativeCall.CreateResult(diagnostic);
                int status = NativeMethods.AddParameterValue(
                    lease.Value,
                    new NativeUtf8Span { Data = namePointer, Length = (nuint)nameBytes.Length },
                    &nativeValue,
                    &result);
                NativeCall.ThrowIfFailed(status, result, diagnostic);
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(payload);
            if (userNameBytes is not null)
            {
                CryptographicOperations.ZeroMemory(userNameBytes);
            }
        }
    }

    internal void AddLiveObjectProbe(nint comObject)
    {
        if (comObject == 0)
        {
            throw new ArgumentException("Live object probe pointer is null.", nameof(comObject));
        }

        InvokeForHandle((nativeHandle, result) => NativeMethods.AddArgumentLiveObject(nativeHandle, comObject, result));
    }

    internal static NativeDataValue CreateNativeValue(PowerShellValue value, byte* payload)
    {
        return new NativeDataValue
        {
            Size = checked((uint)sizeof(NativeDataValue)),
            Kind = (uint)value.Kind,
            Flags = 0,
            Reserved = 0,
            Data = value.Payload.Length == 0 ? null : payload,
            DataLength = (nuint)value.Payload.Length,
        };
    }

    private void InvokeForHandle(NativeHandleOperation operation)
    {
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
        NativeCallResult result = NativeCall.CreateResult(diagnostic);
        int status = operation(lease.Value, &result);
        NativeCall.ThrowIfFailed(status, result, diagnostic);
    }

    private void InvokeUtf8ForHandle(string value, NativeUtf8HandleOperation operation)
    {
        byte[] bytes = EncodeUtf8(value);
        using PowerShellHandle.HandleLease lease = handle.Borrow();
        fixed (byte* pointer = bytes)
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = operation(
                lease.Value,
                new NativeUtf8Span { Data = pointer, Length = (nuint)bytes.Length },
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }
    }

    private static void InvokeUtf8(string value, NativeUtf8Operation operation)
    {
        byte[] bytes = EncodeUtf8(value);
        fixed (byte* pointer = bytes)
        {
            byte* diagnostic = stackalloc byte[NativeCall.DiagnosticCapacity];
            NativeCallResult result = NativeCall.CreateResult(diagnostic);
            int status = operation(
                new NativeUtf8Span { Data = pointer, Length = (nuint)bytes.Length },
                &result);
            NativeCall.ThrowIfFailed(status, result, diagnostic);
        }
    }

    internal static byte[] EncodeUtf8(string value)
    {
        if (value.IndexOf('\0') >= 0)
        {
            throw new ArgumentException("Values passed to the native PowerShell API cannot contain NUL characters.", nameof(value));
        }

        return Encoding.UTF8.GetBytes(value);
    }

    private static bool IsBridgeVariableName(string value)
    {
        if (value.Length is < 1 or > 64 ||
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

    private unsafe delegate int NativeHandleOperation(ulong nativeHandle, NativeCallResult* result);

    private unsafe delegate int NativeUtf8HandleOperation(
        ulong nativeHandle,
        NativeUtf8Span value,
        NativeCallResult* result);

    private unsafe delegate int NativeUtf8Operation(NativeUtf8Span value, NativeCallResult* result);

    private unsafe delegate int NativeValueHandleOperation(
        ulong nativeHandle,
        NativeDataValue* value,
        NativeCallResult* result);

    private const uint SecretSecureStringKind = 15;
    private const uint SecretCredentialKind = 16;
}
