import type { LocalDatabase } from '../../database.js';
import { credentialSchema, type Credential } from './client.js';
import type { CredentialStore } from './service.js';

type AccessDatabase = {
  onecrew_access_accounts: {
    access_user_id: string;
    credential: unknown | null;
    is_active: boolean;
    created_at: string;
    updated_at: string;
  };
};

const accessDb = (db: LocalDatabase['db']) => db.withTables<AccessDatabase>();

const parseStoredCredential = (value: unknown, userId: string): Credential => {
  const credential = credentialSchema.parse(value);
  if (credential.installation.access_user_id !== userId)
    throw new Error('Stored credential identity mismatch');
  return credential;
};

export const createCredentialStore = (baseDb: LocalDatabase['db']): CredentialStore => {
  const db = accessDb(baseDb);
  return {
    async load() {
      const row = await db
        .selectFrom('onecrew_access_accounts')
        .select(['access_user_id', 'credential'])
        .where('is_active', '=', true)
        .executeTakeFirst();
      if (!row) return undefined;
      if (row.credential === null) throw new Error('Active account has no credential');
      return parseStoredCredential(row.credential, row.access_user_id);
    },
    async save(value) {
      const credential = credentialSchema.parse(value);
      const userId = credential.installation.access_user_id;
      const now = new Date().toISOString();
      await db.transaction().execute(async (trx) => {
        await trx
          .updateTable('onecrew_access_accounts')
          .set({ is_active: false, updated_at: now })
          .where('is_active', '=', true)
          .execute();
        await trx
          .insertInto('onecrew_access_accounts')
          .values({
            access_user_id: userId,
            credential,
            is_active: true,
            created_at: now,
            updated_at: now,
          })
          .onConflict((conflict) =>
            conflict.column('access_user_id').doUpdateSet({
              credential,
              is_active: true,
              updated_at: now,
            }),
          )
          .execute();
      });
    },
    async clear() {
      await db.transaction().execute(async (trx) => {
        await trx
          .updateTable('onecrew_access_accounts')
          .set({ credential: null, is_active: false, updated_at: new Date().toISOString() })
          .where('is_active', '=', true)
          .execute();
      });
    },
  };
};
