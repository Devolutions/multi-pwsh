namespace Devolutions.PowerShell.Ffi;

public sealed class PowerShellRuntime
{
    private PowerShellRuntime(
        uint abiVersion,
        ulong featureFlags,
        string payloadDirectory,
        PowerShellRuntimeDiagnosticReport diagnostics)
    {
        AbiVersion = abiVersion;
        FeatureFlags = featureFlags;
        PayloadDirectory = payloadDirectory;
        Diagnostics = diagnostics;
    }

    public uint AbiVersion { get; }

    public ulong FeatureFlags { get; }

    public string PayloadDirectory { get; }

    /// <summary>
    /// Gets safe descriptive facts about the active payload and binding table.
    /// </summary>
    public PowerShellRuntimeDiagnosticReport Diagnostics { get; }

    public static PowerShellRuntime Activate()
    {
        PowerShell.Initialize();
        return CreateActivatedRuntime();
    }

    public static PowerShellRuntime Activate(string payloadDirectory)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(payloadDirectory);
        PowerShell.Initialize(payloadDirectory);
        return CreateActivatedRuntime();
    }

    public static PowerShellRuntime Activate(
        IReadOnlyList<PowerShellLiveObjectContractPack> contractPacks)
    {
        ArgumentNullException.ThrowIfNull(contractPacks);
        PowerShell.Initialize(contractPacks);
        return CreateActivatedRuntime();
    }

    public static PowerShellRuntime Activate(
        string payloadDirectory,
        IReadOnlyList<PowerShellLiveObjectContractPack> contractPacks)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(payloadDirectory);
        ArgumentNullException.ThrowIfNull(contractPacks);
        PowerShell.Initialize(payloadDirectory, contractPacks);
        return CreateActivatedRuntime();
    }

    private static PowerShellRuntime CreateActivatedRuntime()
    {
        uint abiVersion = PowerShell.AbiVersion;
        ulong featureFlags = PowerShell.FeatureFlags;
        string payloadDirectory = PowerShell.GetActivePayloadDirectory();
        return new PowerShellRuntime(
            abiVersion,
            featureFlags,
            payloadDirectory,
            PowerShellRuntimeDiagnosticReport.Create(payloadDirectory, featureFlags));
    }

    public PowerShell Create()
    {
        return PowerShell.Create();
    }

    /// <summary>
    /// Creates an experimental live payload-object probe for validating the
    /// cross-runtime <c>IUnknown</c> transport.
    /// </summary>
    public PowerShellLiveObjectProbe CreateLiveObjectProbe(long initialCount)
    {
        PowerShell.EnsureLiveObjectProbeSupported();
        return PowerShellLiveObjectProbe.Create(initialCount);
    }

    public PowerShellInvocationResult Invoke(
        PowerShellCommandRecipe recipe,
        PowerShellCommandPolicy? policy = null)
    {
        ArgumentNullException.ThrowIfNull(recipe);
        policy?.Validate(recipe);
        using PowerShell powerShell = Create();
        recipe.Apply(powerShell);
        return InvokeRecipe(powerShell, recipe.ResultSchema, recipe.Timeout);
    }

    public PowerShellInvocationResult Invoke(
        PowerShellScriptRecipe recipe,
        PowerShellCommandPolicy? policy = null)
    {
        ArgumentNullException.ThrowIfNull(recipe);
        policy?.Validate(recipe);
        using PowerShell powerShell = Create();
        recipe.Apply(powerShell);
        return InvokeRecipe(powerShell, recipe.ResultSchema, recipe.Timeout);
    }

    public async Task<PowerShellInvocationResult> InvokeAsync(
        PowerShellCommandRecipe recipe,
        PowerShellCommandPolicy? policy = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(recipe);
        policy?.Validate(recipe);
        using PowerShell powerShell = Create();
        recipe.Apply(powerShell);
        return await InvokeAsync(powerShell, recipe.ResultSchema, recipe.Timeout, cancellationToken).ConfigureAwait(false);
    }

    public async Task<PowerShellInvocationResult> InvokeAsync(
        PowerShellScriptRecipe recipe,
        PowerShellCommandPolicy? policy = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(recipe);
        policy?.Validate(recipe);
        using PowerShell powerShell = Create();
        recipe.Apply(powerShell);
        return await InvokeAsync(powerShell, recipe.ResultSchema, recipe.Timeout, cancellationToken).ConfigureAwait(false);
    }

    public PowerShellSession CreateSession(PowerShellSessionOptions options)
    {
        return PowerShellSession.Create(options);
    }

    /// <summary>
    /// Validates copied session configuration without creating a runspace,
    /// importing a module, or executing PowerShell code.
    /// </summary>
    public PowerShellSessionPreflightReport ValidateSessionConfiguration(PowerShellSessionOptions options)
    {
        return PowerShellSession.Preflight(options);
    }

    public PowerShellSessionPreflightReport ValidateSessionConfiguration(
        PowerShellSessionConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        return ValidateSessionConfiguration(new PowerShellSessionOptions(configuration: configuration));
    }

    public PowerShellSessionPool CreateSessionPool(PowerShellSessionPoolOptions options)
    {
        return PowerShellSessionPool.Create(options);
    }

    /// <summary>
    /// Parses copied parameter metadata without executing the supplied script or
    /// exposing SMA parser/AST types.
    /// </summary>
    public PowerShellScriptParseResult ParseScriptParameters(string script)
    {
        return PowerShellScriptParser.Parse(this, script);
    }

    /// <summary>
    /// Creates an opt-in duplex broker channel. Attach it to one invocation with
    /// <see cref="PowerShell.WithBroker"/>; that invocation must be asynchronous.
    /// </summary>
    public PowerShellBrokerChannel CreateBrokerChannel(PowerShellBrokerChannelOptions? options = null)
    {
        PowerShell.EnsureDuplexBrokerChannelSupported();
        return PowerShellBrokerChannel.Create(options ?? new PowerShellBrokerChannelOptions());
    }

    /// <summary>
    /// Creates a broker channel dedicated to generated Bridge Contract v2
    /// dispatchers. Bind generated dispatchers with
    /// <see cref="PowerShellBridgeChannel.CreateBinding"/>.
    /// </summary>
    public PowerShellBridgeChannel CreateBridgeChannel(PowerShellBrokerChannelOptions? options = null)
    {
        PowerShell.EnsureGeneratedBridgeAttachmentSupported();
        PowerShell.EnsureBrokerTerminalObservationSupported();
        PowerShell.EnsureReliableBridgeEventsSupported();
        return new PowerShellBridgeChannel(CreateBrokerChannel(options));
    }

    public PowerShellCapabilitySet RegisterCapabilities(IEnumerable<PowerShellCapabilityBinding> bindings)
    {
        if ((FeatureFlags & (1UL << 16)) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support bounded capability RPC.");
        }

        return PowerShellCapabilitySet.Register(bindings);
    }

    /// <summary>
    /// Registers a bounded staged-intent lifecycle over the existing capability dispatcher.
    /// </summary>
    public PowerShellStagedIntentCoordinator RegisterStagedIntents(
        IEnumerable<PowerShellStagedIntentDefinition> definitions)
    {
        if ((FeatureFlags & (1UL << 16)) == 0)
        {
            throw new PowerShellFfiException(
                PowerShellFfiStatus.UnsupportedCapability,
                "The selected PowerShell payload does not support bounded capability RPC.");
        }

        return PowerShellStagedIntentCoordinator.Register(definitions);
    }

    internal static PowerShellInvocationResult InvokeRecipe(
        PowerShell powerShell,
        PowerShellResultSchema? resultSchema,
        TimeSpan? timeout)
    {
        ArgumentNullException.ThrowIfNull(powerShell);
        if (timeout is null)
        {
            PowerShellInvocationResult result = powerShell.InvokeWithDiagnostics();
            resultSchema?.Validate(result);
            return result;
        }

        return InvokeAsync(powerShell, resultSchema, timeout, CancellationToken.None)
            .GetAwaiter()
            .GetResult();
    }

    private static async Task<PowerShellInvocationResult> InvokeAsync(
        PowerShell powerShell,
        PowerShellResultSchema? resultSchema,
        TimeSpan? timeout,
        CancellationToken cancellationToken)
    {
        using var timeoutCancellation = timeout is { } timeoutValue
            ? new CancellationTokenSource(timeoutValue)
            : null;
        using var linkedCancellation = timeoutCancellation is null
            ? null
            : CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, timeoutCancellation.Token);
        PowerShellInvocationResult result = await powerShell.InvokeAsync(
            linkedCancellation?.Token ?? cancellationToken).ConfigureAwait(false);
        resultSchema?.Validate(result);
        return result;
    }
}
