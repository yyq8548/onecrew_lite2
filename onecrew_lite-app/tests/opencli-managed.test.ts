import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import JSZip from "jszip";
import { MANAGED_NODE_VERSION, NODE_DISTRIBUTIONS, repairManagedOpenCli, verifyNodeArchiveChecksum } from "../src/platforms/opencli/managed-install.ts";
import { extractNodeZip } from "../src/platforms/opencli/node-archive.ts";

test("托管安装固定Node版本及三平台SHA256，未授权不执行安装", async () => {
  assert.equal(MANAGED_NODE_VERSION, "24.21.0");
  assert.deepEqual(Object.fromEntries(Object.entries(NODE_DISTRIBUTIONS).map(([platform, value]) => [platform, value.sha256])), {
    "win-x64": "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541",
    "darwin-x64": "1462cb3b3046b815cf8ea436d3da450ec1a9f11dac7e5a46b0ada5305d7e8097",
    "darwin-arm64": "bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057",
  });
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
