import { createRequire } from "node:module";
import { lstatSync } from "node:fs";
import { lstat, open, realpath, type FileHandle } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyLegacyPrivateDirectory, verifyLegacyPrivateFile } from "../storage/legacy-private-source.ts";

const native = createRequire(import.meta.url)("fs-native-extensions") as { tryLock(fd: number): boolean };
export interface LegacySourcePaths { creator: string; vault: string; masterKey: string; libraryDirectory: string | null }
interface HeldLock { path: string; file: FileHandle; dev: bigint; ino: bigint }
const issued = new WeakSet<LegacyQuiescenceLease>();
const issuer = Symbol("legacy-quiescence");
const equalPath = (a: string, b: string) => relative(resolve(a), resolve(b)) === "";
function contains(directory: string, path: string): boolean { const child = relative(directory, path); return !!child && child !== ".." && !child.startsWith(`..${sep}`) && !isAbsolute(child); }

// 持有旧程序实际使用的原生锁直到激活结束；目录标记或 PID 不能替代锁。
export class LegacyQuiescenceLease {
  readonly #owner: InstanceLease;
  readonly #sources: LegacySourcePaths;
  readonly #serviceDirectory: string;
  readonly #locks: readonly HeldLock[];
  #closed = false;
  private constructor(token: symbol, owner: InstanceLease, sources: LegacySourcePaths, locks: readonly HeldLock[], serviceDirectory: string) { if (token !== issuer) throw new Error("migration.legacy_quiescence_untrusted"); this.#owner = owner; this.#sources = Object.freeze({ ...sources }); this.#locks = locks; this.#serviceDirectory = serviceDirectory; issued.add(this); }
  static async acquire(owner: InstanceLease, options: { serviceDirectory: string; sources: LegacySourcePaths }): Promise<LegacyQuiescenceLease> {
    owner.assertOwned();
    const directory = await realpath(await verifyLegacyPrivateDirectory(options.serviceDirectory));
    if (!equalPath(directory, options.serviceDirectory) || equalPath(directory, owner.directory) || contains(directory, owner.directory) || contains(owner.directory, directory)) throw new Error("migration.legacy_data_directory_conflict");
    for (const name of [".onecrew-service-owner.lock", ".onecrew-service.json"]) if (lstatSync(join(directory, name), { throwIfNoEntry: false })) throw new Error("migration.advanced_edition_directory_rejected");
    const sources = { ...options.sources };
    for (const field of ["creator", "vault", "masterKey"] as const) {
      const requested = sources[field];
      await verifyLegacyPrivateFile(requested);
      sources[field] = await realpath(requested);
      if (!equalPath(requested, sources[field]) || !contains(directory, sources[field])) throw new Error("migration.legacy_source_owner_mismatch");
    }
    const paths = [join(directory, ".onecrew_lite-service-owner.lock")];
    if (sources.libraryDirectory !== null) {
      const requested = sources.libraryDirectory;
      sources.libraryDirectory = await realpath(await verifyLegacyPrivateDirectory(requested));
      if (!equalPath(requested, sources.libraryDirectory) || equalPath(sources.libraryDirectory, owner.directory) || contains(owner.directory, sources.libraryDirectory)) throw new Error("migration.legacy_library_directory_conflict");
      paths.push(join(sources.libraryDirectory, "server.lock"), join(sources.libraryDirectory, "database.lock"));
    }
    const held: HeldLock[] = [];
    let retained = false;
    await using release = { async [Symbol.asyncDispose]() { if (!retained) for (const lock of held.toReversed()) await lock.file.close(); } };
    for (const path of paths) {
      await verifyLegacyPrivateFile(path);
      const before = await lstat(path, { bigint: true });
      const file = await open(path, "r+");
      let keep = false;
      await using closeFile = { async [Symbol.asyncDispose]() { if (!keep) await file.close(); } };
      const opened = await file.stat({ bigint: true });
      if (opened.dev !== before.dev || opened.ino !== before.ino || !opened.isFile() || opened.nlink !== 1n) throw new Error("migration.legacy_lock_changed");
      if (!native.tryLock(file.fd)) throw new Error("migration.legacy_writer_running");
      const after = await lstat(path, { bigint: true });
      if (after.dev !== opened.dev || after.ino !== opened.ino || after.isSymbolicLink()) throw new Error("migration.legacy_lock_changed");
      held.push({ path, file, dev: opened.dev, ino: opened.ino }); keep = true;
    }
    owner.assertOwned();
    const result = new LegacyQuiescenceLease(issuer, owner, sources, held, directory);
    result.assertQuiescent(sources); retained = true;
    return result;
  }
  assertQuiescent(sources: LegacySourcePaths): void {
    if (!issued.has(this) || this.#closed) throw new Error("migration.legacy_quiescence_unavailable");
    this.#owner.assertOwned();
    const normalized = { creator: resolve(sources.creator), vault: resolve(sources.vault), masterKey: resolve(sources.masterKey), libraryDirectory: sources.libraryDirectory === null ? null : resolve(sources.libraryDirectory) };
    if (!isDeepStrictEqual(normalized, this.#sources)) throw new Error("migration.legacy_source_owner_mismatch");
    for (const lock of this.#locks) {
      const current = lstatSync(lock.path, { bigint: true });
      if (current.dev !== lock.dev || current.ino !== lock.ino || !current.isFile() || current.isSymbolicLink() || current.nlink !== 1n) throw new Error("migration.legacy_lock_changed");
    }
  }
  get serviceDirectory(): string { this.assertQuiescent(this.#sources); return this.#serviceDirectory; }
  assertLibraryRecovery(owner: InstanceLease, directory: string): void {
    if (owner !== this.#owner || this.#sources.libraryDirectory === null || !equalPath(directory, this.#sources.libraryDirectory)) throw new Error("migration.legacy_library_directory_conflict");
    this.assertQuiescent(this.#sources);
  }
  async close(): Promise<void> { if (this.#closed) return; this.#closed = true; for (const lock of this.#locks.toReversed()) await lock.file.close(); }
  async [Symbol.asyncDispose](): Promise<void> { await this.close(); }
  toJSON(): never { throw new Error("Legacy quiescence authority must not be serialized"); }
}
