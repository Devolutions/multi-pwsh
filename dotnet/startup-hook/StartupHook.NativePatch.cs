using System;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading;

public static partial class StartupHook
{
    private const int UnixProtectRead = 0x1;
    private const int UnixProtectWrite = 0x2;
    private const int UnixProtectExecute = 0x4;
    private const string LibSystem = "/usr/lib/libSystem.B.dylib";

    private static readonly string[] LinuxClearCacheLibraryNames =
    {
        "libc.so.6",
        "libc.so",
        "libgcc_s.so.1",
        "libgcc_s.so",
        "libSystem.Native",
    };

    private static readonly object s_linuxClearCacheLock = new();
    private static LinuxClearCacheDelegate? s_linuxClearCache;

    private delegate void LinuxClearCacheDelegate(IntPtr begin, IntPtr end);

    private static unsafe void PatchMethod(MethodInfo target, MethodInfo replacement)
    {
        Architecture architecture = RuntimeInformation.ProcessArchitecture;
        int patchSize = GetPatchSize(architecture);

        RuntimeHelpers.PrepareMethod(target.MethodHandle);
        RuntimeHelpers.PrepareMethod(replacement.MethodHandle);

        IntPtr targetPtr = target.MethodHandle.GetFunctionPointer();
        IntPtr replacementPtr = replacement.MethodHandle.GetFunctionPointer();

        if (OperatingSystem.IsMacOS() && architecture == Architecture.Arm64)
        {
            if (Environment.Version.Major >= 10)
            {
                RedirectMacOSArm64FixupPrecode(targetPtr, replacementPtr);
                return;
            }

            if (AppContext.GetData("PWSH_HOST_NATIVE_JIT_PATCH") is not string address
                || !ulong.TryParse(address, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out ulong value)
                || value == 0)
            {
                throw new InvalidOperationException("The native macOS JIT patch helper is unavailable.");
            }

            byte* patch = stackalloc byte[patchSize];
            WriteJumpPatch(patch, replacementPtr, architecture);
            var nativePatch = (delegate* unmanaged<void*, void*, nuint, void>)(nuint)value;
            nativePatch(targetPtr.ToPointer(), patch, (nuint)patchSize);
            return;
        }

        MakeMethodWritable(targetPtr, patchSize, out uint oldProtect);

        try
        {
            WriteJumpPatch((byte*)targetPtr, replacementPtr, architecture);
            FlushPatchedCode(targetPtr, patchSize, architecture);
        }
        finally
        {
            RestoreMethodProtection(targetPtr, patchSize, oldProtect);
        }
    }

    private static unsafe void RedirectMacOSArm64FixupPrecode(IntPtr targetPtr, IntPtr replacementPtr)
    {
        int pageSize = Environment.SystemPageSize;
        uint* instructions = (uint*)targetPtr;
        uint expectedLoad = 0x5800000B | ((uint)(pageSize / 4) << 5);

        // .NET 10 maps fixup-precode instructions from a read-only template;
        // its corresponding writable target slot is one page after the entry.
        if (pageSize <= 0 || pageSize >= 0x100000 || pageSize % 4 != 0
            || ((nuint)targetPtr & 7) != 0
            || instructions[0] != expectedLoad
            || instructions[1] != 0xD61F0160
            || instructions[2] != 0xD50339BF)
        {
            throw new PlatformNotSupportedException("Unsupported macOS arm64 .NET fixup precode.");
        }

        long* targetSlot = (long*)((byte*)targetPtr + pageSize);
        Interlocked.Exchange(ref *targetSlot, replacementPtr.ToInt64());
    }

    private static int GetPatchSize(Architecture architecture)
    {
        if (IntPtr.Size != 8)
        {
            throw new PlatformNotSupportedException("This startup hook only supports 64-bit processes.");
        }

        return architecture switch
        {
            Architecture.X64 => 12,
            Architecture.Arm64 => 16,
            _ => throw new PlatformNotSupportedException($"This startup hook does not support {architecture} processes."),
        };
    }

    private static unsafe void WriteJumpPatch(byte* site, IntPtr replacementPtr, Architecture architecture)
    {
        switch (architecture)
        {
            case Architecture.X64:
                site[0] = 0x48;
                site[1] = 0xB8;
                *((ulong*)(site + 2)) = unchecked((ulong)replacementPtr.ToInt64());
                site[10] = 0xFF;
                site[11] = 0xE0;
                break;

            case Architecture.Arm64:
                *((uint*)site) = 0x58000050;
                *((uint*)(site + 4)) = 0xD61F0200;
                *((ulong*)(site + 8)) = unchecked((ulong)replacementPtr.ToInt64());
                break;

            default:
                throw new PlatformNotSupportedException($"This startup hook does not support {architecture} processes.");
        }
    }

    private static void MakeMethodWritable(IntPtr targetPtr, int patchSize, out uint oldProtect)
    {
        if (OperatingSystem.IsWindows())
        {
            const uint PageExecuteReadWrite = 0x40;

            if (!VirtualProtect(targetPtr, (nuint)patchSize, PageExecuteReadWrite, out oldProtect))
            {
                throw new InvalidOperationException($"VirtualProtect failed: {Marshal.GetLastWin32Error()}");
            }

            return;
        }

        if (OperatingSystem.IsLinux() || OperatingSystem.IsMacOS())
        {
            oldProtect = 0;

            GetProtectionRange(targetPtr, patchSize, out IntPtr protectionAddress, out nuint protectionLength);
            if (mprotect(protectionAddress, protectionLength, UnixProtectRead | UnixProtectWrite | UnixProtectExecute) != 0)
            {
                throw new InvalidOperationException($"mprotect failed: {Marshal.GetLastWin32Error()}");
            }

            return;
        }

        throw new PlatformNotSupportedException("This startup hook only supports Windows, Linux, and macOS.");
    }

    private static void RestoreMethodProtection(IntPtr targetPtr, int patchSize, uint oldProtect)
    {
        if (OperatingSystem.IsWindows())
        {
            if (!VirtualProtect(targetPtr, (nuint)patchSize, oldProtect, out _))
            {
                throw new InvalidOperationException($"VirtualProtect restore failed: {Marshal.GetLastWin32Error()}");
            }

            return;
        }

        if (OperatingSystem.IsLinux() || OperatingSystem.IsMacOS())
        {
            GetProtectionRange(targetPtr, patchSize, out IntPtr protectionAddress, out nuint protectionLength);
            if (mprotect(protectionAddress, protectionLength, UnixProtectRead | UnixProtectExecute) != 0)
            {
                throw new InvalidOperationException($"mprotect restore failed: {Marshal.GetLastWin32Error()}");
            }

            return;
        }

        throw new PlatformNotSupportedException("This startup hook only supports Windows, Linux, and macOS.");
    }

    private static void GetProtectionRange(IntPtr targetPtr, int patchSize, out IntPtr protectionAddress, out nuint protectionLength)
    {
        ulong pageSize = (ulong)Environment.SystemPageSize;
        ulong targetAddress = unchecked((ulong)targetPtr.ToInt64());
        ulong pageStart = targetAddress & ~(pageSize - 1);
        ulong pageEnd = (targetAddress + (ulong)patchSize + pageSize - 1) & ~(pageSize - 1);

        protectionAddress = new IntPtr(unchecked((long)pageStart));
        protectionLength = (nuint)(pageEnd - pageStart);
    }

    private static void FlushPatchedCode(IntPtr targetPtr, int patchSize, Architecture architecture)
    {
        if (OperatingSystem.IsWindows())
        {
            if (!FlushInstructionCache(GetCurrentProcess(), targetPtr, (nuint)patchSize))
            {
                throw new InvalidOperationException($"FlushInstructionCache failed: {Marshal.GetLastWin32Error()}");
            }

            return;
        }

        if (architecture != Architecture.Arm64)
        {
            return;
        }

        if (OperatingSystem.IsMacOS())
        {
            sys_icache_invalidate(targetPtr, (nuint)patchSize);
            return;
        }

        if (OperatingSystem.IsLinux())
        {
            GetLinuxClearCache()(targetPtr, IntPtr.Add(targetPtr, patchSize));
            return;
        }

        throw new PlatformNotSupportedException("This startup hook only supports Windows, Linux, and macOS.");
    }

    private static LinuxClearCacheDelegate GetLinuxClearCache()
    {
        if (s_linuxClearCache is not null)
        {
            return s_linuxClearCache;
        }

        lock (s_linuxClearCacheLock)
        {
            if (s_linuxClearCache is not null)
            {
                return s_linuxClearCache;
            }

            foreach (string libraryName in LinuxClearCacheLibraryNames)
            {
                if (!NativeLibrary.TryLoad(libraryName, out IntPtr libraryHandle))
                {
                    continue;
                }

                if (NativeLibrary.TryGetExport(libraryHandle, "__clear_cache", out IntPtr functionPointer))
                {
                    s_linuxClearCache = Marshal.GetDelegateForFunctionPointer<LinuxClearCacheDelegate>(functionPointer);
                    return s_linuxClearCache;
                }
            }
        }

        throw new PlatformNotSupportedException("Unable to resolve __clear_cache for Linux arm64 instruction cache invalidation.");
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool VirtualProtect(IntPtr address, nuint size, uint newProtect, out uint oldProtect);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool FlushInstructionCache(IntPtr process, IntPtr baseAddress, nuint size);

    [DllImport("libc", SetLastError = true)]
    private static extern int mprotect(IntPtr address, nuint size, int protection);

    [DllImport(LibSystem)]
    private static extern void sys_icache_invalidate(IntPtr start, nuint length);
}