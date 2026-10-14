// Library app 0.1.3: 002 -> 003. Add an owner-published identity projection.
// Existing source rows, field roles, user cells and credentials remain untouched.
import { sql, type Migration } from 'kysely';

export const discoveryIdentityMigration: Migration = {
  async up(db) {
    await db.schema
      .createTable('onecrew_discovery_identity_snapshots')
      .addColumn('table_id', 'text', (c) =>
        c.primaryKey().references('onecrew_discovery_tables.table_id').onDelete('cascade'),
      )
      .addColumn('projection_format', 'integer', (c) => c.notNull())
      .addColumn('criteria_revision', 'integer', (c) => c.notNull())
      .addColumn('source_revision', 'bigint', (c) => c.notNull())
      .addColumn('source_hash', 'text', (c) => c.notNull())
      .addColumn('identity_revision', 'bigint', (c) => c.notNull())
      .addColumn('identity_hash', 'text', (c) => c.notNull())
      .addColumn('projection', 'jsonb', (c) => c.notNull())
      .addColumn('updated_at', 'text', (c) => c.notNull())
      .addCheckConstraint('onecrew_discovery_identity_format_check', sql`projection_format = 1`)
      .addCheckConstraint(
        'onecrew_discovery_identity_revision_check',
        sql`criteria_revision > 0
          AND source_revision BETWEEN 1 AND 9007199254740991
          AND identity_revision BETWEEN 1 AND 9007199254740991`,
      )
      .addCheckConstraint(
        'onecrew_discovery_identity_hash_check',
        sql`source_hash ~ '^[0-9a-f]{64}$' AND identity_hash ~ '^[0-9a-f]{64}$'`,
      )
      .addCheckConstraint(
        'onecrew_discovery_identity_projection_check',
        sql`jsonb_typeof(projection) = 'object'`,
      )
      .execute();
  },
};
