param([Parameter(Mandatory=$true)][string]$Manifest)
$ErrorActionPreference = 'Stop'
try {
    # Task Scheduler terminates the PowerShell action, not necessarily its children.
    # Join a nested job before creating the collector. The handle belongs only to
    # this process; closing it on process death terminates the whole collector tree.
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace RroflowStartup {
    public static class Lifetime {
        [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
            public long ProcessTime, JobTime;
            public uint Flags;
            public UIntPtr MinimumWorkingSet, MaximumWorkingSet;
            public uint ActiveProcesses;
            public UIntPtr Affinity;
            public uint PriorityClass, SchedulingClass;
        }
        [StructLayout(LayoutKind.Sequential)] struct IoCounters {
            public ulong ReadOperations, WriteOperations, OtherOperations;
            public ulong ReadBytes, WriteBytes, OtherBytes;
        }
        [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
            public BasicLimits Basic;
            public IoCounters Io;
            public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
        }
        [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimits limits, uint size);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        static IntPtr job;
        public static void Initialize() {
            job=CreateJobObject(IntPtr.Zero,null);
            if(job==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            var limits=new ExtendedLimits();
            limits.Basic.Flags=0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            if(!SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf(typeof(ExtendedLimits))) ||
               !AssignProcessToJobObject(job,GetCurrentProcess())) {
                int error=Marshal.GetLastWin32Error();CloseHandle(job);job=IntPtr.Zero;
                throw new Win32Exception(error);
            }
            // Intentionally retained until this process exits. No inheritable handle.
        }
    }
}
'@
    [RroflowStartup.Lifetime]::Initialize()
    $install = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
    $binary = Join-Path (Split-Path -Parent $Manifest) 'rroflow.exe'
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $install.sha256) { throw 'Pinned collector changed.' }
    $binding = Join-Path $install.store 'source.json'
    if ((Get-FileHash -LiteralPath $binding -Algorithm SHA256).Hash.ToLowerInvariant() -ne $install.binding_sha256) { throw 'Source binding changed.' }
    & $binary sources managed-run --manifest $Manifest
    exit $LASTEXITCODE
} catch { Write-Error 'RROFLOW startup failed. Inspect sources service status and the local source journal.'; exit 1 }
