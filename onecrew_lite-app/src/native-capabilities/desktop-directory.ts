import { execFile } from "node:child_process";
import { realpath, stat } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);

// 只读取操作系统的 Desktop 位置；不创建目录，不接受请求传入的输出路径。
export async function getDesktopDirectory(): Promise<string> {
  let output: string;
  if (process.platform === "win32") {
    const systemRoot = process.env.SystemRoot;
    if (!systemRoot) throw new Error("Windows system directory is unavailable");
    const result = await execute(join(systemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), [
      "-NoProfile", "-NonInteractive", "-Command",
      "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)",
    ], { windowsHide: true, encoding: "utf8", timeout: 10_000, maxBuffer: 16 * 1024 });
    output = result.stdout;
  } else if (process.platform === "darwin") {
    const result = await execute("/usr/bin/osascript", ["-l", "JavaScript", "-e",
      "ObjC.import('Foundation'); ObjC.unwrap($.NSFileManager.defaultManager.URLsForDirectoryInDomains($.NSDesktopDirectory, $.NSUserDomainMask).firstObject.path)",
    ], { encoding: "utf8", timeout: 10_000, maxBuffer: 16 * 1024 });
    output = result.stdout;
  } else throw new Error("Desktop export supports Windows and macOS only");
  const path = output.trim();
  if (!path || !isAbsolute(path) || /[\r\n\0]/u.test(path)) throw new Error("Operating system did not return one absolute Desktop directory");
  const directory = await realpath(path);
  if (!(await stat(directory)).isDirectory()) throw new Error("Desktop location is not an existing directory");
  return directory;
}
