import assert from 'node:assert/strict';
import { mkdtemp, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { sql } from 'kysely';
import { bootstrap } from '../../src/bootstrap.js';
import { createCredentialStore } from '../../src/modules/access/store.js';
import { credentialSchema, type Credential } from '../../src/modules/access/client.js';

const accountDb = (db: Awaited<ReturnType<typeof bootstrap>>['database']['db']) =>
  db.withTables<{
    onecrew_access_accounts: {
      access_user_id: string;
      credential: unknown | null;
      is_active: boolean;
      created_at: string;
      updated_at: string;
    };
  }>();

const credential = (userId: string, suffix: string): Credential =>
  credentialSchema.parse({
    bearer: `${userId}-${suffix}`.padEnd(80, 'x'),
    installation: {
      installation_id: `11111111-1111-4111-8111-${suffix.padStart(12, '0')}`,
      credential_id: `22222222-2222-4222-8222-${suffix.padStart(12, '0')}`,
      access_user_id: userId,
      access_device_id: 'dev_aaaaaaaaaaaaaaaaaaaaaaaa',
      access_pairing_id: 'pair_bbbbbbbbbbbbbbbbbbbbbbbb',
      source_instance_id: `33333333-3333-4333-8333-${suffix.padStart(12, '0')}`,
      client_platform: 'macos',
      app_version: '0.1.0',
      status: 'active',
      revision: 1,
      created_at: 1,
      updated_at: 1,
      last_seen_at: null,
      revoked_at: null,
    },
  });

const root = await mkdtemp(path.join(tmpdir(), 'onecrew-access-store-'));
let runtime = await bootstrap(root);
try {
  if (process.platform !== 'win32')
    assert.equal((await stat(path.join(root, 'pglite'))).mode & 0o777, 0o700);
  const store = createCredentialStore(runtime.database.db);
  let db = accountDb(runtime.database.db);
  const accountA = credential('user-a', '1');
  const accountB = credential('user-b', '2');

  await store.save(accountA);
  assert.deepEqual(await store.load(), accountA);

  await runtime.data.close();
  runtime = await bootstrap(root);
  db = accountDb(runtime.database.db);
  const restored = createCredentialStore(runtime.database.db);
  assert.deepEqual(await restored.load(), accountA);

  await db.schema
    .alterTable('onecrew_access_accounts')
    .addCheckConstraint('access_store_test_reject_user_b', sql`access_user_id <> 'user-b'`)
    .execute();
  await assert.rejects(restored.save(accountB));
  assert.deepEqual(await restored.load(), accountA);
  const accountAfterRollback = await db
    .selectFrom('onecrew_access_accounts')
    .selectAll()
    .where('access_user_id', '=', 'user-a')
    .executeTakeFirstOrThrow();
  assert.equal(accountAfterRollback.is_active, true);
  assert.deepEqual(accountAfterRollback.credential, accountA);
  await db.schema
    .alterTable('onecrew_access_accounts')
    .dropConstraint('access_store_test_reject_user_b')
    .execute();

  await restored.save(accountB);
  assert.deepEqual(await restored.load(), accountB);
  const accountsAfterB = await db.selectFrom('onecrew_access_accounts').selectAll().execute();
  assert.equal(accountsAfterB.length, 2);
  assert.equal(accountsAfterB.find((row) => row.access_user_id === 'user-a')?.is_active, false);
  assert.deepEqual(
    accountsAfterB.find((row) => row.access_user_id === 'user-a')?.credential,
    accountA,
  );
  await assert.rejects(
    db
      .updateTable('onecrew_access_accounts')
      .set({ is_active: true })
      .where('access_user_id', '=', 'user-a')
      .execute(),
  );

  await restored.clear();
  assert.equal(await restored.load(), undefined);
  const accountsAfterClear = await db.selectFrom('onecrew_access_accounts').selectAll().execute();
  assert.deepEqual(
    accountsAfterClear.find((row) => row.access_user_id === 'user-a')?.credential,
    accountA,
  );
  assert.equal(accountsAfterClear.find((row) => row.access_user_id === 'user-b')?.credential, null);
  assert.equal(accountsAfterClear.find((row) => row.access_user_id === 'user-b')?.is_active, false);

  await db
    .updateTable('onecrew_access_accounts')
    .set({
      is_active: true,
      credential: {
        ...accountB,
        installation: { ...accountB.installation, access_user_id: 'other-user' },
      },
    })
    .where('access_user_id', '=', 'user-b')
    .execute();
  await assert.rejects(restored.load());

  await assert.rejects(
    db
      .updateTable('onecrew_access_accounts')
      .set({ is_active: true, credential: null })
      .where('access_user_id', '=', 'user-b')
      .execute(),
  );
  await db
    .updateTable('onecrew_access_accounts')
    .set({ is_active: false, credential: {} })
    .where('access_user_id', '=', 'user-b')
    .execute();
  await db
    .updateTable('onecrew_access_accounts')
    .set({ is_active: true })
    .where('access_user_id', '=', 'user-b')
    .execute();
  await assert.rejects(restored.load());

  await assert.rejects(
    restored.save({ ...accountA, installation: { ...accountA.installation, access_user_id: '' } }),
  );
  await db
    .updateTable('onecrew_access_accounts')
    .set({ is_active: false, credential: null })
    .where('access_user_id', '=', 'user-b')
    .execute();
  assert.equal(await restored.load(), undefined);
  console.log(
    'PGlite access account persistence, per-user rows, fail-closed loading, and logout passed.',
  );
} finally {
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
