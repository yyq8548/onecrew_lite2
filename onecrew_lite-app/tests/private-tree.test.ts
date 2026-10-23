import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createPrivateDirectory, verifyPrivateTree } from "../src/storage/private-backup.ts";

test("private tree verification checks every nested ACL and refuses reparse traversal", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/private-tree-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "tree-"));
  try {
    const root = await createPrivateDirectory(join(directory, "private"));
    const child = await createPrivateDirectory(join(root, "child"));
    await writeFile(join(child, "data"), "private", { flag: "wx", mode: 0o600 });
    await verifyPrivateTree(root);
    const outside = join(directory, "outside"); await mkdir(outside);
    await symlink(outside, join(root, "link"), process.platform === "win32" ? "junction" : "dir");
    await assert.rejects(verifyPrivateTree(root), /reparse|symbolic/i);
    await rm(join(root, "link"));
    if (process.platform === "win32") {
      await mkdir(join(root, "inherited"));
      await assert.rejects(verifyPrivateTree(root), /inherits permissions/);
    }
  } finally { await rm(directory, { recursive: true }); }
});
