import { lstat, open } from "node:fs/promises";
import { lstatSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";
import { verifyPrivateDirectory } from "../storage/private-backup.ts";

const require = createRequire(import.meta.url);
const nativeLock = require("fs-native-extensions") as { tryLock(fd: number): boolean };

// 文件保留，操作系统在进程退出时释放锁；不按 PID 猜测并删除其他实例的锁。
export async function acquireInstanceLock(dataDirectory: string) {
  const directory = await verifyPrivateDirectory(dataDirectory);
  const path = join(directory, "server.lock");
  const existing = lstatSync(path, { throwIfNoEntry: false });
  if (existing && (!existing.isFile() || existing.isSymbolicLink())) throw new Error("Instance lock must be a regular file");
  const handle = await open(path, "a+", 0o600);
  let acquired = false;
  try {
    if (!nativeLock.tryLock(handle.fd)) throw Object.assign(new Error("The data directory already has a running owner"), { code: "runtime.already_owned" });
    const opened = await handle.stat();
    const current = await lstat(path);
    if (current.isSymbolicLink() || current.dev !== opened.dev || current.ino !== opened.ino) throw new Error("Instance lock path changed during acquisition");
    acquired = true;
    let closed = false;
    return {
      directory,
      assertOwned(): void {
        if (closed) throw new Error("Instance ownership has been released");
      },
      async close(): Promise<void> {
        if (closed) return;
        await handle.close();
        closed = true;
      },
    };
  } finally {
    if (!acquired) await handle.close();
  }
}

export type InstanceLease = Awaited<ReturnType<typeof acquireInstanceLock>>;
