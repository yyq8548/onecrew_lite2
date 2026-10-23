import assert from "node:assert/strict";
import { test } from "node:test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { readVerifiedRuntime } from "../src/service/runtime.ts";

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
const execute = promisify(execFile);
test("Windows service remains reachable after the launching application's real kill-on-close Job exits", { skip: process.platform !== "win32" || !executable, timeout: 180_000 }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/windows-parent-", import.meta.url)));
  const directory = await createPrivateDirectory(join(root, "data"));
  const configuration = join(root, "launch.json"), ready = join(root, "ready.json");
  await writeFile(configuration, JSON.stringify({ executable: resolve(executable!), arguments: ["start", "--data-dir", directory, "--vault", join(directory, "vault.sqlite"), "--master-key", join(directory, "masterkey")], ready, gate: join(root, "start.gate") }), { mode: 0o600 });
  let alive = false;
  try {
    const closed = await execute(join(process.env.SystemRoot!, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./windows-job-owner.ps1", import.meta.url)), "-Configuration", configuration], { windowsHide: true, timeout: 140_000 });
    assert.match(closed.stdout, /parent_job_closed/u);
    console.log(closed.stdout.trim());
    const started = JSON.parse(await readFile(ready, "utf8")); assert.equal(started.state, "running");
    await delay(1000);
    const current = await readVerifiedRuntime(directory); alive = true;
    assert.equal(current.instance_id, started.instance_id);
    const response = await fetch(current.url + "/_local/status", { headers: { authorization: `Bearer ${current.control_token}` }, signal: AbortSignal.timeout(5000) });
    assert.equal(response.status, 200);
  } finally {
    if (alive) await execute(resolve(executable!), ["stop", "--data-dir", directory], { windowsHide: true, timeout: 120_000 });
  }
});
