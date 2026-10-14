import { statfs } from 'node:fs/promises';
import type { PGlite } from '@electric-sql/pglite';
import { Migrator, type Kysely, type MigrationProvider } from 'kysely';
import { inspectLibraryMigrations } from './database-upgrade.js';
import { createLibraryRecoveryPoint } from './database-recovery.js';

export function requireRecoveryCapacity(databaseBytes: bigint, availableBytes: bigint): void {
  // Bound simultaneous source + independent WASM restore memory consumption.
  if (databaseBytes <= 0n || databaseBytes > 512n * 1024n * 1024n) {
    throw new Error('library_recovery_size_unsupported');
  }
  if (availableBytes < databaseBytes * 3n + 64n * 1024n * 1024n) {
    throw new Error('library_recovery_space_insufficient');
  }
}

/** Caller holds the database lease and has not admitted HTTP/MCP operations. */
export async function migrateLibraryDatabase<T>(
  client: PGlite,
  db: Kysely<T>,
  provider: MigrationProvider,
  root: string,
): Promise<'fresh' | 'current' | 'upgraded'> {
  const before = await inspectLibraryMigrations(client, provider);
  if (before.state === 'current') return 'current';
  if (before.state === 'upgrade_required') {
    const size = await client.query<{ bytes: string }>(
      'SELECT pg_database_size(current_database())::text AS bytes',
    );
    const disk = await statfs(root, { bigint: true });
    requireRecoveryCapacity(BigInt(size.rows[0].bytes), disk.bavail * disk.bsize);
    await createLibraryRecoveryPoint(client, provider, root, before);
    if (
      JSON.stringify(await inspectLibraryMigrations(client, provider)) !== JSON.stringify(before)
    ) {
      throw new Error('library_recovery_source_changed');
    }
  }
  // Kysely's PostgreSQL adapter executes all pending edges and its ledger in one
  // transaction; do not use disableTransactions or have an edge commit itself.
  const result = await new Migrator({ db, provider }).migrateToLatest();
  const after = await inspectLibraryMigrations(client, provider).catch(() => undefined);
  if (result.error) {
    if (after?.state === 'current') throw new Error('library_upgrade_failed_after_commit');
    if (JSON.stringify(after) === JSON.stringify(before)) {
      throw new Error('library_upgrade_failed_before_commit');
    }
    throw new Error('library_upgrade_state_unknown');
  }
  if (after?.state !== 'current') throw new Error('library_upgrade_postflight_failed');
  return before.state === 'fresh' ? 'fresh' : 'upgraded';
}
