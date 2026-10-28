import { randomUUID } from "node:crypto";
import { lstatSync, type BigIntStats } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { InstanceLease } from "../service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../service/profile.ts";
import { createPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { writeMigrationJournal } from "./journal.ts";
import { LegacyQuiescenceLease, type LegacySourcePaths } from "./legacy-quiescence.ts";

const issuer = Symbol("macos-installer-cutover");

async function verifyLegacyProfile(lease: InstanceLease, legacyPort: number): Promise<void> {
  await verifyPrivateFile(join(lease.directory, "profile.json"));
  const profile = await loadOrCreateServiceProfile(lease);
  if (profile.originSource !== "legacy-upgrade" || profile.port !== legacyPort) throw new Error("migration.old_origin_unverified");
}

export class PreparedMacCutover {
  readonly quiescence: LegacyQuiescenceLease;
  readonly directory: string;
  readonly #lease: InstanceLease;
  readonly #sources: LegacySourcePaths;
  readonly #journal: BigIntStats;
  readonly #legacyPort: number;
  constructor(token: symbol, lease: InstanceLease, quiescence: LegacyQuiescenceLease, directory: string,
    sources: LegacySourcePaths, legacyPort: number) {
    if (token !== issuer) throw new Error("migration.cutover_authority_unissued");
    this.#lease = lease; this.quiescence = quiescence; this.directory = directory;
    this.#sources = Object.freeze({ ...sources }); this.#legacyPort = legacyPort;
    this.#journal = lstatSync(join(directory, "journal.json"), { bigint: true });
    if (!this.#journal.isFile() || this.#journal.isSymbolicLink() || this.#journal.nlink !== 1n) throw new Error("migration.cutover_journal_changed");
    Object.freeze(this);
  }
  async verifyForCommit(lease: InstanceLease, quiescence: LegacyQuiescenceLease, sources: LegacySourcePaths): Promise<void> {
    this.#lease.assertOwned();
    if (lease !== this.#lease || quiescence !== this.quiescence || !isDeepStrictEqual(sources, this.#sources)) throw new Error("migration.cutover_owner_changed");
    this.quiescence.assertQuiescent(sources);
    const journal = lstatSync(join(this.directory, "journal.json"), { bigint: true }), expected = this.#journal;
    if (!journal.isFile() || journal.isSymbolicLink() || journal.nlink !== 1n || journal.dev !== expected.dev || journal.ino !== expected.ino || journal.size !== expected.size || journal.mtimeNs !== expected.mtimeNs || journal.ctimeNs !== expected.ctimeNs) throw new Error("migration.cutover_journal_changed");
    await verifyLegacyProfile(lease, this.#legacyPort);
    this.quiescence.assertQuiescent(sources); lease.assertOwned();
  }
  toJSON(): never { throw new Error("Installer cutover authority cannot be serialized"); }
  async close(): Promise<void> { await this.quiescence.close(); }
  async [Symbol.asyncDispose](): Promise<void> { await this.close(); }
}

// 持有旧服务和 Library 的真实锁，直到激活结束或准备失败。
export async function prepareMacCutover(lease: InstanceLease, input: { legacyServiceDirectory: string; sources: LegacySourcePaths; legacyPort: number }): Promise<PreparedMacCutover> {
  if (process.platform !== "darwin") throw new Error("migration.macos_cutover_requires_darwin");
  lease.assertOwned();
  const { legacyServiceDirectory, legacyPort } = input, sources = Object.freeze({ ...input.sources });
  await verifyLegacyProfile(lease, legacyPort);
  const directory = await createPrivateDirectory(join(lease.directory, `cutover-${randomUUID()}`));
  let phase = "acquire_original_locks", completed = false, quiescence: LegacyQuiescenceLease | undefined;
  await writeMigrationJournal(directory, { state: "preparing", phase, activated: false });
  await using failure = { async [Symbol.asyncDispose]() {
    if (!completed) { await quiescence?.close(); await writeMigrationJournal(directory, { state: "failed", phase, activated: false }); }
  } };
  quiescence = await LegacyQuiescenceLease.acquire(lease, { serviceDirectory: legacyServiceDirectory, sources });
  phase = "verify_stopped_installation";
  await verifyLegacyProfile(lease, legacyPort);
  quiescence.assertQuiescent(sources); lease.assertOwned();
  await writeMigrationJournal(directory, { state: "macos_cutover_ready", activated: false,
    originalDataDirectory: quiescence.serviceDirectory, originalWebPort: legacyPort, sources });
  const result = new PreparedMacCutover(issuer, lease, quiescence, directory, sources, legacyPort);
  completed = true; return result;
}
