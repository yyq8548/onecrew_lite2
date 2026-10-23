import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { createPayloadManifest, parsePayloadManifest, payloadDigest } from "../src/packaging/payload-manifest.ts";
import { materializePayload, PayloadPermissions, verifyPayloadCache } from "../src/packaging/payload-cache.ts";

test("candidate payload private extraction verifies every warm-cache file and rejects additions", async () => {
  const script = await readFile(new URL("../src/storage/private-backup.ps1", import.meta.url));
  const permissions = new PayloadPermissions(script);
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = join(scratch, `payload-cache-${randomUUID()}`);
  await permissions.create(directory);
  const assets = new Map<string, Uint8Array>([
    ["app/cli.mjs", Buffer.from("export const component = 'onecrew_lite';\n")],
    ["resources/web/index.html", Buffer.from("<!doctype html><title>Workbench fixture</title>")],
    ["resources/library-web/index.html", Buffer.from("<!doctype html><title>Library fixture</title>")],
    ["resources/storage/private-backup.ps1", script],
    ["build/pnpm-lock.yaml", await readFile(new URL("../../pnpm-lock.yaml", import.meta.url))],
    ["licenses/semver.txt", await readFile(new URL("../node_modules/semver/LICENSE", import.meta.url))],
  ]);
  const manifest = createPayloadManifest({ format: 1, product: "onecrew_lite", state: "candidate", productVersion: "0.7.1-ts.candidate.1", target: "win32-x64", nodeVersion: "v26.7.0",
    source: { baselineRepository: "https://github.com/yyq8548/onecrew_lite.git", baselineCommit: "efc3c6eb69507b9809bb30279d9af26156d075e3", checkoutCommit: "efc3c6eb69507b9809bb30279d9af26156d075e3", sourceTreeSha256: payloadDigest("test-fixture"), dirty: true },
    lockfileSha256: payloadDigest(assets.get("build/pnpm-lock.yaml")!), components: [{ name: "semver", version: "7.8.5", license: "ISC", licenseFiles: ["licenses/semver.txt"] }],
    files: [...assets].sort(([a], [b]) => Buffer.compare(Buffer.from(a), Buffer.from(b))).map(([path, bytes]) => ({ path, size: bytes.byteLength, sha256: payloadDigest(bytes), executable: false })),
  });
  try {
    assert.deepEqual(parsePayloadManifest(Buffer.from(JSON.stringify(manifest))), manifest);
    const root = await materializePayload(manifest, path => assets.get(path)!, directory, permissions);
    assert.equal(await materializePayload(manifest, path => assets.get(path)!, directory, permissions), root);
    await writeFile(join(root, "unexpected.txt"), "unlisted", { flag: "wx", mode: 0o600 });
    await assert.rejects(verifyPayloadCache(root, manifest, permissions), /inventory_mismatch/u);
    await rm(join(root, "unexpected.txt"));
    await writeFile(join(root, "app", "cli.mjs"), "changed", { mode: 0o600 });
    await assert.rejects(materializePayload(manifest, path => assets.get(path)!, directory, permissions), /cache_file_invalid|cache_hash_mismatch/u);
  } finally { await rm(directory, { recursive: true }); }
});
