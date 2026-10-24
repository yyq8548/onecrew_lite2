import { createHash } from 'node:crypto';
import { lstat } from 'node:fs/promises';
import { join } from 'node:path';
import { PGlite } from '@electric-sql/pglite';
import { acquireDatabaseLock } from './runtime.js';

export interface LibraryTableFingerprint {
  schema: string;
  name: string;
  columns: string[];
  rows: number;
  sha256: string;
}

const identifier = (value: string) => '"' + value.replaceAll('"', '""') + '"';

// 调用方只传入已校验的隔离副本；原库始终不由检查引擎打开。
export async function fingerprintLibraryCopy(
  directory: string,
  expected?: readonly LibraryTableFingerprint[],
): Promise<LibraryTableFingerprint[]> {
  const version = await lstat(join(directory, 'pglite', 'PG_VERSION'));
  if (!version.isFile() || version.isSymbolicLink())
    throw new Error('library_migration_copy_missing');
  const lock = await acquireDatabaseLock(directory);
  if (!lock) throw new Error('library_database_busy');
  let client: PGlite | undefined;
  await using close = {
    async [Symbol.asyncDispose]() {
      if (client) await client.close();
      if (!client || client.closed) await lock.close();
    },
  };
  client = await PGlite.create(join(directory, 'pglite'));
  await client.waitReady;
  await client.exec('BEGIN READ ONLY');
  const tables = expected ?? (await client.query<{ schema: string; name: string }>(
    `SELECT schemaname AS schema, tablename AS name FROM pg_tables
     WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
       AND tablename NOT IN ('kysely_migration', 'kysely_migration_lock')
     ORDER BY schemaname, tablename`,
  )).rows;
  const result: LibraryTableFingerprint[] = [];
  for (const table of tables) {
    const columns = 'columns' in table ? table.columns : (await client.query<{ column_name: string }>(
      `SELECT column_name FROM information_schema.columns
       WHERE table_schema=$1 AND table_name=$2 ORDER BY ordinal_position`,
      [table.schema, table.name],
    )).rows.map(row => row.column_name);
    if (!columns.length) throw new Error('library_migration_columns_missing');
    const hash = createHash('sha256').update(JSON.stringify([table.schema, table.name, columns]));
    await client.exec(`DECLARE migration_rows NO SCROLL CURSOR FOR
      SELECT to_jsonb(migration_row)::text AS row_data
      FROM (SELECT ${columns.map(identifier).join(',')}
        FROM ${identifier(table.schema)}.${identifier(table.name)}) AS migration_row ORDER BY 1`);
    let rows = 0;
    for (;;) {
      const batch = await client.query<{ row_data: string }>('FETCH FORWARD 256 FROM migration_rows');
      if (!batch.rows.length) break;
      for (const row of batch.rows) {
        hash.update(String(Buffer.byteLength(row.row_data))).update(':').update(row.row_data);
        rows++;
      }
    }
    await client.exec('CLOSE migration_rows');
    result.push({ schema: table.schema, name: table.name, columns, rows, sha256: hash.digest('hex') });
  }
  await client.exec('ROLLBACK');
  return result;
}
