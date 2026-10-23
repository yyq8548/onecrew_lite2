import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("real processes cannot share a data owner and an OS-released crash lock can be acquired", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-lock-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "owner-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const child = spawn(process.execPath, [fileURLToPath(new URL("./fixtures/instance-owner.ts", import.meta.url)), directory], {
    stdio: ["ignore", "ignore", "inherit", "ipc"], windowsHide: true,
  });
  try {
    const result = await Promise.race([
      once(child, "message"),
      once(child, "exit").then(([code]) => { throw new Error(`Owner process exited before readiness: ${code}`); }),
    ]);
    assert.deepEqual(result[0], { state: "owned" });
    await assert.rejects(acquireInstanceLock(directory), { code: "runtime.already_owned" });
    const exited = once(child, "exit");
    child.kill("SIGKILL");
    await exited;
    const lease = await acquireInstanceLock(directory);
    await lease.close();
  } finally {
    if (child.exitCode === null && child.signalCode === null) {
      const exited = once(child, "exit");
      child.kill("SIGKILL");
      await exited;
    }
    await rm(root, { recursive: true });
  }
});
