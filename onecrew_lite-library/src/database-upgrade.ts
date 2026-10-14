import type { PGlite } from '@electric-sql/pglite';
import type { MigrationProvider } from 'kysely';

export type LibraryMigrationInspection = {
  state: 'fresh' | 'current' | 'upgrade_required';
  applied: string[];
  pending: string[];
};

/** Read-only startup preflight. Never initialize a ledger, repair it, or migrate. */
export async function inspectLibraryMigrations(
  client: PGlite,
  provider: MigrationProvider,
): Promise<LibraryMigrationInspection> {
  const available = Object.keys(await provider.getMigrations()).sort();
  if (available.length === 0) throw new Error('library_migration_registry_empty');
  const ledger = await client.query<{ exists: boolean }>(
    "SELECT to_regclass('public.kysely_migration') IS NOT NULL AS exists",
  );
  if (!ledger.rows[0]?.exists) {
    const tables = await client.query<{ count: number }>(
      "SELECT count(*)::integer AS count FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema')",
    );
    if (tables.rows[0]?.count !== 0) throw new Error('library_migration_history_missing');
    return { state: 'fresh', applied: [], pending: available };
  }
  const rows = await client.query<{ name: string }>(
    'SELECT name FROM public.kysely_migration ORDER BY name',
  );
  const applied = rows.rows.map((row) => row.name);
  // An existing empty ledger is not proof that arbitrary existing tables are fresh.
  if (applied.length === 0) throw new Error('library_migration_history_empty');
  if (applied.some((name, index) => name !== available[index])) {
    throw new Error('library_migration_history_unsupported');
  }
  const pending = available.slice(applied.length);
  return { state: pending.length ? 'upgrade_required' : 'current', applied, pending };
}
