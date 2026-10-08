#nullable enable

using System;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;

namespace NativeHost
{
    public static partial class Bindings
    {
        private const uint FfiRemoteProviderUnavailable = 0;
        private const uint FfiRemoteProviderReasonPayloadHooksMissing = 1;
        private const uint FfiRemoteProviderReasonTransportImplementationMissing = 2;

        private static readonly Lazy<FfiRemoteProviderProbeResult> DevolutionsWsManProviderProbe =
            new(ProbeDevolutionsWsManProvider, LazyThreadSafetyMode.ExecutionAndPublication);

        [UnmanagedCallersOnly]
        public static unsafe int FfiRuntimeDiagnostics_GetDevolutionsWsManProviderInfo(
            uint* status,
            uint* unavailableReason,
            ulong* capabilities,
            FfiCallResult* result)
        {
            if (status == null || unavailableReason == null || capabilities == null)
            {
                return WriteFailure(result, FfiStatusInvalidArgument, "Remote provider diagnostic output is invalid.");
            }

            IntPtr outputStatus = (IntPtr)status;
            IntPtr outputReason = (IntPtr)unavailableReason;
            IntPtr outputCapabilities = (IntPtr)capabilities;
            return Execute(result, () =>
            {
                FfiRemoteProviderProbeResult probe = DevolutionsWsManProviderProbe.Value;
                Marshal.WriteInt32(outputStatus, checked((int)probe.Status));
                Marshal.WriteInt32(outputReason, checked((int)probe.UnavailableReason));
                Marshal.WriteInt64(outputCapabilities, unchecked((long)probe.Capabilities));
            });
        }

        private static FfiRemoteProviderProbeResult ProbeDevolutionsWsManProvider()
        {
            return HasDevolutionsRemotingProviderRegistry()
                ? FfiRemoteProviderProbeResult.Unavailable(
                    FfiRemoteProviderReasonTransportImplementationMissing)
                : FfiRemoteProviderProbeResult.Unavailable(
                    FfiRemoteProviderReasonPayloadHooksMissing);
        }

        private static bool HasDevolutionsRemotingProviderRegistry()
        {
            try
            {
                Assembly automation = typeof(RunspaceConnectionInfo).Assembly;
                Type? provider = automation.GetType(
                    "System.Management.Automation.Remoting.Client.IClientRemotingTransportProvider",
                    throwOnError: false,
                    ignoreCase: false);
                Type? options = automation.GetType(
                    "System.Management.Automation.Remoting.Client.ClientRemotingTransportProviderOptions",
                    throwOnError: false,
                    ignoreCase: false);
                Type? registry = automation.GetType(
                    "System.Management.Automation.Remoting.Client.ClientRemotingTransportProviderRegistry",
                    throwOnError: false,
                    ignoreCase: false);
                Type? context = automation.GetType(
                    "System.Management.Automation.Remoting.Client.ClientRemotingTransportCreationContext",
                    throwOnError: false,
                    ignoreCase: false);
                Type? sessionTransport = automation.GetType(
                    "System.Management.Automation.Remoting.Client.BaseClientSessionTransportManager",
                    throwOnError: false,
                    ignoreCase: false);
                if (provider is null ||
                    options is null ||
                    registry is null ||
                    context is null ||
                    sessionTransport is null ||
                    !provider.IsPublic ||
                    !provider.IsInterface ||
                    !options.IsPublic ||
                    !options.IsSealed ||
                    !registry.IsPublic ||
                    !registry.IsAbstract ||
                    !registry.IsSealed ||
                    !context.IsPublic ||
                    !context.IsSealed ||
                    !sessionTransport.IsPublic)
                {
                    return false;
                }

                MethodInfo? canCreateTransport = provider.GetMethod(
                    "CanCreateTransport",
                    BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly,
                    binder: null,
                    types: [typeof(RunspaceConnectionInfo)],
                    modifiers: null);
                MethodInfo? createSessionTransport = provider.GetMethod(
                    "CreateSessionTransport",
                    BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly,
                    binder: null,
                    types: [context],
                    modifiers: null);
                MethodInfo? register = registry.GetMethod(
                    "Register",
                    BindingFlags.Public | BindingFlags.Static | BindingFlags.DeclaredOnly,
                    binder: null,
                    types: [provider, options],
                    modifiers: null);
                PropertyInfo? replaceBuiltInTransports = options.GetProperty(
                    "ReplaceBuiltInTransports",
                    BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly);
                PropertyInfo? priority = options.GetProperty(
                    "Priority",
                    BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly);
                if (canCreateTransport?.ReturnType != typeof(bool) ||
                    createSessionTransport?.ReturnType != sessionTransport ||
                    register?.ReturnType != typeof(IDisposable) ||
                    !IsPublicSettableProperty(replaceBuiltInTransports, typeof(bool)) ||
                    !IsPublicSettableProperty(priority, typeof(int)))
                {
                    return false;
                }

                MethodInfo? createRunspacePool = typeof(RunspaceFactory).GetMethod(
                    "CreateRunspacePool",
                    BindingFlags.Public | BindingFlags.Static,
                    binder: null,
                    types:
                    [
                        typeof(int),
                        typeof(int),
                        typeof(RunspaceConnectionInfo),
                        typeof(System.Management.Automation.Host.PSHost),
                        typeof(TypeTable),
                        typeof(PSPrimitiveDictionary),
                    ],
                    modifiers: null);
                return createRunspacePool?.ReturnType == typeof(RunspacePool);
            }
            catch (AmbiguousMatchException)
            {
                return false;
            }
            catch (TypeLoadException)
            {
                return false;
            }
        }

        private static bool IsPublicSettableProperty(PropertyInfo? property, Type propertyType)
        {
            return property?.PropertyType == propertyType &&
                   property.GetMethod?.IsPublic == true &&
                   property.SetMethod?.IsPublic == true &&
                   property.GetIndexParameters().Length == 0;
        }

        private sealed class FfiRemoteProviderProbeResult
        {
            private FfiRemoteProviderProbeResult(
                uint status,
                uint unavailableReason,
                ulong capabilities)
            {
                Status = status;
                UnavailableReason = unavailableReason;
                Capabilities = capabilities;
            }

            internal uint Status { get; }

            internal uint UnavailableReason { get; }

            internal ulong Capabilities { get; }

            internal static FfiRemoteProviderProbeResult Unavailable(uint reason)
            {
                return new FfiRemoteProviderProbeResult(
                    FfiRemoteProviderUnavailable,
                    reason,
                    0);
            }
        }
    }
}
