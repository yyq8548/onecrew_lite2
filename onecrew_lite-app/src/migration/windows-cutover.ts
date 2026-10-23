import { randomUUID } from "node:crypto";
import { lstatSync, type BigIntStats } from "node:fs";
import { join } from "node:path";
import { setTimeout } from "node:timers/promises";
import { isDeepStrictEqual } from "node:util";
import type { InstanceLease } from "../service/instance-lock.ts";
import { createPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import { loadOrCreateServiceProfile } from "../service/profile.ts";
import { writeMigrationJournal } from "./journal.ts";
import { LegacyQuiescenceLease, type LegacySourcePaths } from "./legacy-quiescence.ts";
import { deriveLegacyWindowsWebPort, stopLegacyLibrary, stopLegacyWindowsService } from "./legacy-control.ts";
import { disableLegacyWindowsRegistration, type DisabledWindowsRegistration } from "./windows-registration.ts";
import { VerifiedLegacyWindowsWorkspace } from "./windows-workspace.ts";

const issuer = Symbol("windows-installer-cutover");

export class PreparedWindowsCutover {
  readonly quiescence: LegacyQuiescenceLease;
  readonly directory: string;
  readonly #lease: InstanceLease;
  readonly #workspace: VerifiedLegacyWindowsWorkspace;
  readonly #registration: DisabledWindowsRegistration;
  readonly #sources: LegacySourcePaths;
  readonly #journal: BigIntStats;
  readonly #webPort: number;
  constructor(token: symbol, lease: InstanceLease, workspace: VerifiedLegacyWindowsWorkspace, registration: DisabledWindowsRegistration,
    quiescence: LegacyQuiescenceLease, directory: string, sources: LegacySourcePaths, webPort: number) {
    if (token !== issuer) throw new Error("migration.cutover_authority_unissued");
    this.#lease = lease; this.#workspace = workspace; this.#registration = registration; this.quiescence = quiescence;
    this.directory = directory; this.#sources = Object.freeze({ ...sources }); this.#journal = lstatSync(join(directory, "journal.json"), { bigint: true }); Object.freeze(this);
    this.#webPort = webPort;
  }
  async verifyForCommit(lease: InstanceLease, quiescence: LegacyQuiescenceLease, sources: LegacySourcePaths): Promise<void> {
    this.#lease.assertOwned();
    if (lease !== this.#lease || quiescence !== this.quiescence || !isDeepStrictEqual(sources, this.#sources)) throw new Error("migration.cutover_owner_changed");
    this.#workspace.assertUnchanged(); this.quiescence.assertQuiescent(sources);
    const journal = lstatSync(join(this.directory, "journal.json"), { bigint: true }), expected = this.#journal;
    if (journal.isSymbolicLink() || journal.dev !== expected.dev || journal.ino !== expected.ino || journal.size !== expected.size || journal.mtimeNs !== expected.mtimeNs || journal.ctimeNs !== expected.ctimeNs) throw new Error("migration.cutover_journal_changed");
    await this.#registration.verify(lease);
    const profile = await loadOrCreateServiceProfile(lease);
    if (profile.originSource !== "legacy-upgrade" || profile.port !== this.#webPort) throw new Error("migration.old_origin_unverified");
    this.#workspace.assertUnchanged(); this.quiescence.assertQuiescent(sources); lease.assertOwned();
  }
  toJSON(): never { throw new Error("Installer cutover authority cannot be serialized"); }
  async close(): Promise<void> { await this.quiescence.close(); }
  async [Symbol.asyncDispose](): Promise<void> { await this.close(); }
}

// 停用已核实的原计划任务后才排空旧程序；锁必须贯穿后续校验和当前指针发布。
export async function prepareWindowsCutover(lease: InstanceLease, input: { workspace: VerifiedLegacyWindowsWorkspace; sources: LegacySourcePaths; signal: AbortSignal }): Promise<PreparedWindowsCutover> {
  lease.assertOwned(); input.signal.throwIfAborted();
  if (!(input.workspace instanceof VerifiedLegacyWindowsWorkspace)) throw new Error("migration.workspace_authority_required");
  input.workspace.assertUnchanged();
  await verifyPrivateFile(join(lease.directory, "profile.json"));
  const profile = await loadOrCreateServiceProfile(lease), webPort = await deriveLegacyWindowsWebPort(input.workspace.registration.dataDirectory);
  if (profile.originSource !== "legacy-upgrade" || profile.port !== webPort) throw new Error("migration.old_origin_unverified");
  const directory = await createPrivateDirectory(join(lease.directory, `cutover-${randomUUID()}`));
  let phase = "disable_original_task", completed = false, quiescence: LegacyQuiescenceLease | undefined;
  await writeMigrationJournal(directory, { state: "preparing", phase, activated: false });
  await using failure = { async [Symbol.asyncDispose]() {
    if (!completed) { await quiescence?.close(); await writeMigrationJournal(directory, { state: "failed", phase, activated: false }); }
  } };
  const registration = await disableLegacyWindowsRegistration(lease, input.workspace.registration, join(directory, "original-task.xml"));
  input.signal.throwIfAborted(); phase = "stop_original_service";
  const service = await stopLegacyWindowsService(input.workspace.registration, input.signal);
  input.signal.throwIfAborted(); phase = "stop_original_library";
  const library = input.sources.libraryDirectory === null ? { state: "absent" as const } : await stopLegacyLibrary(input.sources.libraryDirectory, input.signal);
  phase = "acquire_original_locks";
  const deadline = Date.now() + 60_000;
  while (!quiescence) {
    input.signal.throwIfAborted();
    quiescence = await LegacyQuiescenceLease.acquire(lease, { serviceDirectory: input.workspace.registration.dataDirectory, sources: input.sources }).then(value => value, error => {
      if (!(error instanceof Error) || error.message !== "migration.legacy_writer_running" || Date.now() >= deadline) throw error;
      return undefined;
    });
    if (!quiescence) await setTimeout(100, undefined, { signal: input.signal });
  }
  phase = "verify_stopped_installation";
  await registration.verify(lease); input.workspace.assertUnchanged(); quiescence.assertQuiescent(input.sources); input.signal.throwIfAborted(); lease.assertOwned();
  await writeMigrationJournal(directory, { state: "windows_cutover_ready", activated: false, originalManifestSha256: input.workspace.manifestSha256,
    originalExecutableSha256: input.workspace.registration.executableSha256, originalDataDirectory: input.workspace.registration.dataDirectory, originalWebPort: webPort, service, library, sources: input.sources });
  const result = new PreparedWindowsCutover(issuer, lease, input.workspace, registration, quiescence, directory, input.sources, webPort);
  completed = true; return result;
}
