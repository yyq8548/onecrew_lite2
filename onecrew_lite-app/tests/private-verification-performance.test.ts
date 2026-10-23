import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createPrivateDirectory, verifyPrivateFile } from "../src/storage/private-backup.ts";

test("Windows 权限检查通过 UTF-8 管道保留中文目录和文件名", { skip: process.platform !== "win32" }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/permission-unicode-", import.meta.url)));
  const directory = await createPrivateDirectory(join(root, "专用浏览器资料"));
  const file = join(directory, "权限检查文件");
  await writeFile(file, "UTF-8", { mode: 0o600 });
  await verifyPrivateFile(file);
});

test("Windows repeated credential permission checks complete without repeated process startup", { skip: process.platform !== "win32", timeout: 90_000 }, async context => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/permission-latency-", import.meta.url)));
  try {
    const directory = await createPrivateDirectory(join(root, "private"));
    const file = join(directory, "credential-check");
    await writeFile(file, "permission probe", { mode: 0o600 });
    await verifyPrivateFile(file);
    const started = performance.now();
    for (let index = 0; index < 8; index++) await verifyPrivateFile(file);
    const elapsed = performance.now() - started;
    context.diagnostic(`Eight real permission checks: ${Math.round(elapsed)} ms`);
    assert.ok(elapsed < 2_000, `Eight real permission checks took ${Math.round(elapsed)} ms`);
    const execute = promisify(execFile);
    const icacls = join(process.env.SystemRoot!, "System32", "icacls.exe");
    await execute(icacls, [file, "/grant", "*S-1-1-0:(R)"], { windowsHide: true });
    await assert.rejects(verifyPrivateFile(file), /DACL|identity|rules/);
    await execute(icacls, [file, "/remove:g", "*S-1-1-0"], { windowsHide: true });
    await verifyPrivateFile(file);
    await rm(file);
    await assert.rejects(verifyPrivateFile(file));
  } finally { await rm(root, { recursive: true }); }
});
