import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readdir } from "node:fs/promises";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { getPackagedRuntime, runtimeExecutablePath } from "../packaging/runtime-context.ts";

async function fileDigest(path: string): Promise<string> {
  const before = await lstat(path, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size > 2048n * 1024n * 1024n) throw new Error("build_identity.invalid_file");
  await using file = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  const opened = await file.stat({ bigint: true });
  if (opened.dev !== before.dev || opened.ino !== before.ino) throw new Error("build_identity.file_changed");
  const hash = createHash("sha256");
  for await (const bytes of file.createReadStream({ autoClose: false })) hash.update(bytes);
  const after = await file.stat({ bigint: true }), current = await lstat(path, { bigint: true });
  if (after.size !== opened.size || after.mtimeNs !== opened.mtimeNs || after.ctimeNs !== opened.ctimeNs || current.dev !== opened.dev || current.ino !== opened.ino || current.isSymbolicLink()) throw new Error("build_identity.file_changed");
  return hash.digest("hex");
}

export async function readDevelopmentBuildIdentity() {
  const packaged = getPackagedRuntime();
  if (packaged) return Object.freeze({ kind: "sea_candidate" as const, executable_sha256: await fileDigest(runtimeExecutablePath()), source_tree_sha256: packaged.manifest.source.sourceTreeSha256, dependency_lock_sha256: packaged.manifest.lockfileSha256 });
  const root = fileURLToPath(new URL("../../../", import.meta.url));
  const hash = createHash("sha256");
  async function addFile(path: string): Promise<void> {
    const identity = await fileDigest(path), name = Buffer.from(relative(root, path).replaceAll("\\", "/")), length = Buffer.alloc(8);
    length.writeBigUInt64BE(BigInt(name.length)); hash.update(length).update(name).update(identity);
  }
  async function visit(directory: string): Promise<void> {
    const info = await lstat(directory);
    if (!info.isDirectory() || info.isSymbolicLink()) throw new Error("build_identity.invalid_directory");
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((a, b) => Buffer.compare(Buffer.from(a.name), Buffer.from(b.name)));
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else await addFile(path);
    }
  }
  await visit(join(root, "onecrew_lite-app", "src"));
  await visit(join(root, "onecrew_lite-contracts", "src"));
  await visit(join(root, "onecrew_lite-library", "dist"));
  for (const directory of ["onecrew_lite-app", "onecrew_lite-contracts", "onecrew_lite-library"]) await addFile(join(root, directory, "package.json"));
  return Object.freeze({ kind: "source_development" as const, executable_sha256: await fileDigest(process.execPath), source_tree_sha256: hash.digest("hex"), dependency_lock_sha256: await fileDigest(join(root, "pnpm-lock.yaml")) });
}

export function dataPathIdentity(paths: { dataDirectory: string; generationRoot: string; vaultPath: string; masterKeyPath: string; tablesDirectory: string }): string {
  return createHash("sha256").update(JSON.stringify([paths.dataDirectory, paths.generationRoot, paths.vaultPath, paths.masterKeyPath, paths.tablesDirectory])).digest("hex");
}
