import test from "node:test";
import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { lstat, mkdir, readFile, rm, symlink, unlink, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createPrivateDirectory, verifyPrivateTree } from "../src/storage/private-backup.ts";
import { copyOwnedBrowserProfile } from "../src/platforms/browser/browser-profile-copy.ts";

test("owned browser reconnect copies only private real files and refuses junctions without changing the source", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true }); const root = await createPrivateDirectory(join(scratch, `bp-${randomUUID().slice(0, 12)}`));
  try {
    const source = await createPrivateDirectory(join(root, "original")), child = join(source, "Default"), bytes = randomBytes(32768), file = join(child, "session.fixture"); await mkdir(child, { mode: 0o700 }); await writeFile(file, bytes, { flag: "wx", mode: 0o600 }); const before = await lstat(file, { bigint: true });
    const target = join(root, "reconnect"); await copyOwnedBrowserProfile(source, target, new AbortController().signal); await verifyPrivateTree(target); assert.deepEqual(await readFile(join(target, "Default", "session.fixture")), bytes); const after = await lstat(file, { bigint: true }); assert.equal(after.ino, before.ino); assert.equal(after.mtimeNs, before.mtimeNs); assert.deepEqual(await readFile(file), bytes);
    await symlink(target, join(source, "outside"), process.platform === "win32" ? "junction" : "dir"); await assert.rejects(copyOwnedBrowserProfile(source, join(root, "unsafe-copy"), new AbortController().signal)); await assert.rejects(lstat(join(root, "unsafe-copy")), { code: "ENOENT" }); assert.deepEqual(await readFile(file), bytes);
    if (process.platform === "win32") { await unlink(join(source, "outside")); await promisify(execFile)(join(process.env.SystemRoot!, "System32", "icacls.exe"), [child, "/grant", "*S-1-1-0:(M)"], { windowsHide: true }); await assert.rejects(copyOwnedBrowserProfile(source, join(root, "open-acl-copy"), new AbortController().signal)); await assert.rejects(lstat(join(root, "open-acl-copy")), { code: "ENOENT" }); assert.deepEqual(await readFile(file), bytes); }
  } finally { await rm(root, { recursive: true }); }
});
