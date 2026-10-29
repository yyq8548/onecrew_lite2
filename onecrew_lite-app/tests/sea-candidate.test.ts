import { test } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { load } from "cheerio";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { readVerifiedRuntime } from "../src/service/runtime.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { parsePayloadManifest, payloadDigest } from "../src/packaging/payload-manifest.ts";

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
const execute = promisify(execFile);
test("real unified SEA cold and warm launches serve both UIs, native SQLite/PGlite and reject cache corruption", { skip: !executable, timeout: 240_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `sea-acceptance-${randomUUID()}`)), directory = join(root, "data");
  const binary = resolve(executable!), manifest = parsePayloadManifest(await readFile(join(dirname(binary), "runtime-manifest.json")));
  for (const resource of ["node_modules/@mtcute/wasm/mtcute.wasm", "node_modules/@mtcute/wasm/mtcute-simd.wasm", "resources/workflow-guides.json", "resources/mcp-current-registry.json", "resources/browser/browser-profile-acl.ps1"]) assert.ok(manifest.files.some(file => file.path === resource), resource);
  const archive = parseApplicationJson(await readFile(join(dirname(binary), "archive.json"))) as { path: string; bytes: number; sha256: string };
  const archiveBytes = await readFile(join(dirname(binary), archive.path)); assert.equal(archiveBytes.length, archive.bytes); assert.equal(payloadDigest(archiveBytes), archive.sha256);
  const cacheParent = process.platform === "darwin" ? join(root, "Library", "Caches") : root;
  if (process.platform === "darwin") await mkdir(cacheParent, { recursive: true, mode: 0o700 });
  const env = { ...process.env, LOCALAPPDATA: root, ...(process.platform === "darwin" ? { HOME: root } : {}) };
  async function invoke(command: string) {
    const result = await execute(binary, [command, "--data-dir", directory, ...(command === "start" ? ["--vault", join(directory, "vault.sqlite"), "--master-key", join(directory, "masterkey")] : [])], { env, windowsHide: true, timeout: 120_000, maxBuffer: 64 * 1024 });
    assert.ok(!result.stdout.includes("control_token") && !result.stdout.includes("Bearer"));
    return parseApplicationJson(Buffer.from(result.stdout)) as Record<string, unknown>;
  }
  let running = false;
  try {
    const coldAt = performance.now(), first = await invoke("start"), coldCliReadyMilliseconds = performance.now() - coldAt; running = true;
    assert.equal(first.state, "running"); assert.equal(first.production_ready, false);
    const warmAt = performance.now(); assert.equal((await invoke("start")).instance_id, first.instance_id);
    const warmExistingCliMilliseconds = performance.now() - warmAt;
    assert.equal((await invoke("status")).instance_id, first.instance_id);
    const runtime = await readVerifiedRuntime(directory), headers = { authorization: `Bearer ${runtime.control_token}` };
    const status = await (await fetch(`${runtime.url}/_local/status`, { headers })).json();
    const memory = process.platform === "win32" ? parseApplicationJson((await execute(join(process.env.SystemRoot!, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./sea-process-metrics.ps1", import.meta.url)), "-ProcessId", String(runtime.pid)], { windowsHide: true, timeout: 15_000 })).stdout) : null;
    assert.equal(status.resource_identity.kind, "sea_candidate");
    assert.equal(status.resource_identity.executable_sha256, payloadDigest(await readFile(binary)));
    assert.equal(status.resource_identity.source_tree_sha256, manifest.source.sourceTreeSha256);
    assert.equal((await (await fetch(`${runtime.url}/_local/access/status`, { headers })).json()).state, "signed_out");
    assert.equal((await (await fetch(`${runtime.url}/_local/library/status`, { headers })).json()).ready, true);
    for (const path of ["/", "/library"]) {
      const page = await fetch(runtime.url + path); assert.equal(page.status, 200);
      const asset = load(await page.text())("script[src]").first().attr("src"); assert.ok(asset);
      if (path === "/library") assert.ok(asset.startsWith("/library/assets/"));
      assert.equal((await fetch(new URL(asset, runtime.url))).status, 200);
    }
    const mcp = new Client({ name: "sea-offline-acceptance", version: "1" });
    const transport = new StdioClientTransport({ command: binary, args: ["mcp", "--data-dir", directory], env: Object.fromEntries(Object.entries(env).filter((entry): entry is [string, string] => entry[1] !== undefined)), stderr: "pipe" });
    try {
      await mcp.connect(transport);
      const resource = (await mcp.listResources()).resources[0]; assert.ok(resource);
      const content = (await mcp.readResource({ uri: resource.uri })).contents[0];
      assert.ok(content && "text" in content); assert.match(content.text, /<!doctype html>/iu);
      const result = await mcp.callTool({ name: "onecrew_status", arguments: {} });
      const text = (result.content as { text: string }[])[0]!.text;
      assert.equal((parseApplicationJson(text) as { instance_id: string }).instance_id, runtime.instance_id);
      for (const destination of ["workbench", "library"]) {
        const opened = await mcp.callTool({ name: "onecrew_workspace_open", arguments: { destination } });
        assert.equal(opened.isError, undefined);
        const data = opened.structuredContent as { url: string; instance_id: string };
        assert.equal(data.instance_id, runtime.instance_id);
        const url = new URL(data.url);
        assert.equal(url.pathname, destination === "library" ? "/library" : "/");
        const exchanged = await fetch(`${url.origin}/api/local/session`, { method: "POST", headers: { origin: url.origin, "content-type": "application/json" }, body: JSON.stringify({ ticket: new URLSearchParams(url.hash.slice(1)).get("onecrew_ticket") }) });
        assert.equal(exchanged.status, 200);
        const cookie = exchanged.headers.get("set-cookie")!.split(";")[0]!;
        assert.equal((await fetch(`${url.origin}/api/session`, { headers: { cookie } })).status, 200);
      }
    } finally { await mcp.close(); await transport.close(); }
    assert.equal((await invoke("stop")).state, "stopped"); running = false;
    const restartAt = performance.now(), next = await invoke("start"), warmRestartCliReadyMilliseconds = performance.now() - restartAt; running = true;
    assert.equal(next.port, first.port); assert.notEqual(next.instance_id, first.instance_id);
    assert.equal((await invoke("stop")).state, "stopped"); running = false;
    const cache = join(cacheParent, "OneCrewLite", "cache", "runtime", manifest.payloadSha256);
    await writeFile(join(cache, "app", "cli.mjs"), "corrupted", { mode: 0o600 });
    await assert.rejects(invoke("status"), /package.cache_file_invalid|package.cache_hash_mismatch/u);
    await writeFile(join(scratch, `sea-acceptance-${manifest.payloadSha256.slice(0, 16)}-${randomUUID().slice(0, 8)}.json`), JSON.stringify({ productVersion: manifest.productVersion, target: manifest.target, payloadSha256: manifest.payloadSha256, executableBytes: (await readFile(binary)).byteLength, archiveBytes: archive.bytes, archiveSha256: archive.sha256, payloadFiles: manifest.files.length,
      coldCliReadyMilliseconds, warmExistingCliMilliseconds, warmRestartCliReadyMilliseconds, serviceProcessMemory: memory, originalSameDataPerformanceComparison: "not_measured", productionReady: false, checks: ["cold-private-extraction", "warm-full-payload-hashes", "native-instance-lock", "real-vault-and-sqlite", "real-pglite", "real-telegram-wasm-initialization", "zip-hash-and-size", "workbench-html-and-js", "library-html-and-js", "stdio-mcp-current-card", "same-instance-status", "cache-corruption-rejected"] }), { flag: "wx", mode: 0o600 });
  } finally {
    if (running) await invoke("stop");
    await rm(root, { recursive: true });
  }
});
