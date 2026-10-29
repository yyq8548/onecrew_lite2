import { getAsset, isSea } from "node:sea";
import { lstat, realpath } from "node:fs/promises";
import { delimiter, join, toNamespacedPath } from "node:path";
import { homedir } from "node:os";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { parsePayloadManifest } from "./payload-manifest.ts";
import { materializePayload, PayloadPermissions } from "./payload-cache.ts";

if (!isSea()) throw new Error("package.sea_required");
if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write("[DEBUG-win099] bootstrap-start\n");
const manifest = parsePayloadManifest(new Uint8Array(getAsset("runtime-manifest.json")));
if (manifest.target !== `${process.platform}-${process.arch}` || manifest.nodeVersion !== process.version) throw new Error("package.platform_mismatch");
const script = new Uint8Array(getAsset("resources/storage/private-backup.ps1"));
if (createHash("sha256").update(script).digest("hex") !== manifest.files.find(file => file.path === "resources/storage/private-backup.ps1")?.sha256) throw new Error("package.bootstrap_resource_mismatch");
const permissions = new PayloadPermissions(script);
let executablePath = process.execPath;
if (process.platform === "win32") {
  executablePath = toNamespacedPath(await realpath(toNamespacedPath(executablePath)));
}
const parent = process.platform === "win32" ? process.env.LOCALAPPDATA : process.platform === "darwin" ? join(homedir(), "Library", "Caches") : undefined;
if (!parent) throw new Error("package.cache_platform_unsupported");
let directory = parent;
for (const part of ["OneCrewLite", "cache", "runtime"]) {
  directory = join(directory, part);
  if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write(`[DEBUG-win099] cache-directory ${part}\n`);
  const exists = await lstat(directory).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
  if (exists) await permissions.verify(directory); else await permissions.create(directory);
}
if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write("[DEBUG-win099] materialize-start\n");
const payloadRoot = await realpath(await materializePayload(manifest, path => new Uint8Array(getAsset(path)), directory, permissions));
if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write("[DEBUG-win099] materialize-complete\n");
await permissions.verify(payloadRoot);
if (process.platform === "win32") process.env.PATH = `${join(payloadRoot, "runtime")}${delimiter}${process.env.PATH ?? ""}`;
Object.defineProperty(globalThis, Symbol.for("onecrew_lite.verified_payload"), { value: Object.freeze({ payloadRoot, payloadSha256: manifest.payloadSha256, executablePath }), writable: false, configurable: false });
const loader = join(payloadRoot, "app", "runtime-loader.cjs");
if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write("[DEBUG-win099] runtime-load\n");
const cli = await createRequire(loader)(loader);
if (process.env.ONECREW_LITE_DIAGNOSTICS === "1") process.stderr.write("[DEBUG-win099] cli-loaded\n");
await cli.runCliMain(process.argv.slice(2));
