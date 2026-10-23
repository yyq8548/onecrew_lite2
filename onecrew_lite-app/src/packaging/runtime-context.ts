import { getAsset, isSea } from "node:sea";
import { lstatSync, realpathSync } from "node:fs";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parsePayloadManifest, payloadPathSchema, type PayloadManifest } from "./payload-manifest.ts";

const contextKey = Symbol.for("onecrew_lite.verified_payload");
let cached: Readonly<{ payloadRoot: string; manifest: PayloadManifest; executablePath: string }> | undefined;
function freeze<T>(value: T): T {
  if (value !== null && typeof value === "object") { for (const child of Object.values(value)) freeze(child); Object.freeze(value); }
  return value;
}

export function getPackagedRuntime(): Readonly<{ payloadRoot: string; manifest: PayloadManifest; executablePath: string }> | undefined {
  if (!isSea()) return undefined;
  if (cached) return cached;
  const supplied = Reflect.get(globalThis, contextKey) as unknown;
  if (!supplied || typeof supplied !== "object" || !("payloadRoot" in supplied) || !("payloadSha256" in supplied) || typeof supplied.payloadRoot !== "string" || !("executablePath" in supplied) || typeof supplied.executablePath !== "string" || !isAbsolute(supplied.executablePath)) throw new Error("package.verified_bootstrap_required");
  const manifest = parsePayloadManifest(new Uint8Array(getAsset("runtime-manifest.json")));
  const root = resolve(supplied.payloadRoot), info = lstatSync(root);
  if (supplied.payloadSha256 !== manifest.payloadSha256 || manifest.target !== `${process.platform}-${process.arch}` || manifest.nodeVersion !== process.version ||
      !info.isDirectory() || info.isSymbolicLink() || relative(root, realpathSync(root)) !== "") throw new Error("package.runtime_identity_mismatch");
  cached = freeze({ payloadRoot: root, manifest, executablePath: supplied.executablePath });
  return cached;
}

export function runtimeExecutablePath(): string { return getPackagedRuntime()?.executablePath ?? process.execPath; }

export function runtimeAssetPath(resource: string, sourceUrl: URL): string {
  const runtime = getPackagedRuntime();
  if (!runtime) return fileURLToPath(sourceUrl);
  const path = `resources/${payloadPathSchema.parse(resource)}`;
  if (!runtime.manifest.files.some(file => file.path === path || file.path.startsWith(path + "/"))) throw new Error("package.runtime_resource_missing");
  return join(runtime.payloadRoot, ...path.split("/"));
}

export function packagedFingerprintPaths(): readonly string[] | undefined {
  const runtime = getPackagedRuntime();
  return runtime ? [runtime.executablePath, ...runtime.manifest.files.map(file => join(runtime.payloadRoot, ...file.path.split("/"))), join(runtime.payloadRoot, "runtime-manifest.json"), join(runtime.payloadRoot, ".complete")] : undefined;
}
