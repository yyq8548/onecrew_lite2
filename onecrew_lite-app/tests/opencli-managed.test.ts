import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import JSZip from "jszip";
import { MANAGED_NODE_VERSION, NODE_DISTRIBUTIONS, repairManagedOpenCli, verifyNodeArchiveChecksum } from "../src/platforms/opencli/managed-install.ts";
import { extractNodeZip } from "../src/platforms/opencli/node-archive.ts";

test("托管安装保留原固定Node版本及三平台SHA256，未授权不执行安装", async () => {
  const original = readFileSync(new URL("../../onecrew_lite-rust/crates/opencli/src/managed_installation.rs", import.meta.url), "utf8");
  assert.equal(MANAGED_NODE_VERSION, "24.21.0");
  for (const distribution of Object.values(NODE_DISTRIBUTIONS)) assert.equal(original.includes(distribution.sha256), true);
  assert.throws(() => verifyNodeArchiveChecksum(Buffer.from("not a Node distribution"), "win-x64"), /checksum_failed/);
  await assert.rejects(repairManagedOpenCli(false), /approval_required/);
});
test("真实ZIP解析只解包固定Node根目录，拒绝符号链接与其他根", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/opencli-managed-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const directory = await mkdtemp(join(scratch, "archive-"));
  const root = `node-v${MANAGED_NODE_VERSION}-win-x64`;
  const archive = new JSZip(); archive.file(`${root}/LICENSE`, "Node license contract");
  const path = join(directory, "node.zip"); await writeFile(path, await archive.generateAsync({ type: "nodebuffer", platform: "UNIX" }));
  const extracted = join(directory, "output"); await mkdir(extracted);
  await extractNodeZip(path, extracted, root);
  assert.equal(await readFile(join(extracted, root, "LICENSE"), "utf8"), "Node license contract");
  const link = new JSZip(); link.file(`${root}/escape`, "../outside", { unixPermissions: 0o120777 });
  const linkPath = join(directory, "link.zip"); await writeFile(linkPath, await link.generateAsync({ type: "nodebuffer", platform: "UNIX" }));
  await assert.rejects(extractNodeZip(linkPath, join(directory, "link-output"), root), /unsafe/);
  await assert.rejects(extractNodeZip(path, join(directory, "other-output"), "wrong-root"), /unsafe/);
});
