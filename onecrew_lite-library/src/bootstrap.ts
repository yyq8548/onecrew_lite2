import { runtimePaths } from './runtime.js';
import { openDatabase } from './database.js';
import { AccessService } from './modules/access/service.js';
import { createCredentialStore } from './modules/access/store.js';
import { createTableData } from './modules/tables/data.js';
import { createTableService, normalizePublicWebUrl } from './modules/tables/service.js';
import { recoverInterruptedSearches } from './modules/search/legacy-recovery.js';
import { createTenantService } from './modules/tenancy/service.js';

/** Compose the persistent database and the shared application use cases. */
export const bootstrap = async (
  rootDir = runtimePaths().dataDir,
  accessOverride?: AccessService,
) => {
  const publicWebUrl = normalizePublicWebUrl(
    process.env.PUBLIC_WEB_URL ?? `http://127.0.0.1:${process.env.PORT ?? 8191}`,
  );
  const database = await openDatabase(rootDir);
  const access = accessOverride ?? new AccessService(createCredentialStore(database.db));
  const tenants = createTenantService(database.db);
  const data = await createTableData(database);
  await recoverInterruptedSearches(database.db);
  const service = createTableService(data, publicWebUrl);
  return {
    database,
    data,
    service,
    access,
    tenants,
    dbDir: database.dbDir,
    publicWebUrl,
  };
};
