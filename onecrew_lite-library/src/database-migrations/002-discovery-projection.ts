// Library app 0.1.2: 001 -> 002, additive ownership only. Existing Teable tables,
// fields, cells, tenants and credentials are neither rewritten nor classified.
import { sql, type Migration } from 'kysely';

export const discoveryProjectionMigration: Migration = {
  async up(db) {
    await db.schema
      .createTable('onecrew_discovery_tables')
      .addColumn('table_id', 'text', (c) => c.primaryKey())
      .addColumn('tenant_id', 'text', (c) =>
        c.notNull().references('onecrew_tenants.id').onDelete('cascade'),
      )
      .addColumn('campaign_id', 'text', (c) => c.notNull())
      .addColumn('projection_format', 'integer', (c) => c.notNull())
      .addColumn('created_at', 'text', (c) => c.notNull())
      .addUniqueConstraint('onecrew_discovery_tables_campaign_unique', ['tenant_id', 'campaign_id'])
      .addCheckConstraint('onecrew_discovery_tables_format_check', sql`projection_format = 1`)
      .execute();
    await db.schema
      .createTable('onecrew_discovery_fields')
      .addColumn('table_id', 'text', (c) =>
        c.notNull().references('onecrew_discovery_tables.table_id').onDelete('cascade'),
      )
      .addColumn('field_id', 'text', (c) => c.notNull())
      .addColumn('role', 'text', (c) => c.notNull())
      .addPrimaryKeyConstraint('onecrew_discovery_fields_pk', ['table_id', 'role'])
      .addUniqueConstraint('onecrew_discovery_fields_id_unique', ['table_id', 'field_id'])
      .addCheckConstraint(
        'onecrew_discovery_fields_role_check',
        sql`role IN (
        'qualification', 'reasons', 'criteria_revision', 'candidate_id',
        'native_author_id', 'evidence_version')`,
      )
      .execute();
    await db.schema
      .createTable('onecrew_discovery_rows')
      .addColumn('table_id', 'text', (c) =>
        c.notNull().references('onecrew_discovery_tables.table_id').onDelete('cascade'),
      )
      .addColumn('row_id', 'text', (c) => c.notNull())
      .addColumn('search_hit_id', 'text', (c) => c.notNull())
      .addColumn('criteria_revision', 'integer', (c) => c.notNull())
      .addColumn('source_revision', 'bigint', (c) => c.notNull())
      .addColumn('candidate_id', 'text')
      .addColumn('native_author_id', 'text')
      .addColumn('verdict', 'text', (c) => c.notNull())
      .addColumn('reasons', 'jsonb', (c) => c.notNull())
      .addColumn('evidence_version', 'text', (c) => c.notNull())
      .addColumn('updated_at', 'text', (c) => c.notNull())
      .addPrimaryKeyConstraint('onecrew_discovery_rows_pk', ['table_id', 'row_id'])
      .addUniqueConstraint('onecrew_discovery_rows_hit_unique', ['table_id', 'search_hit_id'])
      .addCheckConstraint(
        'onecrew_discovery_rows_revision_check',
        sql`criteria_revision > 0 AND source_revision > 0`,
      )
      .addCheckConstraint(
        'onecrew_discovery_rows_verdict_check',
        sql`verdict IN ('unverified', 'pending', 'qualified', 'rejected')`,
      )
      .addCheckConstraint(
        'onecrew_discovery_rows_binding_check',
        sql`
        (verdict = 'unverified' AND candidate_id IS NULL AND native_author_id IS NULL)
        OR (verdict <> 'unverified' AND length(candidate_id) > 0 AND length(native_author_id) > 0
            AND candidate_id IS NOT NULL AND native_author_id IS NOT NULL)`,
      )
      .addCheckConstraint(
        'onecrew_discovery_rows_reasons_check',
        sql`jsonb_typeof(reasons) = 'array'`,
      )
      .addCheckConstraint(
        'onecrew_discovery_rows_evidence_check',
        sql`evidence_version ~ '^[0-9a-f]{64}$'`,
      )
      .execute();
  },
};
