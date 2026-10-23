import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { assertValidatedCreatorConnection } from "../storage/creator-schema.ts";
import { loadDiscoveryLibraryBasis } from "./library-basis.ts";
import { buildDiscoveryLibrarySnapshot } from "./library-material.ts";
import { reserveDiscoverySourceVersionInTransaction } from "./library-repository.ts";
import { identityGroupsInput } from "./identity-contracts.ts";
import { buildIdentityGroups, type IdentityGroupsBasis } from "./identity-groups.ts";
import { loadIdentityGroupsBasis } from "./identity-groups-basis.ts";
import { identityTransaction } from "./identity-repository.ts";

export function loadIdentityPublicationBasis(database: DatabaseSync, campaign: string, revision: number, includePendingDetails = true): IdentityGroupsBasis {
  identityGroupsInput.parse({ campaign_id: campaign, expected_revision: revision });
  if (!database.isTransaction) throw new Error("library_publication_requires_snapshot");
  return loadIdentityGroupsBasis(database, loadDiscoveryLibraryBasis(database, campaign, revision, includePendingDetails));
}
export function buildIdentityLibraryMaterial(basis: IdentityGroupsBasis) { return { sources: buildDiscoveryLibrarySnapshot(basis.library), identity: buildIdentityGroups(basis) }; }
export class DiscoveryIdentityPublicationRepository {
  readonly #database: DatabaseSync;
  constructor(database: DatabaseSync) { assertValidatedCreatorConnection(database); this.#database = database; }
  readBasis(campaign: string, revision: number): IdentityGroupsBasis { return identityTransaction(this.#database, false, () => loadIdentityPublicationBasis(this.#database, campaign, revision)); }
  readMaterial(campaign: string, revision: number) { return buildIdentityLibraryMaterial(this.readBasis(campaign, revision)); }
  reservePublication(basis: IdentityGroupsBasis, sourceHash: string, identityHash: string): { source_revision: number; identity_revision: number } {
    if (![sourceHash, identityHash].every(hash => /^[a-f\d]{64}$/u.test(hash))) throw new Error("library_publication_hash_invalid");
    return identityTransaction(this.#database, true, () => {
      const library = basis.library;
      if (!isDeepStrictEqual(loadIdentityPublicationBasis(this.#database, library.campaign_id, library.revision), basis)) throw new Error("library_publication_changed");
      const source_revision = reserveDiscoverySourceVersionInTransaction(this.#database, library, sourceHash);
      const row = this.#database.prepare("SELECT identity_revision,criteria_revision,source_revision,source_hash,identity_hash FROM discovery_library_identity_versions WHERE discovery_campaign_id=?").get(library.campaign_id);
      const prior = row === undefined ? null : z.object({ identity_revision: z.number().int().positive().safe(), criteria_revision: z.number().int().positive(), source_revision: z.number().int().positive().safe(), source_hash: z.string(), identity_hash: z.string() }).parse(row);
      const unchanged = prior !== null && prior.criteria_revision === library.revision && prior.source_revision === source_revision && prior.source_hash === sourceHash && prior.identity_hash === identityHash;
      const identity_revision = prior === null ? 1 : unchanged ? prior.identity_revision : prior.identity_revision + 1;
      if (!Number.isSafeInteger(identity_revision)) throw new Error("library_identity_version_exhausted");
      if (!unchanged) this.#database.prepare("INSERT INTO discovery_library_identity_versions VALUES(?,?,?,?,?,?,?) ON CONFLICT(discovery_campaign_id) DO UPDATE SET criteria_revision=excluded.criteria_revision,source_revision=excluded.source_revision,source_hash=excluded.source_hash,identity_revision=excluded.identity_revision,identity_hash=excluded.identity_hash,updated_at=excluded.updated_at").run(library.campaign_id, library.revision, source_revision, sourceHash, identity_revision, identityHash, Math.floor(Date.now() / 1000));
      return { source_revision, identity_revision };
    });
  }
  preparePublication(campaign: string, revision: number) {
    const basis = this.readBasis(campaign, revision), material = buildIdentityLibraryMaterial(basis);
    const versions = this.reservePublication(basis, material.sources.snapshot_hash, material.identity.snapshot_hash);
    return { material: { ...material, sources: { ...material.sources, source_revision: versions.source_revision } }, identity_revision: versions.identity_revision };
  }
}
