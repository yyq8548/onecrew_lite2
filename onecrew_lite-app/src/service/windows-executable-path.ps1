param([Parameter(Mandatory=$true)][uint32]$TargetProcessId)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class ExecutableImagePath {
  [DllImport("kernel32.dll", SetLastError=true)] private static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern bool QueryFullProcessImageName(IntPtr process, uint flags, StringBuilder path, ref uint size);
  [DllImport("kernel32.dll", SetLastError=true)] private static extern bool CloseHandle(IntPtr handle);
  public static string Read(uint processId) {
    var handle = OpenProcess(0x1000, false, processId);
    if (handle == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
    try {
      var path = new StringBuilder(32768); uint size = (uint)path.Capacity;
      if (!QueryFullProcessImageName(handle,0,path,ref size)) throw new Win32Exception(Marshal.GetLastWin32Error());
      return path.ToString();
    } finally { if (!CloseHandle(handle)) throw new Win32Exception(Marshal.GetLastWin32Error()); }
  }
}
'@
[Console]::WriteLine(([ExecutableImagePath]::Read($TargetProcessId) | ConvertTo-Json -Compress))
