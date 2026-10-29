import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { constants } from "node:fs";
import { lstat, mkdir, open, readdir, realpath, rename } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { promisify } from "node:util";
import { parsePayloadManifest, type PayloadManifest } from "./payload-manifest.ts";

const execute = promisify(execFile);
const digest = (value: Uint8Array) => createHash("sha256").update(value).digest("hex");

export class PayloadPermissions {
  readonly #script: Uint8Array;
  constructor(script: Uint8Array) { this.#script = Buffer.from(new TextDecoder("utf-8", { fatal: true }).decode(script)); }
  async #windows(operation: "Create" | "CreateTree" | "Verify" | "VerifyTree", directory: string, directories?: readonly string[]): Promise<void> {
    if (!process.env.SystemRoot) throw new Error("package.windows_system_root_missing");
    // 引导脚本取自已核验的内嵌资源；路径只作为环境参数传入。
    const command = "& ([scriptblock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:ONECREW_BOOTSTRAP_SCRIPT)))) -Operation $env:ONECREW_BOOTSTRAP_OPERATION -Directory $env:ONECREW_BOOTSTRAP_DIRECTORY -TreeManifest $env:ONECREW_BOOTSTRAP_TREE";
    const result = await execute(join(process.env.SystemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", Buffer.from(command, "utf16le").toString("base64")], {
      windowsHide: true, timeout: 120_000, maxBuffer: 64 * 1024,
      env: { ...process.env, ONECREW_BOOTSTRAP_SCRIPT: Buffer.from(this.#script).toString("base64"), ONECREW_BOOTSTRAP_OPERATION: operation, ONECREW_BOOTSTRAP_DIRECTORY: directory, ONECREW_BOOTSTRAP_TREE: JSON.stringify(directories ?? []) },
    });
    if (result.stdout.trim() !== "private-acl-verified") throw new Error("package.private_acl_failed");
  }
  async #unix(path: string, requirePrivate = true): Promise<void> {
    for (let current = resolve(path);;) {
      const item = await lstat(current);
      if (!item.isDirectory() || item.isSymbolicLink()) throw new Error("package.unsafe_cache_ancestor");
      const parent = dirname(current); if (parent === current) break; current = parent;
    }
    if (requirePrivate) {
      const item = await lstat(path);
      if (!process.getuid || item.uid !== process.getuid() || (item.mode & 0o077) !== 0) throw new Error("package.cache_permissions_invalid");
    }
  }
  async verify(path: string): Promise<void> {
    if (process.platform === "win32") await this.#windows("Verify", path);
    else await this.#unix(path);
  }
  async create(path: string, directories: readonly string[] = []): Promise<void> {
    if (process.platform === "win32") await this.#windows(directories.length ? "CreateTree" : "Create", path, directories);
    else {
      await this.#unix(dirname(path), false);
      await mkdir(path, { mode: 0o700 });
      for (const directory of directories) await mkdir(join(path, ...directory.split("/")), { mode: 0o700 });
      await this.verify(path);
    }
  }
  async verifyTree(path: string, ancestors: readonly string[] = []): Promise<void> {
    if (process.platform === "win32") await this.#windows("VerifyTree", path, ancestors);
    else {
      for (const ancestor of ancestors) await this.#unix(ancestor);
      await this.#unix(path);
      for (const child of await readdir(path, { withFileTypes: true })) {
        const target = join(path, child.name);
        if (child.isDirectory()) await this.verifyTree(target);
        else {
          const item = await lstat(target);
          if (!item.isFile() || item.isSymbolicLink() || !process.getuid || item.uid !== process.getuid() || (item.mode & 0o077) !== 0) throw new Error("package.cache_file_permissions_invalid");
        }
      }
    }
  }
}

async function readStable(path: string, expectedSize?: number): Promise<Uint8Array> {
  const before = await lstat(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size > 512n * 1024n * 1024n || expectedSize !== undefined && before.size !== BigInt(expectedSize)) throw new Error("package.cache_file_invalid");
  await using handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await handle.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino) throw new Error("package.cache_file_changed");
  const bytes = await handle.readFile(), after = await handle.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || current.dev !== opened.dev || current.ino !== opened.ino || current.isSymbolicLink()) throw new Error("package.cache_file_changed");
  return bytes;
}

export async function verifyPayloadCache(root: string, manifest: PayloadManifest, permissions: PayloadPermissions, ancestors: readonly string[] = []): Promise<void> {
  await permissions.verifyTree(root, ancestors);
  const actual: string[] = [];
  const allowedDirectories = new Set<string>();
  for (const file of manifest.files) { const parts = file.path.split("/"); parts.pop(); while (parts.length) { allowedDirectories.add(parts.join("/")); parts.pop(); } }
  async function visit(directory: string, prefix: string): Promise<void> {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const name = prefix + entry.name, path = join(directory, entry.name);
      if (entry.isDirectory()) { if (!allowedDirectories.has(name)) throw new Error("package.cache_inventory_mismatch"); await visit(path, name + "/"); }
      else if (entry.isFile()) actual.push(name);
      else throw new Error("package.cache_special_entry");
    }
  }
  await visit(root, "");
  const expected = [...manifest.files.map(file => file.path), "runtime-manifest.json", ".complete"].sort();
  if (JSON.stringify(actual.sort()) !== JSON.stringify(expected)) throw new Error("package.cache_inventory_mismatch");
  const stored = parsePayloadManifest(await readStable(join(root, "runtime-manifest.json")));
  if (JSON.stringify(stored) !== JSON.stringify(manifest) || Buffer.from(await readStable(join(root, ".complete"))).toString("utf8") !== manifest.payloadSha256 + "\n") throw new Error("package.cache_manifest_mismatch");
  for (const file of manifest.files) if (digest(await readStable(join(root, ...file.path.split("/")), file.size)) !== file.sha256) throw new Error("package.cache_hash_mismatch");
}

export async function materializePayload(manifest: PayloadManifest, readAsset: (path: string) => Uint8Array, base: string, permissions: PayloadPermissions, ancestors: readonly string[] = [base]): Promise<string> {
  const target = join(base, manifest.payloadSha256);
  async function verifiedRoot(): Promise<string> {
    const marker = join(target, ".complete");
    if (Buffer.from(await readStable(marker, manifest.payloadSha256.length + 1)).toString("utf8") !== manifest.payloadSha256 + "\n") throw new Error("package.cache_manifest_mismatch");
    // Windows 桌面宿主可能只重定向文件；以真实完成标记确定物理目录，再核验整份资源。
    const physical = dirname(await realpath(marker));
    await verifyPayloadCache(physical, manifest, permissions, ancestors);
    return physical;
  }
  const existing = await lstat(target).then(item => item, error => { if (error.code === "ENOENT") return undefined; throw error; });
  if (existing) {
    if (!existing.isDirectory() || existing.isSymbolicLink()) throw new Error("package.unsafe_cache_ancestor");
    return verifiedRoot();
  }
  await permissions.verify(base);
  const stage = join(base, `.candidate-${randomUUID()}`), directories = new Set<string>();
  for (const file of manifest.files) {
    const parts = file.path.split("/"); parts.pop();
    while (parts.length) { directories.add(parts.join("/")); parts.pop(); }
  }
  const ordered = [...directories].sort((a, b) => a.split("/").length - b.split("/").length || Buffer.compare(Buffer.from(a), Buffer.from(b)));
  await permissions.create(stage, ordered);
  for (const file of manifest.files) {
    const bytes = readAsset(file.path);
    if (bytes.byteLength !== file.size || digest(bytes) !== file.sha256) throw new Error("package.embedded_asset_mismatch");
    await using output = await open(join(stage, ...file.path.split("/")), "wx", file.executable ? 0o700 : 0o600);
    await output.writeFile(bytes); await output.sync();
  }
  for (const [name, value] of [["runtime-manifest.json", JSON.stringify(manifest)], [".complete", manifest.payloadSha256 + "\n"]] as const) {
    await using output = await open(join(stage, name), "wx", 0o600); await output.writeFile(value); await output.sync();
  }
  await verifyPayloadCache(stage, manifest, permissions);
  await rename(stage, target).then(undefined, async error => {
    if (error.code !== "EEXIST" && error.code !== "ENOTEMPTY" && error.code !== "EPERM") throw error;
    await verifyPayloadCache(target, manifest, permissions);
  });
  return verifiedRoot();
}
