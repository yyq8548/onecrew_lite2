import { z } from 'zod';
import type { ColumnType } from 'kysely';
import { tableCreateSchema, isSocialProfile } from '@onecrew/contracts/tables';

const id = z
  .string()
  .min(1)
  .max(512)
  .refine((value) => value === value.trim());
const revision = z.number().int().positive().max(Number.MAX_SAFE_INTEGER);
export const discoveryContactSchema = z
  .strictObject({
    kind: z.enum(['email', 'telegram', 'hidden_email', 'social_profile', 'whatsapp', 'wechat', 'wechat_official']),
    original_value: z.string().min(1).max(2048),
    normalized_value: z.string().min(1).max(2048),
    source_url: z
      .url()
      .max(2048)
      .refine((v) => v.startsWith('https://')),
    source_excerpt: z.string().min(1).max(4096),
    captured_at: revision,
    native_author_id: id.max(128),
    attribution: z.enum(['author_published', 'unconfirmed']),
  })
  .superRefine((contact, ctx) => {
    const valid =
      contact.kind === 'email'
        ? /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(contact.normalized_value)
        : contact.kind === 'telegram'
          ? /^https:\/\/t\.me\/[a-zA-Z0-9_+/-]+(?:\?[^\s]*)?$/.test(contact.normalized_value)
          : contact.kind === 'hidden_email'
            ? contact.normalized_value === contact.source_url &&
              contact.normalized_value ===
                `https://www.youtube.com/channel/${contact.native_author_id}` &&
              /^UC[A-Za-z0-9_-]{22}$/.test(contact.native_author_id)
            : contact.kind === 'whatsapp'
              ? /^https:\/\/wa\.me\/[1-9][0-9]{6,14}$/.test(contact.normalized_value)
              : contact.kind === 'wechat'
                ? /^[a-zA-Z][a-zA-Z0-9_-]{5,19}$/.test(contact.normalized_value)
                : contact.kind === 'wechat_official'
                  ? contact.normalized_value.trim().length > 0 && contact.normalized_value.length <= 80
                  : isSocialProfile(contact.normalized_value);
    if (!valid) ctx.addIssue({ code: 'custom', message: 'Invalid contact locator' });
    if (['hidden_email', 'social_profile', 'whatsapp', 'wechat', 'wechat_official'].includes(contact.kind)
      && contact.attribution !== 'author_published')
      ctx.addIssue({ code: 'custom', message: 'Contact route requires author-published evidence' });
  });
export type DiscoveryContact = z.infer<typeof discoveryContactSchema>;
const rowSchema = z
  .strictObject({
    row_id: id,
    search_hit_id: id,
    candidate_id: id.nullable(),
    native_author_id: id.nullable(),
    verdict: z.enum(['unverified', 'pending', 'qualified', 'rejected']),
    reasons: z.array(z.string().min(1).max(2000)).max(54),
    evidence_version: z.string().regex(/^[0-9a-f]{64}$/),
    // Older producers have no contacts. Default only this additive wire field.
    contacts: z.array(discoveryContactSchema).max(100).default([]),
  })
  .superRefine((row, ctx) => {
    const valid =
      row.verdict === 'unverified'
        ? row.candidate_id === null && row.native_author_id === null
        : row.candidate_id !== null && row.native_author_id !== null;
    if (!valid) ctx.addIssue({ code: 'custom', message: 'Invalid source identity binding' });
    if (row.contacts.some((contact) => contact.native_author_id !== row.native_author_id))
      ctx.addIssue({ code: 'custom', message: 'Contact author binding differs from source' });
  });

// Internal producer contract only. Do not expose this as an ordinary table editing API.
export const discoveryProjectionSchema = z
  .strictObject({
    table_id: id,
    campaign_id: id,
    criteria_revision: revision.max(2147483647),
    source_revision: revision,
    rows: z.array(rowSchema).min(1).max(100),
  })
  .superRefine((input, ctx) => {
    for (const key of ['row_id', 'search_hit_id'] as const) {
      if (new Set(input.rows.map((row) => row[key])).size !== input.rows.length)
        ctx.addIssue({ code: 'custom', message: `Duplicate ${key}` });
    }
  });
export type DiscoveryProjection = z.infer<typeof discoveryProjectionSchema>;
// The producer supplies sources, never destination row IDs. New IDs are returned
// by the engine and bound in the same transaction that creates those records.
export const discoveryIngestSchema = z
  .strictObject({
    table_id: id,
    campaign_id: id,
    criteria_revision: revision.max(2147483647),
    source_revision: revision,
    rows: z
      .array(
        z
          .strictObject({
            search_hit_id: rowSchema.shape.search_hit_id,
            candidate_id: rowSchema.shape.candidate_id,
            native_author_id: rowSchema.shape.native_author_id,
            verdict: rowSchema.shape.verdict,
            reasons: rowSchema.shape.reasons,
            evidence_version: rowSchema.shape.evidence_version,
            contacts: rowSchema.shape.contacts,
            values: z
              .record(
                id,
                z.union([
                  z.string().max(16384),
                  z.number().finite(),
                  z.boolean(),
                  z.null(),
                  z.strictObject({
                    level: z.enum(['excellent', 'good', 'medium', 'low']),
                    reason: z.string().min(1).max(4000),
                  }),
                ]),
              )
              .refine((values) => Object.keys(values).length <= 100),
          })
          .superRefine((row, ctx) => {
            const { values: _values, ...projection } = row;
            if (!rowSchema.safeParse({ ...projection, row_id: 'new-source' }).success)
              ctx.addIssue({ code: 'custom', message: 'Invalid source identity binding' });
            if (
              Object.values(row.values).some(
                (value) =>
                  value !== null &&
                  typeof value === 'object' &&
                  row.verdict !== 'qualified' &&
                  !(row.verdict === 'rejected' && value.level === 'low'),
              )
            )
              ctx.addIssue({ code: 'custom', message: 'Score conflicts with source verdict' });
          }),
      )
      .min(1)
      .max(100),
  })
  .superRefine((input, ctx) => {
    if (new Set(input.rows.map((row) => row.search_hit_id)).size !== input.rows.length)
      ctx.addIssue({ code: 'custom', message: 'Duplicate search_hit_id' });
  });
export type DiscoveryIngest = z.infer<typeof discoveryIngestSchema>;
export const discoveryStateSchema = z.strictObject({
  table_id: id,
  campaign_id: id,
  after_hit_id: id.optional(),
  limit: z.coerce.number().int().min(1).max(100).default(100),
});
export type DiscoveryStateInput = z.infer<typeof discoveryStateSchema>;
export const discoveryCampaignSchema = z.strictObject({ campaign_id: id });
export const discoveryTableSchema = z.strictObject({
  ...tableCreateSchema.shape,
  campaign_id: id,
  previous_auto_name: tableCreateSchema.shape.name.optional(),
});
export type DiscoveryTableInput = z.infer<typeof discoveryTableSchema>;
export type ProjectionRow = DiscoveryProjection['rows'][number] & {
  criteria_revision: number;
  source_revision: number;
};
export type DiscoveryProjectionDatabase = {
  onecrew_discovery_tables: {
    table_id: string;
    tenant_id: string;
    campaign_id: string;
    projection_format: number;
    created_at: string;
  };
  onecrew_discovery_fields: { table_id: string; field_id: string; role: string };
  onecrew_discovery_rows: Omit<ProjectionRow, 'reasons' | 'contacts' | 'source_revision'> & {
    table_id: string;
    source_revision: ColumnType<string | number, number, number>;
    reasons: ColumnType<string[], string, string>;
    contacts: ColumnType<DiscoveryContact[], string, string>;
    updated_at: string;
  };
};
export const projectionFields = [
  { role: 'qualification', name: 'OneCrew qualification', data_type: 'string' },
  { role: 'reasons', name: 'OneCrew qualification reasons', data_type: 'string' },
  { role: 'criteria_revision', name: 'OneCrew criteria revision', data_type: 'int' },
  { role: 'candidate_id', name: 'OneCrew candidate ID', data_type: 'string' },
  { role: 'native_author_id', name: 'OneCrew native author ID', data_type: 'string' },
  { role: 'evidence_version', name: 'OneCrew evidence version', data_type: 'string' },
] as const;

export function projectionConflict(previous: ProjectionRow, next: ProjectionRow): string | null {
  if (previous.row_id !== next.row_id || previous.search_hit_id !== next.search_hit_id)
    return 'data.projection_row_conflict';
  if (
    next.source_revision < previous.source_revision ||
    next.criteria_revision < previous.criteria_revision
  )
    return 'data.projection_stale';
  if (
    previous.native_author_id !== null &&
    (previous.native_author_id !== next.native_author_id ||
      (previous.criteria_revision === next.criteria_revision &&
        previous.candidate_id !== next.candidate_id))
  )
    return 'data.projection_identity_conflict';
  if (
    next.source_revision === previous.source_revision &&
    (next.criteria_revision !== previous.criteria_revision ||
      next.candidate_id !== previous.candidate_id ||
      next.native_author_id !== previous.native_author_id ||
      next.verdict !== previous.verdict ||
      next.evidence_version !== previous.evidence_version ||
      JSON.stringify(next.reasons) !== JSON.stringify(previous.reasons) ||
      contactVersion(next.contacts) !== contactVersion(previous.contacts))
  )
    return 'data.projection_version_conflict';
  return null;
}

// JSONB may reorder object keys. Compare typed facts, not serialized key order.
function contactVersion(contacts: DiscoveryContact[]) {
  return JSON.stringify(
    contacts.map((c) => [
      c.kind,
      c.original_value,
      c.normalized_value,
      c.source_url,
      c.source_excerpt,
      c.captured_at,
      c.native_author_id,
      c.attribution,
    ]),
  );
}

export function projectionValues(row: ProjectionRow): Record<string, unknown> {
  return {
    qualification: row.verdict,
    reasons: JSON.stringify(row.reasons),
    criteria_revision: row.criteria_revision,
    candidate_id: row.candidate_id,
    native_author_id: row.native_author_id,
    evidence_version: row.evidence_version,
  };
}
