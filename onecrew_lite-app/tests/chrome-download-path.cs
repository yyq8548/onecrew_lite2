using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class ChromeDownloadPath {
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  private static extern SafeFileHandle CreateFileW(string name, uint access, uint share,
    IntPtr security, uint creation, uint flags, IntPtr template);

  public static int Main(string[] args) {
    using (SafeFileHandle file = CreateFileW(args[0], 0x40010000, 7, IntPtr.Zero, 1, 0x04000100, IntPtr.Zero)) {
      int error = file.IsInvalid ? Marshal.GetLastWin32Error() : 0;
      Console.WriteLine(error);
      return error;
    }
  }
}
