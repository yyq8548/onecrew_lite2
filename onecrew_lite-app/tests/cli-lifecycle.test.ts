import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { setTimeout as delay } from "node:timers/promises";
import test from "node:test";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";

const execute = promisify(execFile);
const cli = fileURLToPath(new URL("../src/cli.ts", import.meta.url));

test("real CLI starts one owned unified process, reports redacted status and drains stop", { timeout: 180_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/cli-lifecycle-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "cli-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  async function invoke(command: string) {
    const extra = command === "start" ? ["--vault", join(directory, "vault.sqlite"), "--master-key", join(directory, "master.key")] : [];
    const result = await execute(process.execPath, [cli, command, "--data-dir", directory, ...extra], { windowsHide: true, timeout: 90_000, maxBuffer: 64 * 1024 });
    assert.equal(result.stdout.includes("control_token"), false);
    assert.equal(result.stdout.includes("Bearer"), false);
    return JSON.parse(result.stdout);
  }
  let started = false;
  let completed = false;
  try {
    assert.equal((await invoke("status")).state, "stopped");
    const first = await invoke("start");
    started = true;
    assert.equal(first.state, "running");
    assert.equal(first.production_ready, false);
    assert.equal((await invoke("status")).instance_id, first.instance_id);
    assert.equal((await invoke("start")).instance_id, first.instance_id);
    assert.equal((await invoke("stop")).state, "stopped");
    started = false;
    assert.equal((await invoke("status")).state, "stopped");
    const interrupted = await invoke("start");
    started = true;
    assert.equal(interrupted.port, first.port);
    assert.equal(typeof interrupted.pid, "number");
    process.kill(interrupted.pid, "SIGKILL");
    started = false;
    const deadline = performance.now() + 10_000;
    for (;;) {
      const released = await acquireInstanceLock(directory).then(async lease => { await lease.close(); return true; }, error => {
        if (error && typeof error === "object" && "code" in error && error.code === "runtime.already_owned") return false;
        throw error;
      });
      if (released) break;
      if (performance.now() >= deadline) throw new Error("Killed test process did not release native ownership");
      await delay(100);
    }
    assert.equal((await invoke("status")).state, "stopped");
    const recovered = await invoke("start");
    started = true;
    assert.equal(recovered.port, first.port);
    assert.notEqual(recovered.instance_id, interrupted.instance_id);
    assert.equal((await invoke("stop")).state, "stopped");
    started = false;
    completed = true;
  } finally {
    if (started && (await invoke("status")).state === "running") await invoke("stop");
    if (completed) await rm(root, { recursive: true });
    else console.error(`CLI lifecycle diagnostics retained at ${root}`);
  }
});
