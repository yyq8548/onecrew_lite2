import { Hono } from 'hono';
export { createSystemRoutes } from './system.js';
export { TableDataError } from './modules/tables/data.js';
import { openDatabase } from './database.js';
import { createTableData } from './modules/tables/data.js';
import { createTableRoutes } from './modules/tables/http.js';
import { createTableService, normalizePublicWebUrl } from './modules/tables/service.js';
import { recoverInterruptedSearches } from './modules/search/legacy-recovery.js';
import {
  createTenantService,
  type TenantContext,
  type TenantHttpEnv,
} from './modules/tenancy/service.js';

/** 统一服务进程内的表格模块；身份由主服务验证，不创建第二个 Access 服务。 */
export async function createEmbeddedTables(rootDir: string, publicWebUrl: string, options: { recoverInterruptedSearches?: boolean } = {}) {
  const origin = normalizePublicWebUrl(publicWebUrl);
  const database = await openDatabase(rootDir);
  let ready = false;
  try {
    const tenants = createTenantService(database.db);
    const data = await createTableData(database);
    if (options.recoverInterruptedSearches !== false) await recoverInterruptedSearches(database.db);
    const service = createTableService(data, origin);
    ready = true;
    return {
      data,
      service,
      tenants,
      close: () => data.close(),
      createRoutes(
        resolveContext: (request: Request) => Promise<TenantContext>,
      ): Hono<TenantHttpEnv> {
        const app = new Hono<TenantHttpEnv>();
        app.use('*', async (context, next) => {
          const selected = await resolveContext(context.req.raw);
          const actual = await tenants.resolve(selected.user);
          if (actual.tenantId !== selected.tenantId || actual.baseId !== selected.baseId) {
            throw Object.assign(new Error('The selected account does not own this table context'), {
              code: 'access.account_changed',
              status: 409,
            });
          }
          context.set('tenant', actual);
          await next();
        });
        app.route('/', createTableRoutes(service));
        return app;
      },
    };
  } finally {
    if (!ready) await database.close();
  }
}
