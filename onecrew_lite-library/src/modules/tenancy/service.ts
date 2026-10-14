import { randomBytes } from 'node:crypto';
import { BaseId } from '@teable/v2-core';
import type { Kysely } from 'kysely';
import { sql } from 'kysely';
import type { V1TeableDatabase } from '@teable/v2-postgres-schema';

export type TenantContext = Readonly<{
  tenantId: string;
  baseId: string;
  user: Readonly<{ id: string; email: string }>;
}>;

export type TenantHttpEnv = { Variables: { tenant: TenantContext } };
export type PrincipalResolver = (request: Request) => Promise<{ id: string; email: string }>;

type TenantRow = {
  id: string;
  owner_user_id: string;
  space_id: string;
  base_id: string;
};

export type TenantService = {
  resolve(user: { id: string; email: string }): Promise<TenantContext>;
};

export function createRequestTenantResolver(
  tenants: TenantService,
  principalResolver: PrincipalResolver,
): (request: Request) => Promise<TenantContext> {
  return async (request) => {
    const user = await principalResolver(request);
    const accountId = request.headers.get('X-OneCrew-Account-Id');
    if (accountId !== null && accountId !== user.id) {
      throw Object.assign(new Error('The selected OneCrew account changed; reload the page.'), {
        status: 409,
        code: 'access.account_changed',
      });
    }
    return tenants.resolve(user);
  };
}

export function createTenantService(database: Kysely<V1TeableDatabase>): TenantService {
  return {
    async resolve(user) {
      if (!user.id || !user.email) throw new Error('A verified user principal is required');
      const existing = await sql<TenantRow>`
        SELECT id, owner_user_id, space_id, base_id
        FROM onecrew_tenants
        WHERE owner_user_id = ${user.id}
      `.execute(database);
      if (existing.rows[0]) return contextFor(existing.rows[0], user);

      const tenantId = `ten${randomBytes(12).toString('hex')}`;
      const spaceId = `spc${randomBytes(8).toString('hex')}`;
      const baseId = BaseId.generate()._unsafeUnwrap().toString();
      const now = new Date().toISOString();
      return database.transaction().execute(async (trx) => {
        const inserted = await sql<{ id: string }>`
          INSERT INTO onecrew_tenants (id, owner_user_id, space_id, base_id, created_at, updated_at)
          VALUES (${tenantId}, ${user.id}, ${spaceId}, ${baseId}, ${now}, ${now})
          ON CONFLICT (owner_user_id) DO NOTHING
          RETURNING id
        `.execute(trx);

        if (inserted.rows.length === 0) {
          const winner = await sql<TenantRow>`
            SELECT id, owner_user_id, space_id, base_id
            FROM onecrew_tenants
            WHERE owner_user_id = ${user.id}
          `.execute(trx);
          const row = winner.rows[0];
          if (!row) throw new Error('Tenant creation did not produce a tenant');
          return contextFor(row, user);
        }

        await trx
          .insertInto('space')
          .values({ id: spaceId, name: 'OneCrew', created_by: user.id })
          .execute();
        await trx
          .insertInto('base')
          .values({
            id: baseId,
            space_id: spaceId,
            name: 'OneCrew',
            order: 1,
            created_by: user.id,
          })
          .execute();
        return { tenantId, baseId, user: { id: user.id, email: user.email } };
      });
    },
  };
}

const contextFor = (row: TenantRow, user: { id: string; email: string }): TenantContext => ({
  tenantId: row.id,
  baseId: row.base_id,
  user: { id: user.id, email: user.email },
});
