import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { loadDiscoveryLibraryBasis } from "./library-basis.ts";
import { buildDiscoveryLibrarySnapshot, discoveryLibraryBasis, type DiscoveryLibraryBasis } from "./library-material.ts";

export function reserveDiscoverySourceVersionInTransaction(database: DatabaseSync, basis: DiscoveryLibraryBasis, hash: string): number {
  if (!database.isTransaction) throw new Error("library_version_requires_transaction");
  if (!/^[a-f0-9]{64}$/u.test(hash)) throw new Error("library_source_hash_invalid");
  const raw = database.prepare("SELECT source_revision,snapshot_hash,criteria_revision FROM discovery_library_source_versions WHERE discovery_campaign_id=?1").get(basis.campaign_id);
  const prior = raw ? z.object({ source_revision: z.number().int().positive().safe(), snapshot_hash: z.string(), criteria_revision: z.number().int().positive() }).parse(raw) : null;
  if (prior && prior.snapshot_hash === hash && prior.criteria_revision === basis.revision) return prior.source_revision;
  const version = prior === null ? 1 : prior.source_revision + 1;
  if (!Number.isSafeInteger(version)) throw new Error("library_source_version_exhausted");
  const changed = database.prepare(`INSERT INTO discovery_library_source_versions VALUES(?1,?2,?3,?4,?5)
    ON CONFLICT(discovery_campaign_id) DO UPDATE SET criteria_revision=excluded.criteria_revision,
    source_revision=excluded.source_revision,snapshot_hash=excluded.snapshot_hash,updated_at=excluded.updated_at`).run(basis.campaign_id, basis.revision, version, hash, Math.floor(Date.now() / 1000));
  if (changed.changes !== 1) throw new Error("library_source_version_write_failed"); return version;
}

export class DiscoveryLibraryRepository {
  readonly #database: DatabaseSync;
  constructor(database: DatabaseSync) { this.#database = database; }
  #transaction<T>(mode: "DEFERRED" | "IMMEDIATE", body: () => T): T {
    this.#database.exec(`BEGIN ${mode}`); let committed = false;
    try { const value = body(); this.#database.exec("COMMIT"); committed = true; return value; }
    finally { if (!committed) this.#database.exec("ROLLBACK"); }
  }
  readBasis(campaign: string, revision: number): DiscoveryLibraryBasis { return this.#transaction("DEFERRED", () => loadDiscoveryLibraryBasis(this.#database, campaign, revision)); }
  reserveVersion(rawBasis: unknown, hash: string): number {
    const basis = discoveryLibraryBasis.parse(rawBasis);
    if (!/^[a-f0-9]{64}$/u.test(hash)) throw new Error("library_source_hash_invalid");
    return this.#transaction("IMMEDIATE", () => {
      if (!isDeepStrictEqual(loadDiscoveryLibraryBasis(this.#database, basis.campaign_id, basis.revision), basis)) throw new Error("library_source_changed");
      return reserveDiscoverySourceVersionInTransaction(this.#database, basis, hash);
    });
  }
  prepareSnapshot(campaign: string, revision: number) {
    const basis = this.readBasis(campaign, revision), snapshot = buildDiscoveryLibrarySnapshot(basis);
    return { ...snapshot, source_revision: this.reserveVersion(basis, snapshot.snapshot_hash) };
  }
}
