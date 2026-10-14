// Add owner-published contacts without changing historical source rows or cells.
import { sql, type Migration } from 'kysely';

export const discoveryContactsMigration: Migration = {
  async up(db) {
    await db.schema.alterTable('onecrew_discovery_rows')
      .addColumn('contacts', 'jsonb', (c) => c.notNull().defaultTo(sql`'[]'::jsonb`))
      .execute();
  },
};
