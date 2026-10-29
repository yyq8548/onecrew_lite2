import { lstat, open, readFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { OwnedDiscoveryQueueDirectory } from "./saved-scores.ts";
import type { HostTask, HostWorkState } from "./host-work.ts";

export class HostWorkStore {
  readonly #owned: OwnedDiscoveryQueueDirectory;
  readonly #signal: AbortSignal;
  readonly #directory: string;
  readonly #path: string;
  readonly #decode: (value: unknown) => HostTask;
  readonly #decodeLegacy: (value: unknown) => HostWorkState;
  #opening: Promise<DatabaseSync> | undefined;
  #identity: { dev: number; ino: number } | undefined;
  constructor(owned: OwnedDiscoveryQueueDirectory, signal: AbortSignal, decode: (value: unknown) => HostTask, decodeLegacy: (value: unknown) => HostWorkState) {
    this.#owned = owned; this.#signal = signal; this.#decode = decode; this.#decodeLegacy = decodeLegacy;
    this.#directory = join(owned.directory, "discovery-host-work"); this.#path = join(this.#directory, "tasks.sqlite");
  }
  #owner(): string { return stringifyApplicationJson([this.#owned.owner.accountId, this.#owned.owner.tenantId, this.#owned.owner.baseId]); }
  async #open(): Promise<DatabaseSync> {
    this.#signal.throwIfAborted(); this.#owned.assertOwned(); await verifyPrivateDirectory(this.#owned.directory);
    const directory = await lstat(this.#directory).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (directory === null) await createPrivateDirectory(this.#directory); else await verifyPrivateDirectory(this.#directory);
    const existing = await lstat(this.#path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (existing === null) {
      const file = await open(this.#path, "wx", 0o600); await file.close();
    }
    await verifyPrivateFile(this.#path);
    const checked = await lstat(this.#path);
    if (checked.nlink !== 1) throw new Error("host_work_file_invalid");
    const db = new DatabaseSync(this.#path);
    try {
      db.exec("PRAGMA busy_timeout=5000; PRAGMA synchronous=FULL; BEGIN IMMEDIATE");
      const version = db.prepare("PRAGMA user_version").get()?.user_version;
      if (version === 0) {
        if (db.prepare("SELECT name FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' LIMIT 1").get()) throw new Error("host_work_storage_version_invalid");
        let legacy: HostWorkState | undefined;
        const oldPath = join(this.#directory, "tasks.json");
        const old = await lstat(oldPath).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
        if (old !== null) {
          await verifyPrivateFile(oldPath);
          if (!old.isFile() || old.isSymbolicLink() || old.nlink !== 1) throw new Error("host_work_file_invalid");
          legacy = this.#decodeLegacy(parseApplicationJson(await readFile(oldPath)));
          const owner = stringifyApplicationJson([legacy.owner.accountId, legacy.owner.tenantId, legacy.owner.baseId]);
          if (owner !== this.#owner() || new Set(legacy.tasks.map(row => row.id)).size !== legacy.tasks.length) throw new Error("host_work_owner_mismatch");
        }
        db.exec("CREATE TABLE host_work_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT; CREATE TABLE host_work_tasks (id TEXT PRIMARY KEY, body TEXT NOT NULL) STRICT; PRAGMA user_version=1");
        db.prepare("INSERT INTO host_work_metadata VALUES ('owner', ?)").run(this.#owner());
        const insert = db.prepare("INSERT INTO host_work_tasks(id,body) VALUES (?,?)");
        for (const row of legacy?.tasks ?? []) insert.run(row.id, stringifyApplicationJson(row));
      } else if (version !== 1) throw new Error("host_work_storage_version_invalid");
      if (db.prepare("SELECT value FROM host_work_metadata WHERE key='owner'").get()?.value !== this.#owner()) throw new Error("host_work_owner_mismatch");
      this.#signal.throwIfAborted(); this.#owned.assertOwned(); db.exec("COMMIT");
      this.#identity = { dev: checked.dev, ino: checked.ino };
      this.#signal.addEventListener("abort", () => { if (db.isOpen) db.close(); }, { once: true });
      return db;
    } catch (error) {
      if (db.isOpen) db.close();
      throw error;
    }
  }
  async #database(): Promise<DatabaseSync> {
    this.#signal.throwIfAborted(); this.#owned.assertOwned();
    if (!this.#opening) {
      const opening = this.#open(); this.#opening = opening;
      void opening.then(() => undefined, () => { if (this.#opening === opening) this.#opening = undefined; });
    }
    const db = await this.#opening;
    await verifyPrivateFile(this.#path);
    const info = await lstat(this.#path);
    if (info.nlink !== 1 || info.dev !== this.#identity?.dev || info.ino !== this.#identity?.ino) throw new Error("host_work_file_invalid");
    this.#signal.throwIfAborted(); this.#owned.assertOwned();
    if (db.prepare("SELECT value FROM host_work_metadata WHERE key='owner'").get()?.value !== this.#owner()) throw new Error("host_work_owner_mismatch");
    return db;
  }
  async get(id: string): Promise<HostTask | null> { return (await this.getMany([id]))[0] ?? null; }
  async getMany(ids: readonly string[]): Promise<HostTask[]> {
    const db = await this.#database(), statement = db.prepare("SELECT body FROM host_work_tasks WHERE id=?"), rows: HostTask[] = [];
    for (const id of ids) {
      const value = statement.get(id); if (!value) continue;
      if (typeof value.body !== "string") throw new Error("host_work_file_invalid");
      const row = this.#decode(parseApplicationJson(Buffer.from(value.body)));
      if (row.id !== id) throw new Error("host_work_file_invalid");
      rows.push(row);
    }
    return rows;
  }
  async put(row: HostTask): Promise<void> { await this.putMany([row]); }
  async putMany(rows: readonly HostTask[]): Promise<void> {
    if (!rows.length) return;
    const db = await this.#database();
    db.exec("BEGIN IMMEDIATE");
    try {
      const put = db.prepare("INSERT INTO host_work_tasks(id,body) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body");
      for (const row of rows) put.run(row.id, stringifyApplicationJson(row));
      this.#signal.throwIfAborted(); this.#owned.assertOwned(); db.exec("COMMIT");
    } catch (error) { db.exec("ROLLBACK"); throw error; }
  }
}
