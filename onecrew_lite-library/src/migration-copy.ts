import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { sql } from 'kysely';
import { openDatabase } from './database.js';

export interface LegacyLibraryAccountRow {
  access_user_id: string;
  credential: unknown | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}
export interface LegacyLibraryTenantRow {
  id: string;
  owner_user_id: string;
  space_id: string;
  base_id: string;
}

/** 仅供主迁移器检查已经隔离的副本；账号存在不代表获得远端授权。 */
export async function inspectLibraryMigrationCopy<T>(
  rootDir: string,
  inspect: (snapshot: {
    accounts: readonly LegacyLibraryAccountRow[];
    tenants: readonly LegacyLibraryTenantRow[];
  }) => T | Promise<T>,
): Promise<T> {
  const versionPath = join(rootDir, 'pglite', 'PG_VERSION');
  const info = await lstat(versionPath);
  if (!info.isFile() || info.isSymbolicLink()) throw new Error('library_migration_copy_missing');
  const version = await readFile(versionPath, 'utf8');
  if (!/^\d+\s*$/.test(version)) throw new Error('library_migration_copy_invalid');
  const database = await openDatabase(rootDir);
  await using release = { [Symbol.asyncDispose]: () => database.close() };
  return await database.db.transaction().execute(async (transaction) => {
    await sql`SET TRANSACTION READ ONLY`.execute(transaction);
    const accounts = await sql<LegacyLibraryAccountRow>`
      SELECT access_user_id, credential, is_active, created_at, updated_at
      FROM onecrew_access_accounts ORDER BY access_user_id
    `.execute(transaction);
    const tenants = await sql<LegacyLibraryTenantRow>`
      SELECT id, owner_user_id, space_id, base_id FROM onecrew_tenants ORDER BY id
    `.execute(transaction);
    if (accounts.rows.filter((account) => account.is_active).length > 1)
      throw new Error('library_migration_multiple_active_accounts');
    return inspect({ accounts: accounts.rows, tenants: tenants.rows });
  });
}
