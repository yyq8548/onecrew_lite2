import { mkdir, chmod } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely, sql } from 'kysely';
import type { V1TeableDatabase } from '@teable/v2-postgres-schema';
import { createChildContainer, Lifecycle } from '@teable/v2-di';
import { registerV2PostgresStateAdapter } from '@teable/v2-adapter-repository-postgres';
import {
  PostgresUnitOfWork,
  v2DataDbTokens,
  v2MetaDbTokens,
} from '@teable/v2-adapter-db-postgres-shared';
import {
  createTypeValidationStrategy,
  registerV2TableRepositoryPostgresAdapter,
} from '@teable/v2-adapter-table-repository-postgres';
import { registerV2ImportServices } from '@teable/v2-import';
import { PapaparseCsvParser } from '@teable/v2-adapter-csv-parser-papaparse';
import {
  AsyncMemoryEventBus,
  MemoryCommandBus,
  MemoryQueryBus,
  NoopLogger,
  NoopRealtimeEngine,
  NoopTracer,
  StaticTableDataSafetyLimitPlugin,
  TableDataSafetyLimitCommandBusMiddleware,
  registerV2CoreServices,
  v2CoreTokens,
} from '@teable/v2-core';
import { databaseMigrationProvider } from './database-schema.js';
import { migrateLibraryDatabase } from './database-migrate.js';
import { acquireDatabaseLock } from './runtime.js';
import { TABLE_CAPACITY } from '@onecrew/contracts/tables';

export async function openDatabase(rootDir: string) {
  const dbDir = path.resolve(rootDir, 'pglite');
  await mkdir(dbDir, { recursive: true, mode: 0o700 });
  if (process.platform !== 'win32') await chmod(dbDir, 0o700);

  const lease = await acquireDatabaseLock(rootDir);
  if (!lease) throw new Error('library_database_busy');
  let client: PGlite;
  try {
    client = await PGlite.create(dbDir);
  } catch (error) {
    await lease.close();
    throw error;
  }
  let db: Kysely<V1TeableDatabase> | undefined;
  let storageUpgraded = false;
  const container = createChildContainer();
  const scheduledEventDrains = new Set<Promise<void>>();
  let closed = false;
  const close = async () => {
    if (closed) return;
    closed = true;
    let failure: unknown;
    while (scheduledEventDrains.size > 0) {
      await Promise.allSettled([...scheduledEventDrains]);
    }
    for (const release of [() => container.dispose(), () => (db ? db.destroy() : client.close())]) {
      try {
        await release();
      } catch (error) {
        failure ??= error;
      }
    }
    // Keep the lease if the engine could not close: no second writer may enter.
    if (client.closed) await lease.close();
    if (failure !== undefined) throw failure;
  };
  try {
    await client.waitReady;
    if (process.platform !== 'win32') await chmod(dbDir, 0o700);
    db = new Kysely<V1TeableDatabase>({ dialect: new PGliteDialect(client) });
    storageUpgraded =
      (await migrateLibraryDatabase(client, db, databaseMigrationProvider, rootDir)) === 'upgraded';
    const version = await sql<{
      server_version_num: string;
    }>`SELECT current_setting('server_version_num') AS server_version_num`.execute(db);
    if (Number(version.rows[0]?.server_version_num) < 160000)
      throw new Error('onecrew_lite requires PostgreSQL 16 or newer');
    const limits = {
      tableSchema: {
        maxRowsPerTable: TABLE_CAPACITY.maxRows,
        maxFieldsPerTable: TABLE_CAPACITY.maxColumns,
        maxCreateTableFields: TABLE_CAPACITY.maxColumns,
        maxCreateTableRecords: TABLE_CAPACITY.maxRows,
      },
      displayText: { maxNameLength: 255 },
    };
    container.registerInstance(v2MetaDbTokens.db, db);
    container.registerInstance(v2DataDbTokens.db, db);
    const postgresConfig = { pg: { connectionString: `pglite://${dbDir}` } };
    container.registerInstance(v2MetaDbTokens.config, postgresConfig);
    container.registerInstance(v2DataDbTokens.config, postgresConfig);
    await registerV2PostgresStateAdapter(container, {
      db,
      recordCountDb: db,
      ensureSchema: false,
      tableMaxRowLimit: TABLE_CAPACITY.maxRows,
    });
    const typeValidationStrategy = await createTypeValidationStrategy(db);
    registerV2TableRepositoryPostgresAdapter(container, {
      db,
      metaDb: db,
      typeValidationStrategy,
      tableDataSafetyLimits: limits,
      computedUpdate: { hybridConfig: { dispatchMode: 'external' } },
    });
    container.registerInstance(
      v2CoreTokens.unitOfWork,
      new PostgresUnitOfWork(db, db, postgresConfig, postgresConfig),
    );
    container.registerInstance(v2CoreTokens.logger, new NoopLogger());
    container.registerInstance(v2CoreTokens.tracer, new NoopTracer());
    container.registerInstance(v2CoreTokens.realtimeEngine, new NoopRealtimeEngine());
    container.registerInstance(v2CoreTokens.csvParser, new PapaparseCsvParser());
    container.registerInstance(v2CoreTokens.hasher, {
      hash: (value: string) => createHash('sha256').update(value).digest('hex'),
    });
    const commandBus = new MemoryCommandBus(container, [
      new TableDataSafetyLimitCommandBusMiddleware(new StaticTableDataSafetyLimitPlugin(limits)),
    ]);
    container.registerInstance(v2CoreTokens.commandBus, commandBus);
    container.registerInstance(v2CoreTokens.internalCommandBus, commandBus);
    container.registerInstance(v2CoreTokens.queryBus, new MemoryQueryBus(container));
    container.registerInstance(
      v2CoreTokens.eventBus,
      new AsyncMemoryEventBus(container, {
        recordPublishedEvents: false,
        schedule: (task) => {
          const drain = Promise.resolve().then(task);
          scheduledEventDrains.add(drain);
          void drain.then(
            () => scheduledEventDrains.delete(drain),
            (error: unknown) => {
              scheduledEventDrains.delete(drain);
              console.error(
                `Teable event drain failed: ${error instanceof Error ? error.message : String(error)}`,
              );
            },
          );
        },
        onError: ({ error }) => console.error(`Teable event handler failed: ${error}`),
      }),
    );
    registerV2CoreServices(container, { lifecycle: Lifecycle.Singleton });
    registerV2ImportServices(container);
    return { db, container, dbDir, close };
  } catch (error) {
    await close().catch(() => undefined);
    // Startup errors are printed by the CLI. Preserve the commit boundary, not
    // an internal SQL/provider error body that may contain customer data.
    // eslint-disable-next-line preserve-caught-error -- intentional public error redaction
    if (storageUpgraded) throw new Error('library_upgrade_runtime_initialization_failed');
    throw error;
  }
}

export type LocalDatabase = Awaited<ReturnType<typeof openDatabase>>;
