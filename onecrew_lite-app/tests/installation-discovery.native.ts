import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, copyFile } from "node:fs/promises";
import { join, resolve, dirname } from "node:path";
import { promisify } from "node:util";
import test from "node:test";

const execute = promisify(execFile);
test("customer installation discovers fresh, legacy, registered custom and ambiguous data", { timeout: 300_000, skip: process.platform !== "win32" }, async () => {
  assert.ok(process.env.ONECREW_DISCOVERY_EXECUTABLE);
  const binary = resolve(process.env.ONECREW_DISCOVERY_EXECUTABLE);
  const previous = resolve(process.env.ONECREW_PREVIOUS_SEA_EXECUTABLE!);
  await mkdir(resolve(".scratch/installation-discovery"), { recursive: true });
  const root = await mkdtemp(resolve(".scratch/installation-discovery/user-"));
  const local = join(root, "本地 应用"), codex = join(root, "codex"), custom = join(root, "自定义 数据");
  await mkdir(local); await mkdir(codex);
  const env = { ...process.env, LOCALAPPDATA: local, CODEX_HOME: codex };
  const invoke = async (program: string, args: string[]) => JSON.parse((await execute(program, args, { env, windowsHide: true, timeout: 120_000 })).stdout);
  const discover = (...args: string[]) => invoke(binary, ["installation-discover", ...args]);
  const first = await discover();
  assert.equal(first.state, "new");
  assert.equal(first.data_directory, join(local, "OneCrewLite", "TypeScript"));
  const install = join(root, "安装目录"); await mkdir(install);
  for (const name of ["onecrew_lite.exe", "candidate.json"]) await copyFile(join(dirname(binary), name), join(install, name));
  await copyFile(resolve("development/windows-customer-package/OneCrew.ps1"), join(install, "OneCrew.ps1"));
  const launch = (args: string[], environment = env) => execute("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", join(install, "OneCrew.ps1"), ...args, "-SkipCodex"], { env: environment, windowsHide: true, timeout: 120_000 });
  const freshBase = join(root, "首次安装"); await mkdir(freshBase);
  const freshEnv = { ...env, LOCALAPPDATA: freshBase };
  await launch(["-Action", "Start"], freshEnv);
  try {
    const fresh = JSON.parse((await execute(binary, ["installation-discover"], { env: freshEnv, windowsHide: true, timeout: 30_000 })).stdout);
    assert.equal(fresh.state, "existing");
    assert.equal(fresh.data_directory, join(freshBase, "OneCrewLite", "TypeScript"));
  } finally { await launch(["-Action", "Stop"], freshEnv); }
  const legacy = join(local, "onecrew_lite", "unified-data");
  await mkdir(dirname(legacy), { recursive: true });
  let running: string | undefined;
  try {
    await invoke(previous, ["start", "--data-dir", legacy, "--vault", join(legacy, "vault.sqlite"), "--master-key", join(legacy, "masterkey")]); running = legacy;
    assert.equal((await discover()).data_directory, legacy);
    await invoke(previous, ["stop", "--data-dir", legacy]); running = undefined;
    assert.equal((await discover()).data_directory, legacy);
    await invoke(previous, ["start", "--data-dir", custom, "--vault", join(custom, "vault.sqlite"), "--master-key", join(custom, "master.key")]); running = custom;
    await assert.rejects(discover(), (error: unknown) => {
      assert.ok(error instanceof Error && "stdout" in error);
      assert.match(String(error.stdout), /multiple_data_directories/);
      return true;
    });
    const selected = await discover("--data-dir", custom);
    assert.equal(selected.master_key_path, join(custom, "master.key"));
    const before = await readFile(selected.master_key_path);
    await launch(["-Action", "Start", "-DataDirectory", custom]);
    assert.deepEqual(await readFile(selected.master_key_path), before);
    assert.equal((await invoke(binary, ["status", "--data-dir", custom])).state, "running");
  } finally { if (running) await invoke(binary, ["stop", "--data-dir", running]); }
});
