import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { PGlite } from '@electric-sql/pglite';
import type { MigrationProvider } from 'kysely';
import { inspectLibraryMigrations, type LibraryMigrationInspection } from './database-upgrade.js';
import { writePrivateRecoveryFile } from './recovery-file.js';

const digest = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const identifier = (value: string) => '"' + value.replaceAll('"', '""') + '"';

async function tableCounts(client: PGlite) {
  const tables = await client.query<{ schemaname: string; tablename: string }>(
    "SELECT schemaname, tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema') ORDER BY schemaname, tablename",
  );
  const counts: [string, string, string][] = [];
  for (const { schemaname, tablename } of tables.rows) {
    const result = await client.query<{ count: string }>(
      `SELECT count(*)::text AS count FROM ${identifier(schemaname)}.${identifier(tablename)}`,
    );
    counts.push([schemaname, tablename, result.rows[0].count]);
  }
  return counts;
}

/** Startup-only preparation, under the caller's existing profile/process lease.
 * The server must not admit writes between inspection and migration. This does
 * not execute migrations, restore the source or expose a public backup command.
 */
export async function createLibraryRecoveryPoint(
  client: PGlite,
  provider: MigrationProvider,
  root: string,
  expected: LibraryMigrationInspection,
): Promise<{ file: string; sha256: string; bytes: number }> {
  const current = await inspectLibraryMigrations(client, provider);
  if (
    expected.state !== 'upgrade_required' ||
    JSON.stringify(current) !== JSON.stringify(expected)
  ) {
    throw new Error('library_recovery_source_changed');
  }
  const before = await tableCounts(client);
  const archive = new Uint8Array(await (await client.dumpDataDir('gzip')).arrayBuffer());
  const sha256 = digest(archive);
  const file = await writePrivateRecoveryFile(root, archive);
  let restored: PGlite | undefined;
  try {
    const saved = await readFile(file);
    if (saved.length !== archive.length || digest(saved) !== sha256) {
      throw new Error('archive mismatch');
    }
    restored = await PGlite.create({ loadDataDir: new Blob([saved]) });
    const restoredState = await inspectLibraryMigrations(restored, provider);
    if (
      JSON.stringify(restoredState) !== JSON.stringify(expected) ||
      JSON.stringify(await tableCounts(restored)) !== JSON.stringify(before) ||
      JSON.stringify(await inspectLibraryMigrations(client, provider)) !==
        JSON.stringify(expected) ||
      JSON.stringify(await tableCounts(client)) !== JSON.stringify(before)
    ) {
      throw new Error('source or recovery state changed');
    }
    return { file, sha256, bytes: archive.length };
  } catch {
    // Retain the private artifact for diagnosis, never authorize migration.
    throw new Error('library_recovery_verification_failed');
  } finally {
    if (restored)
      await restored.close().catch(() => {
        throw new Error('library_recovery_verification_failed');
      });
  }
}
