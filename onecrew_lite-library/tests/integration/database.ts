import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { sql } from 'kysely';
import { openDatabase } from '../../src/database.js';
import { createTenantService } from '../../src/modules/tenancy/service.js';

const withTimeout = async <T>(promise: Promise<T>, ms = 30_000): Promise<T> => {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Database integration timed out after ${ms}ms`)),
          ms,
        );
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
};

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-database-'));
try {
  const opened = await withTimeout(openDatabase(root));
  await assert.rejects(openDatabase(root), /library_database_busy/);
  const db = opened.db;
  const baseCount = await db
    .selectFrom('base')
    .select(({ fn }) => fn.countAll<number>().as('count'))
    .executeTakeFirstOrThrow();
  assert.equal(Number(baseCount.count), 0);
  for (const table of [
    'onecrew_tenants',
    'onecrew_field_types',
    'onecrew_search_jobs',
    'onecrew_access_accounts',
  ]) {
    const result = await db.introspection.getTables();
    assert.ok(
      result.some((entry) => entry.name === table),
      `missing ${table}`,
    );
  }
  const fieldCount = await sql<{
    count: string;
  }>`SELECT count(*)::text AS count FROM onecrew_field_types`.execute(db);
  const searchCount = await sql<{
    count: string;
  }>`SELECT count(*)::text AS count FROM onecrew_search_jobs`.execute(db);
  const accessCount = await sql<{
    count: string;
  }>`SELECT count(*)::text AS count FROM onecrew_access_accounts`.execute(db);
  assert.deepEqual(
    [fieldCount.rows[0]?.count, searchCount.rows[0]?.count, accessCount.rows[0]?.count].map(Number),
    [0, 0, 0],
  );
  await db.schema
    .createTable('database_transaction_probe')
    .addColumn('id', 'text', (c) => c.primaryKey())
    .execute();

  const transactions = await Promise.allSettled([
    db.transaction().execute(async (trx) => {
      await sql`INSERT INTO database_transaction_probe (id) VALUES ('committed')`.execute(trx);
    }),
    db.transaction().execute(async (trx) => {
      await sql`INSERT INTO database_transaction_probe (id) VALUES ('rolled-back')`.execute(trx);
      throw new Error('intentional rollback');
    }),
  ]);
  assert.equal(transactions.filter((entry) => entry.status === 'fulfilled').length, 1);
  assert.equal(transactions.filter((entry) => entry.status === 'rejected').length, 1);
  const probeRows = await sql<{ id: string }>`SELECT id FROM database_transaction_probe`.execute(
    db,
  );
  assert.deepEqual(
    probeRows.rows.map((row) => row.id),
    ['committed'],
  );
  await db.schema.dropTable('database_transaction_probe').execute();
  const principal = { id: 'database-test-user', email: 'database-test@example.test' };
  const firstTenant = await createTenantService(db).resolve(principal);
  assert.equal(firstTenant.user.id, principal.id);
  await withTimeout(opened.close());

  const reopened = await withTimeout(openDatabase(root));
  const reopenedBaseCount = await reopened.db
    .selectFrom('base')
    .select(({ fn }) => fn.countAll<number>().as('count'))
    .executeTakeFirstOrThrow();
  assert.equal(Number(reopenedBaseCount.count), 1);
  const reopenedTenant = await createTenantService(reopened.db).resolve(principal);
  assert.equal(reopenedTenant.tenantId, firstTenant.tenantId);
  assert.equal(reopenedTenant.baseId, firstTenant.baseId);
  await withTimeout(reopened.close());
  console.log('Database integration passed');
} finally {
  await rm(root, { recursive: true, force: true });
}
