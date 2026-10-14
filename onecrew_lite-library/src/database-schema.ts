import { ensureV1MetaSchema } from '@teable/v2-adapter-repository-postgres';
import { installUndoCaptureGlobals } from '@teable/v2-adapter-table-repository-postgres';
import type { Migration, MigrationProvider } from 'kysely';
import { sql, type Kysely } from 'kysely';
import type { V1TeableDatabase } from '@teable/v2-postgres-schema';
import { discoveryProjectionMigration } from './database-migrations/002-discovery-projection.js';
import { discoveryIdentityMigration } from './database-migrations/003-discovery-identity.js';
import { discoveryContactsMigration } from './database-migrations/004-discovery-contacts.js';

const initialMigration: Migration = {
  async up(db) {
    const typedDb = db as Kysely<V1TeableDatabase>;
    await ensureV1MetaSchema(typedDb);
    await installUndoCaptureGlobals(db);

    await db.schema
      .createTable('onecrew_tenants')
      .addColumn('id', 'text', (c) => c.primaryKey())
      .addColumn('owner_user_id', 'text', (c) => c.notNull().unique())
      .addColumn('space_id', 'text', (c) => c.notNull().unique())
      .addColumn('base_id', 'text', (c) => c.notNull().unique())
      .addColumn('created_at', 'text', (c) => c.notNull())
      .addColumn('updated_at', 'text', (c) => c.notNull())
      .execute();
    await db.schema
      .createTable('onecrew_field_types')
      .addColumn('table_id', 'text', (c) => c.notNull())
      .addColumn('field_id', 'text', (c) => c.notNull())
      .addColumn('data_type', 'text', (c) => c.notNull())
      .addPrimaryKeyConstraint('onecrew_field_types_pk', ['table_id', 'field_id'])
      .execute();
    await db.schema
      .createTable('onecrew_search_jobs')
      .addColumn('id', 'text', (c) => c.primaryKey())
      .addColumn('tenant_id', 'text', (c) => c.notNull())
      .addColumn('platform', 'text', (c) => c.notNull())
      .addColumn('query', 'text', (c) => c.notNull())
      .addColumn('limit', 'integer', (c) => c.notNull())
      .addColumn('status', 'text', (c) => c.notNull())
      .addColumn('error', 'text')
      .addColumn('result_json', 'text')
      .addColumn('created_at', 'text', (c) => c.notNull())
      .addColumn('updated_at', 'text', (c) => c.notNull())
      .execute();
    await db.schema
      .createIndex('onecrew_search_jobs_tenant_created_idx')
      .on('onecrew_search_jobs')
      .columns(['tenant_id', 'created_at'])
      .execute();
    await db.schema
      .createTable('onecrew_access_accounts')
      .addColumn('access_user_id', 'text', (c) => c.primaryKey())
      .addColumn('credential', 'jsonb')
      .addColumn('is_active', 'boolean', (c) => c.notNull().defaultTo(false))
      .addColumn('created_at', 'text', (c) => c.notNull())
      .addColumn('updated_at', 'text', (c) => c.notNull())
      .addCheckConstraint(
        'onecrew_access_accounts_active_credential_check',
        sql`NOT is_active OR credential IS NOT NULL`,
      )
      .execute();
    await db.schema
      .createIndex('onecrew_access_accounts_active_idx')
      .unique()
      .on('onecrew_access_accounts')
      .column('is_active')
      .where('is_active' as never, '=', true)
      .execute();
  },
};

export const databaseMigrationProvider: MigrationProvider = {
  async getMigrations() {
    return {
      '001_tenant_initial': initialMigration,
      '002_discovery_projection': discoveryProjectionMigration,
      '003_discovery_identity': discoveryIdentityMigration,
      '004_discovery_contacts': discoveryContactsMigration,
    };
  },
};
