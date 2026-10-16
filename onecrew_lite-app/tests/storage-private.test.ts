import assert from "node:assert/strict";
import { mkdtemp, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../src/storage/private-backup.ts";

test("private directory creation is exclusive and rejects reparse ancestors", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-private-", import.meta.url)));
  try {
    const privatePath = join(directory, "private space");
    assert.equal(await createPrivateDirectory(privatePath), privatePath);
    assert.equal(await verifyPrivateDirectory(privatePath), privatePath);
    await writeFile(join(privatePath, "secret"), "synthetic", { mode: 0o600 });
    await verifyPrivateFile(join(privatePath, "secret"));
    await assert.rejects(createPrivateDirectory(privatePath));
    assert.equal(await verifyPrivateDirectory(privatePath), privatePath);
    await assert.rejects(createPrivateDirectory(directory));
    const junction = join(directory, "junction");
    await symlink(privatePath, junction, process.platform === "win32" ? "junction" : "dir");
    await assert.rejects(verifyPrivateDirectory(junction));
    await assert.rejects(createPrivateDirectory(join(junction, "child")));
    assert.deepEqual(await readdir(privatePath), ["secret"]);
  } finally {
    await rm(directory, { recursive: true });
  }
});
