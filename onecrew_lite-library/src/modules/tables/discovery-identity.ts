import { z } from 'zod';
import type { ProjectionRow } from './discovery-projection.js';
import type { ColumnType } from 'kysely';

const id = z
  .string()
  .min(1)
  .max(512)
  .refine((value) => value === value.trim());
const hash = z.string().regex(/^[0-9a-f]{64}$/);
const revision = z.number().int().positive().max(Number.MAX_SAFE_INTEGER);
const member = z.strictObject({
  candidate_id: id,
  platform: id,
  native_author_id: id,
  display_name: z.string().max(2000),
  profile_url: z.string().max(8192),
  source_hit_ids: z.array(id).min(1).max(10000),
});

// Materialized owner decisions, not a second identity policy. No ordinary Web
// editing route may accept this contract. Eligibility remains on source rows.
export const discoveryIdentitySchema = z
  .strictObject({
    table_id: id,
    campaign_id: id,
    projection_format: z.literal(1),
    criteria_revision: revision.max(2147483647),
    source_revision: revision,
    source_hash: hash,
    identity_revision: revision,
    identity_hash: hash,
    source_rows: z
      .array(
        z.strictObject({
          search_hit_id: id,
          candidate_id: id.nullable(),
          native_author_id: id.nullable(),
          evidence_version: hash,
        }),
      )
      .max(10000),
    groups: z
      .array(
        z.strictObject({
          group_id: hash,
          has_same_author_review: z.boolean(),
          decision_ids: z.array(id).max(10000),
          members: z.array(member).min(1).max(10000),
        }),
      )
      .max(10000),
  })
  .superRefine((input, ctx) => {
    const reject = () =>
      ctx.addIssue({ code: 'custom', message: 'Invalid identity projection binding' });
    const sources = new Map(input.source_rows.map((row) => [row.search_hit_id, row]));
    if (sources.size !== input.source_rows.length) reject();
    const candidates = new Set<string>();
    const accounts = new Set<string>();
    const seenSources = new Set<string>();
    const seenGroups = new Set<string>();
    let memberships = 0;
    for (const group of input.groups) {
      if (seenGroups.has(group.group_id)) reject();
      seenGroups.add(group.group_id);
      if (
        new Set(group.decision_ids).size !== group.decision_ids.length ||
        group.has_same_author_review !== group.decision_ids.length > 0 ||
        (group.members.length > 1 && !group.has_same_author_review)
      )
        reject();
      for (const account of group.members) {
        const native = JSON.stringify([account.platform, account.native_author_id]);
        if (candidates.has(account.candidate_id) || accounts.has(native)) reject();
        candidates.add(account.candidate_id);
        accounts.add(native);
        for (const hit of account.source_hit_ids) {
          memberships++;
          if (memberships > 10000) {
            reject();
            return;
          }
          const source = sources.get(hit);
          if (
            seenSources.has(hit) ||
            !source ||
            source.candidate_id !== account.candidate_id ||
            source.native_author_id !== account.native_author_id
          )
            reject();
          seenSources.add(hit);
        }
      }
    }
    for (const source of input.source_rows) {
      if (
        (source.candidate_id === null) !== (source.native_author_id === null) ||
        (source.candidate_id !== null && !seenSources.has(source.search_hit_id)) ||
        (source.candidate_id === null && seenSources.has(source.search_hit_id))
      )
        reject();
    }
  });
export type DiscoveryIdentityProjection = z.infer<typeof discoveryIdentitySchema>;
export const discoveryIdentityReadSchema = z.strictObject({ table_id: id, campaign_id: id });
export type DiscoveryIdentityRead = z.infer<typeof discoveryIdentityReadSchema>;
export type DiscoveryIdentityDatabase = {
  onecrew_discovery_identity_snapshots: {
    table_id: string;
    projection_format: number;
    criteria_revision: number;
    source_revision: ColumnType<string | number, number, number>;
    source_hash: string;
    identity_revision: ColumnType<string | number, number, number>;
    identity_hash: string;
    projection: ColumnType<DiscoveryIdentityProjection, string, string>;
    updated_at: string;
  };
};
export type DiscoveryIdentityState = {
  state: 'missing' | 'current' | 'sources_changed';
  projection: DiscoveryIdentityProjection | null;
};

/** Must run against all saved rows in the same transaction as publication/read.
 * Partial source ingestion invalidates the previous identity projection even
 * when the changed source is not a member of any author group.
 */
export function identitySourceConflict(
  input: DiscoveryIdentityProjection,
  rows: ProjectionRow[],
): string | null {
  if (rows.length !== input.source_rows.length) return 'data.identity_sources_incomplete';
  const stored = new Map(rows.map((row) => [row.search_hit_id, row]));
  if (stored.size !== rows.length) return 'data.identity_sources_incomplete';
  for (const source of input.source_rows) {
    const row = stored.get(source.search_hit_id);
    if (
      !row ||
      row.criteria_revision !== input.criteria_revision ||
      row.source_revision !== input.source_revision ||
      row.candidate_id !== source.candidate_id ||
      row.native_author_id !== source.native_author_id ||
      row.evidence_version !== source.evidence_version
    )
      return 'data.identity_sources_changed';
  }
  return null;
}

export function identityVersionConflict(
  previous: DiscoveryIdentityProjection,
  next: DiscoveryIdentityProjection,
): string | null {
  if (previous.table_id !== next.table_id || previous.campaign_id !== next.campaign_id)
    return 'data.identity_owner_conflict';
  if (
    next.criteria_revision < previous.criteria_revision ||
    next.source_revision < previous.source_revision ||
    next.identity_revision < previous.identity_revision
  )
    return 'data.identity_stale';
  // Identity review may advance independently, but cannot redefine a source
  // version. The producer must first publish a later source snapshot for that.
  if (next.source_revision === previous.source_revision) {
    const sources = new Map(previous.source_rows.map((row) => [row.search_hit_id, row]));
    if (
      next.criteria_revision !== previous.criteria_revision ||
      next.source_hash !== previous.source_hash ||
      next.source_rows.length !== previous.source_rows.length ||
      next.source_rows.some((row) => {
        const old = sources.get(row.search_hit_id);
        return (
          !old ||
          old.candidate_id !== row.candidate_id ||
          old.native_author_id !== row.native_author_id ||
          old.evidence_version !== row.evidence_version
        );
      })
    )
      return 'data.identity_source_version_conflict';
  }
  if (
    next.identity_revision === previous.identity_revision &&
    JSON.stringify(next) !== JSON.stringify(previous)
  )
    return 'data.identity_version_conflict';
  return null;
}
