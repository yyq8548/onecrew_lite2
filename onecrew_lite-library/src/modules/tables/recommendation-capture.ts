import { createHash } from 'node:crypto';
import { z } from 'zod';
import type { DiscoveryCreatorView, TableDto } from './data.js';
import { recommendationFields, recommendationRows } from './recommendation-result.js';

export const recommendationCaptureMetadataSchema = z.strictObject({
  version_id: z.uuid(), source_instance_id: z.uuid(), consent_id: z.uuid(),
  capture_started_at: z.iso.datetime(),
});
export type RecommendationCaptureMetadata = z.infer<typeof recommendationCaptureMetadataSchema>;

/** Complete immutable JSONL v1 bytes. Upload retries must reuse these bytes. */
export function recommendationCapture(table: TableDto, material: DiscoveryCreatorView, input: RecommendationCaptureMetadata) {
  const metadata = recommendationCaptureMetadataSchema.parse(input);
  z.string().min(1).max(128).parse(table.id);
  z.string().min(1).max(256).parse(table.name);
  const rows = recommendationRows(table, material).sort((a, b) => a.id.localeCompare(b.id));
  const schemaId = 'onecrew_lite.recommendations.v1';
  const events = [
    { kind: 'header', format: 'onecrew.dynamic_table_capture', format_version: 1,
      ...metadata, table_id: table.id, table_name: table.name, capture_kind: 'full_sample' },
    { kind: 'schema', schema_id: schemaId, observed_at: metadata.capture_started_at,
      table_name: table.name, fields: recommendationFields },
    ...rows.map((row, order) => ({ kind: 'row.upsert', row_id: row.id, schema_id: schemaId,
      observed_at: metadata.capture_started_at,
      source_row_version: createHash('sha256').update(JSON.stringify(row.values)).digest('hex'),
      order, values: row.values })),
  ];
  const bytes = Buffer.from(events.map(event => JSON.stringify(event)).join('\n') + '\n', 'utf8');
  return { bytes, record_count: rows.length, schema_count: 1 as const,
    content_hash: createHash('sha256').update(JSON.stringify({ table_name: table.name,
      fields: recommendationFields, criteria_revision: material.projection!.criteria_revision,
      rows })).digest('hex') };
}
