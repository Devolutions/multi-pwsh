using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Collections.ObjectModel;
using System.Reflection;
using System.Text.Json;
using System.Management.Automation;

namespace NativeHost
{
    // PowerShell Class
    // https://learn.microsoft.com/en-us/dotnet/api/system.management.automation.powershell

    public static partial class Bindings
    {
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

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_InvokeMemberJson(IntPtr ptrHandle, IntPtr ptrMemberName, IntPtr ptrArgumentsJson)
        {
            try
            {
                GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
                PowerShell ps = (PowerShell) gch.Target;
                string memberName = Marshal.PtrToStringUTF8(ptrMemberName) ?? string.Empty;
                string argsJson = Marshal.PtrToStringUTF8(ptrArgumentsJson) ?? "[]";
                object[] args = ParseJsonArray(argsJson);
                object result = ps.GetType().InvokeMember(memberName, BindingFlags.Public | BindingFlags.Instance | BindingFlags.InvokeMethod, null, ps, args);
                return Marshal.StringToCoTaskMemUTF8(SerializeSuccess(result));
            }
            catch (Exception ex)
            {
                return Marshal.StringToCoTaskMemUTF8(SerializeError(ex));
            }
        }

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_GetPropertyJson(IntPtr ptrHandle, IntPtr ptrPropertyName)
        {
            try
            {
                GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
                PowerShell ps = (PowerShell) gch.Target;
                string propertyName = Marshal.PtrToStringUTF8(ptrPropertyName) ?? string.Empty;
                object result = ps.GetType().InvokeMember(propertyName, BindingFlags.Public | BindingFlags.Instance | BindingFlags.GetProperty, null, ps, null);
                return Marshal.StringToCoTaskMemUTF8(SerializeSuccess(result));
            }
            catch (Exception ex)
            {
                return Marshal.StringToCoTaskMemUTF8(SerializeError(ex));
            }
        }

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_SetPropertyJson(IntPtr ptrHandle, IntPtr ptrPropertyName, IntPtr ptrValueJson)
        {
            try
            {
                GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
                PowerShell ps = (PowerShell) gch.Target;
                string propertyName = Marshal.PtrToStringUTF8(ptrPropertyName) ?? string.Empty;
                string valueJson = Marshal.PtrToStringUTF8(ptrValueJson) ?? "null";
                object value = ParseJsonValue(valueJson);
                ps.GetType().InvokeMember(propertyName, BindingFlags.Public | BindingFlags.Instance | BindingFlags.SetProperty, null, ps, new object[] { value });
                return Marshal.StringToCoTaskMemUTF8(SerializeSuccess(null));
            }
            catch (Exception ex)
            {
                return Marshal.StringToCoTaskMemUTF8(SerializeError(ex));
            }
        }

        [UnmanagedCallersOnly]
        public static IntPtr Bindings_InvokeStaticMemberJson(IntPtr ptrMemberName, IntPtr ptrArgumentsJson)
        {
            try
            {
                string memberName = Marshal.PtrToStringUTF8(ptrMemberName) ?? string.Empty;
                string argsJson = Marshal.PtrToStringUTF8(ptrArgumentsJson) ?? "[]";
                object[] args = ParseJsonArray(argsJson);
                object result = typeof(PowerShell).InvokeMember(memberName, BindingFlags.Public | BindingFlags.Static | BindingFlags.InvokeMethod, null, null, args);
                return Marshal.StringToCoTaskMemUTF8(SerializeSuccess(result));
            }
            catch (Exception ex)
            {
                return Marshal.StringToCoTaskMemUTF8(SerializeError(ex));
            }
        }

        [UnmanagedCallersOnly]
        public static void GCHandle_Free(IntPtr ptrHandle)
        {
            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
            if (gch.IsAllocated)
            {
                gch.Free();
            }
        }

        private static object[] ParseJsonArray(string json)
        {
            using JsonDocument doc = JsonDocument.Parse(json);
            if (doc.RootElement.ValueKind != JsonValueKind.Array)
            {
                throw new InvalidOperationException("Arguments JSON must be an array.");
            }

            List<object> values = new List<object>();
            foreach (JsonElement element in doc.RootElement.EnumerateArray())
            {
                values.Add(ParseElement(element));
            }
            return values.ToArray();
        }

        private static object ParseJsonValue(string json)
        {
            using JsonDocument doc = JsonDocument.Parse(json);
            return ParseElement(doc.RootElement);
        }

        private static object ParseElement(JsonElement element)
        {
            switch (element.ValueKind)
            {
                case JsonValueKind.Null:
                    return null;
                case JsonValueKind.True:
                case JsonValueKind.False:
                    return element.GetBoolean();
                case JsonValueKind.String:
                    return element.GetString();
                case JsonValueKind.Number:
                    if (element.TryGetInt32(out int intValue))
                    {
                        return intValue;
                    }
                    if (element.TryGetInt64(out long longValue))
                    {
                        return longValue;
                    }
                    return element.GetDouble();
                case JsonValueKind.Array:
                    List<object> list = new List<object>();
                    foreach (JsonElement item in element.EnumerateArray())
                    {
                        list.Add(ParseElement(item));
                    }
                    return list.ToArray();
                case JsonValueKind.Object:
                    if (element.TryGetProperty("kind", out JsonElement kindElement) && kindElement.ValueKind == JsonValueKind.String)
                    {
                        string kind = kindElement.GetString();
                        if (string.Equals(kind, "handle", StringComparison.OrdinalIgnoreCase) && element.TryGetProperty("handle", out JsonElement handleElement))
                        {
                            long handleValue = handleElement.ValueKind == JsonValueKind.String ? long.Parse(handleElement.GetString()) : handleElement.GetInt64();
                            IntPtr ptrHandle = new IntPtr(handleValue);
                            GCHandle gch = GCHandle.FromIntPtr(ptrHandle);
                            return gch.Target;
                        }
                    }

                    Hashtable table = new Hashtable();
                    foreach (JsonProperty property in element.EnumerateObject())
                    {
                        table[property.Name] = ParseElement(property.Value);
                    }
                    return table;
                default:
                    throw new InvalidOperationException($"Unsupported JSON value kind: {element.ValueKind}");
            }
        }

        private static string SerializeSuccess(object result)
        {
            object value = SerializeResultValue(result);
            return JsonSerializer.Serialize(new Dictionary<string, object>
            {
                ["ok"] = true,
                ["result"] = value,
            });
        }

        private static string SerializeError(Exception ex)
        {
            return JsonSerializer.Serialize(new Dictionary<string, object>
            {
                ["ok"] = false,
                ["errorType"] = ex.GetType().FullName,
                ["errorMessage"] = ex.Message,
            });
        }

        private static object SerializeResultValue(object value)
        {
            if (value == null)
            {
                return null;
            }

            if (value is string || value is bool || value is int || value is long || value is double || value is float || value is decimal)
            {
                return value;
            }

            Type valueType = value.GetType();
            if (valueType.IsEnum)
            {
                return value.ToString();
            }

            GCHandle handle = GCHandle.Alloc(value, GCHandleType.Normal);
            IntPtr ptrHandle = GCHandle.ToIntPtr(handle);
            return new Dictionary<string, object>
            {
                ["kind"] = "handle",
                ["handle"] = ptrHandle.ToInt64(),
                ["type"] = valueType.FullName,
            };
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