using System;
using System.Runtime.InteropServices;
using System.Collections.ObjectModel;
using System.Management.Automation;

namespace NativeHost
{
    // PowerShell Class
    // https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.powershell

    public static class Bindings
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct ApiPS74
        {
            public IntPtr PowerShell_Create;
            public IntPtr PowerShell_AddArgument_String;
            public IntPtr PowerShell_AddParameter_String;
            public IntPtr PowerShell_AddParameter_Int;
            public IntPtr PowerShell_AddParameter_Long;
            public IntPtr PowerShell_AddCommand;
            public IntPtr PowerShell_AddScript;
            public IntPtr PowerShell_AddStatement;
            public IntPtr PowerShell_Invoke;
            public IntPtr PowerShell_Clear;
            public IntPtr PowerShell_ExportToXml;
            public IntPtr PowerShell_ExportToJson;
            public IntPtr PowerShell_ExportToString;
            public IntPtr Marshal_FreeCoTaskMem;
        }

        private static readonly object ApiPS74Lock = new object();
        private static IntPtr ApiPS74Ptr = IntPtr.Zero;

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_GetApiPS74()
        {
            lock (ApiPS74Lock)
            {
                if (ApiPS74Ptr == IntPtr.Zero)
                {
                    ApiPS74 api = CreateApiPS74();
                    ApiPS74Ptr = Marshal.AllocCoTaskMem(Marshal.SizeOf<ApiPS74>());
                    Marshal.StructureToPtr(api, ApiPS74Ptr, false);
                }

                return ApiPS74Ptr;
            }
        }

        private static unsafe ApiPS74 CreateApiPS74()
        {
            return new ApiPS74
            {
                PowerShell_Create = (IntPtr)(delegate* unmanaged<IntPtr>)&PowerShell_Create,
                PowerShell_AddArgument_String = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, void>)&PowerShell_AddArgument_String,
                PowerShell_AddParameter_String = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, IntPtr, void>)&PowerShell_AddParameter_String,
                PowerShell_AddParameter_Int = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, int, void>)&PowerShell_AddParameter_Int,
                PowerShell_AddParameter_Long = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, long, void>)&PowerShell_AddParameter_Long,
                PowerShell_AddCommand = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, void>)&PowerShell_AddCommand,
                PowerShell_AddScript = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, void>)&PowerShell_AddScript,
                PowerShell_AddStatement = (IntPtr)(delegate* unmanaged<IntPtr, void>)&PowerShell_AddStatement,
                PowerShell_Invoke = (IntPtr)(delegate* unmanaged<IntPtr, void>)&PowerShell_Invoke,
                PowerShell_Clear = (IntPtr)(delegate* unmanaged<IntPtr, void>)&PowerShell_Clear,
                PowerShell_ExportToXml = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, IntPtr>)&PowerShell_ExportToXml,
                PowerShell_ExportToJson = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, IntPtr>)&PowerShell_ExportToJson,
                PowerShell_ExportToString = (IntPtr)(delegate* unmanaged<IntPtr, IntPtr, IntPtr>)&PowerShell_ExportToString,
                Marshal_FreeCoTaskMem = (IntPtr)(delegate* unmanaged<IntPtr, void>)&Marshal_FreeCoTaskMem,
            };
        }

        [UnmanagedCallersOnly]
        public static IntPtr PowerShell_Create()
        {
            // https://stackoverflow.com/a/32108252
            PowerShell ps = PowerShell.Create();
            GCHandle gch = GCHandle.Alloc(ps, GCHandleType.Normal);
            IntPtr ptrHandle = GCHandle.ToIntPtr(gch);
            return ptrHandle;
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddArgument_String(IntPtr ptrHandle, IntPtr ptrArgument)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string argument = Marshal.PtrToStringUTF8(ptrArgument);
            ps.AddArgument(argument);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddParameter_String(IntPtr ptrHandle, IntPtr ptrName, IntPtr ptrValue)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            string value = Marshal.PtrToStringUTF8(ptrValue);
            ps.AddParameter(name, value);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddParameter_Int(IntPtr ptrHandle, IntPtr ptrName, int value)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            ps.AddParameter(name, value);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddParameter_Long(IntPtr ptrHandle, IntPtr ptrName, long value)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            ps.AddParameter(name, value);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddCommand(IntPtr ptrHandle, IntPtr ptrCommand)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string command = Marshal.PtrToStringUTF8(ptrCommand);
            ps.AddCommand(command);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddScript(IntPtr ptrHandle, IntPtr ptrScript)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string script = Marshal.PtrToStringUTF8(ptrScript);
            ps.AddScript(script);
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_AddStatement(IntPtr ptrHandle)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            ps.AddStatement();
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_Invoke(IntPtr ptrHandle)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            ps.Invoke();
        }

        [UnmanagedCallersOnly]
        public static void PowerShell_Clear(IntPtr ptrHandle)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            ps.Commands.Clear();
        }

        [UnmanagedCallersOnly]
        public static IntPtr PowerShell_ExportToXml(IntPtr ptrHandle, IntPtr ptrName)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            ps.AddScript(string.Format("[System.Management.Automation.PSSerializer]::Serialize(${0})", name));
            ps.AddStatement();
            Collection<PSObject> results = ps.Invoke();
            string result = results[0].ToString().Trim();
            ps.Commands.Clear();
            return Marshal.StringToCoTaskMemUTF8(result);
        }

        [UnmanagedCallersOnly]
        public static IntPtr PowerShell_ExportToJson(IntPtr ptrHandle, IntPtr ptrName)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            ps.AddScript(string.Format("${0} | ConvertTo-Json", name));
            ps.AddStatement();
            Collection<PSObject> results = ps.Invoke();
            string result = results[0].ToString().Trim();
            ps.Commands.Clear();
            return Marshal.StringToCoTaskMemUTF8(result);
        }

        [UnmanagedCallersOnly]
        public static IntPtr PowerShell_ExportToString(IntPtr ptrHandle, IntPtr ptrName)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            PowerShell ps = (PowerShell) gch.Target;
            string name = Marshal.PtrToStringUTF8(ptrName);
            ps.AddScript(string.Format("${0} | Out-String", name));
            ps.AddStatement();
            Collection<PSObject> results = ps.Invoke();
            string result = results[0].ToString().Trim();
            ps.Commands.Clear();
            return Marshal.StringToCoTaskMemUTF8(result);
        }

        // Marshal Class
        // https://learn.microsoft.com/en-us/dotnet/api/system.runtime.interopservices.marshal

        [UnmanagedCallersOnly]
        public static void Marshal_FreeCoTaskMem(IntPtr ptr)
        {
            Marshal.FreeCoTaskMem(ptr);
        }
    }
}
