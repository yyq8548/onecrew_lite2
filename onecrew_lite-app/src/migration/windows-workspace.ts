import { createHash } from "node:crypto";
import { constants, lstatSync, realpathSync, type BigIntStats } from "node:fs";
import { lstat, open, readdir, realpath } from "node:fs/promises";
import { basename, dirname, join, relative, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
import type { LegacyWindowsRegistration } from "./windows-registration.ts";

const sha = z.string().regex(/^[a-f0-9]{64}$/u);
const entry = z.object({ path: z.string().min(1), size: z.number().int().positive().max(Number.MAX_SAFE_INTEGER), sha256: sha }).strict();
const workspace = z.object({ schema_version: z.literal(1), product: z.literal("onecrew_lite-workspace"), version: z.literal("0.7.1"),
  target: z.literal("x86_64-pc-windows-msvc"), library_version: z.string().regex(/^\d+\.\d+\.\d+$/u), files: z.array(entry).min(1) }).strict();
const issuer = Symbol("verified-original-windows-workspace");
const equalPath = (a: string, b: string) => relative(resolve(a), resolve(b)) === "";

async function hashFile(path: string) {
  const before = await lstat(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || !equalPath(path, await realpath(path))) throw new Error("migration.workspace_path_invalid");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.ino !== before.ino || opened.dev !== before.dev) throw new Error("migration.workspace_file_changed");
  const hash = createHash("sha256");
  for await (const bytes of file.createReadStream({ autoClose: false })) hash.update(bytes);
  const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (current.isSymbolicLink() || current.ino !== opened.ino || current.dev !== opened.dev || current.nlink !== 1n || after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs) throw new Error("migration.workspace_file_changed");
  return { size: Number(opened.size), sha256: hash.digest("hex"), stat: opened };
}
async function jsonFile(path: string) {
  if ((await lstat(path)).size > 8 * 1024 * 1024) throw new Error("migration.workspace_manifest_limit");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  return parseApplicationJson(await file.readFile());
}
function safeRelative(path: string): string {
  if (path.includes("\\") || path.includes(":") || path.includes("\0") || path.startsWith("/") || path.split("/").some(part => !part || part === "." || part === "..") || ["manifest.json", "release-build.json"].includes(path)) throw new Error("migration.workspace_manifest_path_invalid");
  return path;
}

export class VerifiedLegacyWindowsWorkspace {
  readonly registration: LegacyWindowsRegistration;
  readonly manifestSha256: string;
  readonly #files: ReadonlyMap<string, BigIntStats>;
  constructor(token: symbol, registration: LegacyWindowsRegistration, manifestSha256: string, files: ReadonlyMap<string, BigIntStats>) {
    if (token !== issuer) throw new Error("migration.workspace_authority_unissued");
    this.registration = Object.freeze({ ...registration }); this.manifestSha256 = manifestSha256; this.#files = files; Object.freeze(this);
  }
  assertUnchanged(): void {
    for (const [path, expected] of this.#files) {
      const current = lstatSync(path, { bigint: true });
      if (current.isSymbolicLink() || !equalPath(path, realpathSync(path)) || current.dev !== expected.dev || current.ino !== expected.ino || current.size !== expected.size ||
          current.mtimeNs !== expected.mtimeNs || current.ctimeNs !== expected.ctimeNs || current.mode !== expected.mode || (current.isFile() && current.nlink !== 1n)) throw new Error("migration.workspace_file_changed");
    }
  }
  toJSON(): never { throw new Error("Workspace authority cannot be serialized"); }
}

// 仅接受原 workspace-release.mjs 已发布的 Windows 配套清单；不把单文件 CLI 当完整安装。
export async function verifyLegacyWindowsWorkspace(input: { directory: string; serviceExecutable: string; dataDirectory: string }): Promise<VerifiedLegacyWindowsWorkspace> {
  if (process.platform !== "win32") throw new Error("migration.windows_workspace_platform_unsupported");
  const directory = resolve(input.directory);
  if (!equalPath(directory, await realpath(directory))) throw new Error("migration.workspace_path_invalid");
  if (lstatSync(join(directory, ".onecrew_lite-install"), { throwIfNoEntry: false })) throw new Error("migration.workspace_installation_pending");
  const manifestPath = join(directory, "manifest.json"), manifestIdentity = await hashFile(manifestPath);
  const manifest = workspace.parse(await jsonFile(manifestPath));
  const provenanceIdentity = await hashFile(join(directory, "release-build.json"));
  const provenance = z.object({ schema_version: z.literal(1), node: z.literal("v26.7.0"), plugin: z.literal("embedded-in-cli"), commit: z.string().regex(/^[a-f0-9]{40}$/u),
    version: z.literal("0.7.1"), target: z.literal("x86_64-pc-windows-msvc"), host: z.literal("win32-x64"), manifest_sha256: z.literal(manifestIdentity.sha256) }).passthrough().parse(await jsonFile(join(directory, "release-build.json")));
  if (provenance.target !== manifest.target) throw new Error("migration.workspace_target_mismatch");
  const expectedPaths = manifest.files.map(file => safeRelative(file.path));
  if (new Set(expectedPaths).size !== expectedPaths.length) throw new Error("migration.workspace_duplicate_path");
  const files = new Map<string, BigIntStats>(), actual: string[] = [];
  async function visit(path: string): Promise<void> {
    const stat = await lstat(path, { bigint: true });
    if (!stat.isDirectory() || stat.isSymbolicLink() || !equalPath(path, await realpath(path))) throw new Error("migration.workspace_path_invalid");
    files.set(path, stat);
    for (const child of await readdir(path, { withFileTypes: true })) {
      const full = join(path, child.name);
      if (child.isDirectory()) await visit(full);
      else { if (!child.isFile()) throw new Error("migration.workspace_path_invalid"); actual.push(relative(directory, full).replaceAll("\\", "/")); }
    }
  }
  await visit(directory);
  if (!isDeepStrictEqual(actual.sort(), [...expectedPaths, "manifest.json", "release-build.json"].sort())) throw new Error("migration.workspace_files_mismatch");
  for (const file of manifest.files) {
    const path = join(directory, file.path), identity = await hashFile(path);
    if (identity.size !== file.size || identity.sha256 !== file.sha256) throw new Error("migration.workspace_integrity_mismatch");
    files.set(path, identity.stat);
  }
  const cliEntry = manifest.files.find(file => file.path === "onecrew_lite.exe"), libraryEntry = manifest.files.find(file => file.path === "onecrew_lite_library/onecrew_lite.exe");
  if (!cliEntry || !libraryEntry) throw new Error("migration.workspace_component_missing");
  const cli = z.object({ schema_version: z.literal(1), product: z.literal("onecrew_lite-cli"), version: z.literal("0.7.1"), target: z.literal("x86_64-pc-windows-msvc"),
    sha256: z.literal(cliEntry.sha256), size: z.literal(cliEntry.size) }).passthrough().parse(await jsonFile(join(directory, "workbench-manifest.json")));
  const library = z.object({ version: z.literal(manifest.library_version), target: z.literal("win32-x64"), capabilities: z.array(z.string()), files: z.array(entry) }).passthrough().parse(await jsonFile(join(directory, "onecrew_lite_library", "manifest.json")));
  if (new Set(library.files.map(file => file.path)).size !== library.files.length) throw new Error("migration.workspace_duplicate_path");
  if (cli.target !== manifest.target || !["discovery-contacts-v2", "recommendation-capture-v1"].every(value => library.capabilities.includes(value)) ||
      !library.files.some(file => file.path === "onecrew_lite.exe" && file.size === libraryEntry.size && file.sha256 === libraryEntry.sha256)) throw new Error("migration.workspace_component_mismatch");
  for (const file of library.files) {
    const path = `onecrew_lite_library/${safeRelative(file.path)}`;
    if (!manifest.files.some(row => row.path === path && row.size === file.size && row.sha256 === file.sha256)) throw new Error("migration.workspace_component_mismatch");
  }
  const serviceExecutable = resolve(input.serviceExecutable), service = await hashFile(serviceExecutable);
  if (basename(serviceExecutable) !== "onecrew_lite.exe" || basename(dirname(serviceExecutable)) !== cliEntry.sha256 || basename(dirname(dirname(serviceExecutable))) !== "service-binaries" ||
      basename(dirname(dirname(dirname(serviceExecutable)))) !== "onecrew_lite" || service.sha256 !== cliEntry.sha256 || service.size !== cliEntry.size) throw new Error("migration.workspace_service_image_mismatch");
  files.set(serviceExecutable, service.stat);
  files.set(manifestPath, manifestIdentity.stat);
  files.set(join(directory, "release-build.json"), provenanceIdentity.stat);
  const result = new VerifiedLegacyWindowsWorkspace(issuer, { dataDirectory: resolve(input.dataDirectory), serviceExecutable, packageExecutable: join(directory, "onecrew_lite.exe"), executableSha256: cliEntry.sha256 }, manifestIdentity.sha256, files);
  result.assertUnchanged();
  return result;
}
