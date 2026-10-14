import {
  ActorId,
  type RecordBulkUpdateService,
  type ITableRecordQueryRepository,
  RecordByIdsSpec,
  RecordId,
  CreateFieldCommand,
  CreateFieldResult,
  CreateRecordsCommand,
  type CreateRecordsResult,
  CreateTableCommand,
  CreateTableResult,
  RenameTableCommand,
  DeleteRecordsCommand,
  DeleteRecordsResult,
  GetTableByIdQuery,
  GetTableByIdResult,
  ListTableRecordsQuery,
  ListTableRecordsResult,
  ListTablesQuery,
  ListTablesResult,
  ImportCsvCommand,
  ImportExcelCommand,
  UpdateFieldCommand,
  type ImportCsvResult,
  type ImportExcelResult,
  type UpdateFieldResult,
  UpdateRecordsCommand,
  UpdateRecordsResult,
  domainError,
  err,
  ok,
  v2CoreTokens,
} from '@teable/v2-core';
import type {
  DomainError,
  ICommandBus,
  IExecutionContext,
  IQueryBus,
  IUnitOfWork,
  IPublicCommand,
  Result,
} from '@teable/v2-core';
import { resolvePostgresDbOrTx } from '@teable/v2-adapter-db-postgres-shared';
import pLimit from 'p-limit';
import type { LocalDatabase } from '../../database.js';
import { TABLE_CAPACITY } from '@onecrew/contracts/tables';
import type { ColumnInput, DataType } from '@onecrew/contracts/tables';
import type { TenantContext } from '../tenancy/service.js';
import {
  discoveryIdentitySchema,
  discoveryIdentityReadSchema,
  identitySourceConflict,
  identityVersionConflict,
  type DiscoveryIdentityProjection,
  type DiscoveryIdentityRead,
  type DiscoveryIdentityState,
  type DiscoveryIdentityDatabase,
} from './discovery-identity.js';
import {
  discoveryProjectionSchema,
  discoveryIngestSchema,
  discoveryStateSchema,
  discoveryCampaignSchema,
  discoveryTableSchema,
  projectionConflict,
  projectionFields,
  projectionValues,
  type DiscoveryProjection,
  type DiscoveryIngest,
  type DiscoveryStateInput,
  type DiscoveryTableInput,
  type DiscoveryProjectionDatabase,
  type ProjectionRow,
} from './discovery-projection.js';

type TableAggregate = CreateTableResult['table'];
type FieldTypesDatabase = {
  onecrew_field_types: {
    table_id: string;
    field_id: string;
    data_type: string;
  };
} & DiscoveryProjectionDatabase &
  DiscoveryIdentityDatabase;

export type ColumnDto = { id: string; name: string; data_type: DataType; read_only?: true };
export type TableDto = { id: string; name: string; columns: ColumnDto[] };
export type RowDto = { id: string; values: Record<string, unknown> };
export type DiscoveryCreatorView = DiscoveryIdentityState & {
  sources: Array<{ search_hit_id: string; verdict: string; reasons: string[]; contacts: ProjectionRow['contacts']; row: RowDto }>;
};

export class TableDataError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details?: unknown;
  constructor(message: string, status = 400, code = 'engine.error', details?: unknown) {
    super(message);
    this.name = 'TableDataError';
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

const API_TO_TEABLE: Record<DataType, string> = {
  string: 'singleLineText',
  int: 'number',
  float: 'number',
  bool: 'checkbox',
  date: 'date',
  people: 'singleLineText',
  company: 'singleLineText',
  website: 'singleLineText',
  email: 'singleLineText',
  telephone: 'singleLineText',
  score: 'singleLineText',
};
const TEABLE_TO_DEFAULT: Record<string, DataType> = {
  singleLineText: 'string',
  longText: 'string',
  number: 'float',
  checkbox: 'bool',
  date: 'date',
};
const COMPLEX_TYPES = new Set<DataType>(['people', 'company', 'score']);
type FieldLike = {
  id(): { toString(): string };
  name(): { toString(): string };
  type(): { toString(): string };
};
type TableExecutionContext = IExecutionContext & { actorName: string; actorEmail: string };

const unwrap = <T>(result: Result<T, DomainError>): T => {
  if (result.isOk()) return result.value;
  const error = result.error;
  const text = `${error.code ?? ''} ${error.message ?? ''}`.toLocaleLowerCase();
  const status =
    error.tags?.[0] === 'not-found' || text.includes('not_found') || text.includes('not found')
      ? 404
      : error.tags?.[0] === 'conflict'
        ? 409
        : 400;
  throw new TableDataError(
    error.message ?? 'Teable operation failed',
    status,
    error.code ?? 'engine.error',
    error.details,
  );
};
const asColumn = (field: FieldLike, dataType: DataType): ColumnDto => ({
  id: field.id().toString(),
  name: field.name().toString(),
  data_type: dataType,
});
const asTable = (table: TableAggregate, fieldTypes = new Map<string, DataType>()): TableDto => ({
  id: table.id().toString(),
  name: table.name().toString(),
  columns: table
    .getFields()
    .map((field) =>
      asColumn(
        field,
        fieldTypes.get(field.id().toString()) ??
          TEABLE_TO_DEFAULT[field.type().toString()] ??
          'string',
      ),
    ),
});
const decodeValue = (dataType: DataType, value: unknown): unknown => {
  if (value === null || value === undefined || !COMPLEX_TYPES.has(dataType)) return value;
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value) as unknown;
  } catch {
    throw new TableDataError(
      `stored ${dataType} value is not valid structured data`,
      500,
      'data.invalid_structured_value',
    );
  }
};
const asCreatedRow = (
  record: {
    id(): { toString(): string };
    fields(): {
      entries(): ReadonlyArray<{ fieldId: { toString(): string }; value: { toValue(): unknown } }>;
    };
  },
  fieldTypes: Map<string, DataType>,
): RowDto => ({
  id: record.id().toString(),
  values: Object.fromEntries(
    record
      .fields()
      .entries()
      .map((entry) => [
        entry.fieldId.toString(),
        decodeValue(fieldTypes.get(entry.fieldId.toString()) ?? 'string', entry.value.toValue()),
      ]),
  ),
});
const encodeValue = (dataType: DataType, value: unknown): unknown =>
  value === null || value === undefined || !COMPLEX_TYPES.has(dataType)
    ? value
    : JSON.stringify(value);

export type TableData = {
  readonly dbDir: string;
  publishDiscoveryIdentity(
    ctx: TenantContext,
    input: DiscoveryIdentityProjection,
  ): Promise<DiscoveryIdentityState>;
  getDiscoveryIdentity(
    ctx: TenantContext,
    input: DiscoveryIdentityRead,
  ): Promise<DiscoveryIdentityState>;
  getDiscoveryCreatorView(ctx: TenantContext, tableId: string): Promise<DiscoveryCreatorView>;
  getDiscoveryCampaignTable(ctx: TenantContext, campaignId: string): Promise<TableDto | null>;
  ensureDiscoveryCampaignTable(ctx: TenantContext, input: DiscoveryTableInput): Promise<TableDto>;
  getDiscoveryState(
    ctx: TenantContext,
    input: DiscoveryStateInput,
  ): Promise<{
    table_id: string;
    campaign_id: string;
    bound: boolean;
    rows: ProjectionRow[];
    has_more: boolean;
  }>;
  // Trusted source snapshot only; not registered as an HTTP/MCP editing operation.
  applyDiscoveryProjection(
    ctx: TenantContext,
    input: DiscoveryProjection,
  ): Promise<{ table: TableDto; updated_rows: number; unchanged_rows: number }>;
  ingestDiscoverySources(
    ctx: TenantContext,
    input: DiscoveryIngest,
  ): Promise<{
    table: TableDto;
    inserted_rows: number;
    updated_rows: number;
    unchanged_rows: number;
    rows: Array<{ search_hit_id: string; row_id: string }>;
  }>;
  listTables(
    ctx: TenantContext,
    page: number,
    size: number,
    search?: string,
  ): Promise<{ tables: TableDto[]; page: number; size: number; has_more: boolean }>;
  getTable(ctx: TenantContext, tableId: string): Promise<TableDto>;
  createTable(ctx: TenantContext, name: string, columns: ColumnInput[]): Promise<TableDto>;
  addColumns(ctx: TenantContext, tableId: string, columns: ColumnInput[]): Promise<TableDto>;
  queryRows(
    ctx: TenantContext,
    tableId: string,
    page: number,
    size: number,
    sort?: { fieldId: string; order: 'asc' | 'desc' },
  ): Promise<{ rows: RowDto[]; page: number; size: number; has_more: boolean; total: number }>;
  createRows(
    ctx: TenantContext,
    tableId: string,
    rows: Array<{ values: Record<string, unknown> }>,
  ): Promise<RowDto[]>;
  updateRows(
    ctx: TenantContext,
    tableId: string,
    rows: Array<{ id: string; values: Record<string, unknown> }>,
  ): Promise<RowDto[]>;
  importFile(
    ctx: TenantContext,
    input: {
      name?: string;
      extension: 'csv' | 'xlsx';
      data: Uint8Array;
      sheetName?: string;
    },
  ): Promise<TableDto>;
  renameColumn(
    ctx: TenantContext,
    tableId: string,
    columnId: string,
    name: string,
  ): Promise<TableDto>;
  deleteRows(ctx: TenantContext, tableId: string, rowIds: string[]): Promise<string[]>;
  close(): Promise<void>;
};

export const createTableData = async (database: LocalDatabase): Promise<TableData> => {
  const { dbDir, db } = database;
  const write = pLimit(1);
  const commandBus = database.container.resolve<ICommandBus>(v2CoreTokens.commandBus);
  const queryBus = database.container.resolve<IQueryBus>(v2CoreTokens.queryBus);
  const unitOfWork = database.container.resolve<IUnitOfWork>(v2CoreTokens.unitOfWork);
  const createExecutionContext = (ctx: TenantContext): TableExecutionContext => {
    return {
      actorId: ActorId.create(ctx.user.id)._unsafeUnwrap(),
      actorName: ctx.user.email,
      actorEmail: ctx.user.email,
    };
  };
  const get = async (
    ctx: TenantContext,
    tableId: string,
    executionContext?: TableExecutionContext,
  ): Promise<TableAggregate> => {
    const resolvedContext = executionContext ?? createExecutionContext(ctx);
    const query = unwrap(GetTableByIdQuery.create({ baseId: ctx.baseId, tableId }));
    return unwrap(await queryBus.execute<typeof query, GetTableByIdResult>(resolvedContext, query))
      .table;
  };
  const runCommand = async <T>(
    ctx: TenantContext,
    command: unknown,
    executionContext?: TableExecutionContext,
  ): Promise<T> => {
    const resolvedContext = executionContext ?? createExecutionContext(ctx);
    return unwrap(
      await commandBus.execute<IPublicCommand, T>(resolvedContext, command as IPublicCommand),
    );
  };
  const runQuery = async <T>(
    ctx: TenantContext,
    query: unknown,
    executionContext?: TableExecutionContext,
  ): Promise<T> => {
    const resolvedContext = executionContext ?? createExecutionContext(ctx);
    return unwrap(await queryBus.execute<unknown, T>(resolvedContext, query));
  };
  const dbFor = (executionContext?: IExecutionContext) =>
    resolvePostgresDbOrTx(db, executionContext, 'data').withTables<FieldTypesDatabase>();
  const runAtomic = async <T>(
    ctx: TenantContext,
    work: (executionContext: TableExecutionContext) => Promise<T>,
  ): Promise<T> => {
    let originalDataError: TableDataError | undefined;
    const executionContext = createExecutionContext(ctx);
    const result = await unitOfWork.withTransaction(
      executionContext,
      async (transactionContext: IExecutionContext) => {
        try {
          return ok(await work(transactionContext as TableExecutionContext));
        } catch (error) {
          if (error instanceof TableDataError) originalDataError = error;
          return err(
            domainError.unexpected({
              message: error instanceof Error ? error.message : String(error),
            }),
          );
        }
      },
      { scope: 'data' },
    );
    if (originalDataError) throw originalDataError;
    return unwrap(result as never);
  };
  const readFieldTypes = async (tableId: string, executionContext?: IExecutionContext) => {
    const entries = (await dbFor(executionContext)
      .selectFrom('onecrew_field_types')
      .select(['field_id', 'data_type'])
      .where('table_id', '=', tableId)
      .execute()) as Array<{ field_id: string; data_type: string }>;
    return new Map<string, DataType>(
      entries.map((entry) => [entry.field_id, entry.data_type as DataType]),
    );
  };
  const readProtectedFields = async (tableId: string, executionContext?: IExecutionContext) => {
    const fields = await dbFor(executionContext)
      .selectFrom('onecrew_discovery_fields')
      .select('field_id')
      .where('table_id', '=', tableId)
      .execute();
    return new Set(fields.map((field) => field.field_id));
  };
  // Called only after tenant-scoped table lookup, inside the owner's write queue.
  const requireWritableFields = async (
    tableId: string,
    fieldIds: string[],
    executionContext?: IExecutionContext,
  ) => {
    const protectedFields = await readProtectedFields(tableId, executionContext);
    if (fieldIds.some((id) => protectedFields.has(id)))
      throw new TableDataError(
        'Discovery system fields are read-only',
        409,
        'data.system_field_read_only',
      );
  };
  const tableDto = async (table: TableAggregate, executionContext?: IExecutionContext) => {
    const tableId = table.id().toString();
    const dto = asTable(table, await readFieldTypes(tableId, executionContext));
    const protectedFields = await readProtectedFields(tableId, executionContext);
    for (const column of dto.columns) {
      if (protectedFields.has(column.id)) column.read_only = true;
    }
    return dto;
  };
  const saveFieldTypes = async (
    tableId: string,
    fields: ReadonlyArray<FieldLike>,
    columns: ColumnInput[],
    executionContext?: IExecutionContext,
  ) => {
    const byName = new Map(
      columns.map((column) => [column.name.trim().toLocaleLowerCase(), column.data_type]),
    );
    const db = dbFor(executionContext);
    for (const field of fields) {
      const dataType = byName.get(field.name().toString().trim().toLocaleLowerCase());
      if (!dataType) continue;
      await db
        .insertInto('onecrew_field_types')
        .values({ table_id: tableId, field_id: field.id().toString(), data_type: dataType })
        .onConflict((conflict) =>
          conflict.columns(['table_id', 'field_id']).doUpdateSet({ data_type: dataType }),
        )
        .execute();
    }
  };
  const applyProjection = async (
    ctx: TenantContext,
    input: DiscoveryProjection | DiscoveryIngest,
    ingest = false,
  ) => {
    const parsed = ingest
      ? discoveryIngestSchema.safeParse(input)
      : discoveryProjectionSchema.safeParse(input);
    if (!parsed.success)
      throw new TableDataError('Invalid discovery projection', 400, 'data.projection_invalid');
    const sources = ingest ? (parsed.data as DiscoveryIngest) : undefined;
    const snapshot: DiscoveryProjection = sources
      ? { ...sources, rows: [] }
      : (parsed.data as DiscoveryProjection);
    return write(() =>
      runAtomic(ctx, async (executionContext) => {
        const tableId = snapshot.table_id;
        let table = await get(ctx, tableId, executionContext);
        const scoreCells = new Map<string, Record<string, unknown>>();
        const db = dbFor(executionContext);
        const conflict = (code: string): never => {
          throw new TableDataError('Discovery projection conflicts with saved state', 409, code);
        };
        const bindings = await db
          .selectFrom('onecrew_discovery_tables')
          .selectAll()
          .where((eb) =>
            eb.or([
              eb('table_id', '=', tableId),
              eb.and([
                eb('tenant_id', '=', ctx.tenantId),
                eb('campaign_id', '=', snapshot.campaign_id),
              ]),
            ]),
          )
          .execute();
        if (
          bindings.some(
            (binding) =>
              binding.table_id !== tableId ||
              binding.tenant_id !== ctx.tenantId ||
              binding.campaign_id !== snapshot.campaign_id ||
              binding.projection_format !== 1,
          )
        )
          conflict('data.projection_table_conflict');
        const previous = await db
          .selectFrom('onecrew_discovery_rows')
          .selectAll()
          .where('table_id', '=', tableId)
          .execute();
        if (
          previous.some(
            (row) =>
              BigInt(row.source_revision) > BigInt(snapshot.source_revision) ||
              row.criteria_revision > snapshot.criteria_revision,
          )
        )
          conflict('data.projection_stale');
        const byRow = new Map(previous.map((row) => [row.row_id, row]));
        const byHit = new Map(previous.map((row) => [row.search_hit_id, row.row_id]));
        let insertedRows = 0;
        if (sources) {
          // A legacy table has no trustworthy row/hit ownership. Never adopt
          // its editable cells or bind its customer records by name.
          if (!bindings.length) {
            const records = await runQuery<ListTableRecordsResult>(
              ctx,
              unwrap(
                ListTableRecordsQuery.create({ baseId: ctx.baseId, tableId, limit: 1, offset: 0 }),
              ),
              executionContext,
            );
            if (records.records.length) conflict('data.projection_unmanaged_table');
          }
          await requireWritableFields(
            tableId,
            sources.rows.flatMap((row) => Object.keys(row.values)),
            executionContext,
          );
          const fields = new Set(table.getFields().map((field) => field.id().toString()));
          if (
            sources.rows.some((row) => Object.keys(row.values).some((field) => !fields.has(field)))
          )
            conflict('data.projection_field_conflict');
          const types = await readFieldTypes(tableId, executionContext);
          const categoryField = table.getFields().find((field) =>
            field.name().toString() === 'Content category')?.id().toString();
          const followersField = table.getFields().find((field) =>
            field.name().toString() === 'Followers')?.id().toString();
          for (const row of sources.rows) {
            const scores: Record<string, unknown> = {};
            for (const [field, value] of Object.entries(row.values)) {
              const score = types.get(field) === 'score';
              if (
                (score && value !== null && typeof value !== 'object') ||
                (!score && value !== null && typeof value === 'object')
              )
                conflict('data.projection_field_conflict');
              if (score) scores[field] = encodeValue('score', value);
              else if ((field === categoryField || field === followersField) && types.get(field) === 'string')
                scores[field] = value === '' ? null : value;
            }
            scoreCells.set(row.search_hit_id, scores);
          }
          const fresh = sources.rows.filter((row) => !byHit.has(row.search_hit_id));
          if (fresh.length) {
            const created = await runCommand<CreateRecordsResult>(
              ctx,
              unwrap(
                CreateRecordsCommand.create({
                  tableId,
                  typecast: false,
                  fieldKeyType: 'id',
                  records: fresh.map((row) => ({
                    fields: { ...row.values, ...scoreCells.get(row.search_hit_id) },
                  })),
                }),
              ),
              executionContext,
            );
            if (created.records.length !== fresh.length) conflict('data.projection_row_limit');
            for (const [index, record] of created.records.entries()) {
              byHit.set(fresh[index].search_hit_id, record.id().toString());
            }
            insertedRows = created.records.length;
          }
          snapshot.rows = sources.rows.map(({ values: _values, ...row }) => ({
            ...row,
            row_id: byHit.get(row.search_hit_id)!,
          }));
        }
        const incoming: ProjectionRow[] = snapshot.rows.map((row) => ({
          ...row,
          criteria_revision: snapshot.criteria_revision,
          source_revision: snapshot.source_revision,
        }));
        const changed: ProjectionRow[] = [];
        for (const row of incoming) {
          const existing = byRow.get(row.row_id);
          if (byHit.has(row.search_hit_id) && byHit.get(row.search_hit_id) !== row.row_id)
            conflict('data.projection_row_conflict');
          if (existing) {
            const code = projectionConflict(
              { ...existing, source_revision: Number(existing.source_revision) },
              row,
            );
            if (code) conflict(code);
          }
          if (!existing || BigInt(existing.source_revision) !== BigInt(row.source_revision))
            changed.push(row);
        }
        const owned = await db
          .selectFrom('onecrew_discovery_fields')
          .selectAll()
          .where('table_id', '=', tableId)
          .execute();
        const fieldIds = new Map(owned.map((field) => [field.role, field.field_id]));
        if (bindings.length) {
          if (
            owned.length !== projectionFields.length ||
            projectionFields.some(
              (field) =>
                !table
                  .getFields()
                  .some(
                    (stored) =>
                      stored.id().toString() === fieldIds.get(field.role) &&
                      stored.type().toString() === API_TO_TEABLE[field.data_type],
                  ),
            )
          )
            conflict('data.projection_field_conflict');
        } else {
          if (owned.length || previous.length) conflict('data.projection_table_conflict');
          // Never claim a similarly named customer field. Ambiguity leaves the table untouched.
          const names = new Set(
            table.getFields().map((field) => field.name().toString().toLocaleLowerCase()),
          );
          if (projectionFields.some((field) => names.has(field.name.toLocaleLowerCase())))
            conflict('data.projection_field_name_conflict');
          await db
            .insertInto('onecrew_discovery_tables')
            .values({
              table_id: tableId,
              tenant_id: ctx.tenantId,
              campaign_id: snapshot.campaign_id,
              projection_format: 1,
              created_at: new Date().toISOString(),
            })
            .execute();
          for (const field of projectionFields) {
            const before = new Set(table.getFields().map((entry) => entry.id().toString()));
            const result = await runCommand<CreateFieldResult>(
              ctx,
              unwrap(
                CreateFieldCommand.create(
                  {
                    baseId: ctx.baseId,
                    tableId,
                    field: { name: field.name, type: API_TO_TEABLE[field.data_type] },
                  },
                  { preloadedTable: table },
                ),
              ),
              executionContext,
            );
            table = result.table;
            const added = table.getFields().filter((entry) => !before.has(entry.id().toString()));
            if (added.length !== 1) conflict('data.projection_field_conflict');
            const fieldId = added[0].id().toString();
            fieldIds.set(field.role, fieldId);
            await db
              .insertInto('onecrew_discovery_fields')
              .values({
                table_id: tableId,
                field_id: fieldId,
                role: field.role,
              })
              .execute();
            await db
              .insertInto('onecrew_field_types')
              .values({
                table_id: tableId,
                field_id: fieldId,
                data_type: field.data_type,
              })
              .execute();
          }
        }
        const readRecords = async () => {
          // Reuse the authenticated aggregate, including just-created fields
          // that are provision-pending until this outer transaction commits.
          const reader = database.container.resolve<ITableRecordQueryRepository>(
            v2CoreTokens.tableRecordQueryRepository,
          );
          const result = unwrap(
            await reader.find(
              executionContext,
              table,
              RecordByIdsSpec.create(incoming.map((row) => unwrap(RecordId.create(row.row_id)))),
              {
                mode: 'stored',
                includeTotal: false,
                projectionFieldIds: table
                  .getFields()
                  .filter((field) =>
                    [
                      ...fieldIds.values(),
                      ...[...scoreCells.values()].flatMap(Object.keys),
                    ].includes(field.id().toString()),
                  )
                  .map((field) => field.id()),
              },
            ),
          );
          const records = new Map(result.records.map((row) => [row.id, row.fields]));
          if (records.size !== incoming.length || incoming.some((row) => !records.has(row.row_id)))
            conflict('data.projection_row_missing');
          return records;
        };
        const cellsMatch = (cells: Record<string, unknown>, row?: ProjectionRow) => {
          const expected = row ? projectionValues(row) : {};
          return projectionFields.every(
            (field) =>
              (cells[fieldIds.get(field.role)!] ?? null) === (expected[field.role] ?? null),
          );
        };
        const before = await readRecords();
        // Adding the category export field must also populate older owned
        // rows, even when their qualification/source revision is unchanged.
        const enrichedFields = table.getFields().filter((field) =>
          ['Content category', 'Followers'].includes(field.name().toString())).map(field => field.id().toString());
        for (const categoryField of enrichedFields) for (const row of incoming) {
          const values = scoreCells.get(row.search_hit_id);
          if (values && Object.hasOwn(values, categoryField) &&
              (before.get(row.row_id)![categoryField] ?? null) !== values[categoryField] &&
              !changed.some((entry) => entry.row_id === row.row_id)) changed.push(row);
        }
        for (const row of incoming) {
          const existing = byRow.get(row.row_id);
          if (
            !cellsMatch(
              before.get(row.row_id)!,
              existing
                ? {
                    ...existing,
                    source_revision: Number(existing.source_revision),
                  }
                : undefined,
            )
          )
            conflict('data.projection_readback_conflict');
        }
        if (changed.length) {
          for (const row of changed) {
            const saved = {
              ...row,
              table_id: tableId,
              reasons: JSON.stringify(row.reasons),
              contacts: JSON.stringify(row.contacts),
              updated_at: new Date().toISOString(),
            };
            await db
              .insertInto('onecrew_discovery_rows')
              .values(saved)
              .onConflict((oc) => oc.columns(['table_id', 'row_id']).doUpdateSet(saved))
              .execute();
          }
          const command = unwrap(
            UpdateRecordsCommand.create({
              tableId,
              typecast: false,
              fieldKeyType: 'id',
              records: changed.map((row) => ({
                id: row.row_id,
                fields: {
                  ...scoreCells.get(row.search_hit_id),
                  ...Object.fromEntries(
                    Object.entries(projectionValues(row)).map(([role, value]) => [
                      fieldIds.get(role)!,
                      value,
                    ]),
                  ),
                },
              })),
            }),
          );
          // Newly created fields keep the table provision-pending until commit.
          // Reuse its authenticated aggregate with the engine's normal update
          // service instead of reloading an active-only table mid-transaction.
          const updates = database.container.resolve<RecordBulkUpdateService>(
            v2CoreTokens.recordBulkUpdateService,
          );
          unwrap(await updates.update(executionContext, { ...command, table }));
        }
        const after = await readRecords();
        if (incoming.some((row) => !cellsMatch(after.get(row.row_id)!, row)))
          conflict('data.projection_readback_conflict');
        if (
          incoming.some((row) =>
            Object.entries(scoreCells.get(row.search_hit_id) ?? {}).some(
              ([field, value]) => (after.get(row.row_id)![field] ?? null) !== value,
            ),
          )
        )
          conflict('data.projection_readback_conflict');
        return {
          table: await tableDto(table, executionContext),
          updated_rows: changed.length,
          unchanged_rows: incoming.length - changed.length,
          inserted_rows: insertedRows,
          rows: incoming.map((row) => ({ search_hit_id: row.search_hit_id, row_id: row.row_id })),
        };
      }),
    );
  };
  const identitySnapshot = async (
    ctx: TenantContext,
    input: { table_id: string; campaign_id?: string },
    publish?: DiscoveryIdentityProjection,
    includeSources = false,
  ): Promise<DiscoveryCreatorView> =>
    write(() =>
      runAtomic(ctx, async (executionContext) => {
        const table = await get(ctx, input.table_id, executionContext);
        const db = dbFor(executionContext);
        const fail = (code: string): never => {
          throw new TableDataError('Identity projection unavailable', 409, code);
        };
        const binding = await db
          .selectFrom('onecrew_discovery_tables')
          .selectAll()
          .where('table_id', '=', input.table_id)
          .executeTakeFirst();
        if (
          !binding ||
          binding.tenant_id !== ctx.tenantId ||
          (input.campaign_id !== undefined && binding.campaign_id !== input.campaign_id) ||
          binding.projection_format !== 1
        )
          fail('data.identity_owner_conflict');
        const prior = await db
          .selectFrom('onecrew_discovery_identity_snapshots')
          .selectAll()
          .where('table_id', '=', input.table_id)
          .executeTakeFirst();
        const previous = prior ? discoveryIdentitySchema.safeParse(prior.projection) : null;
        if (previous && !previous.success) fail('data.identity_stored_invalid');
        const saved = previous?.success ? previous.data : null;
        if (
          saved &&
          prior &&
          (saved.table_id !== prior.table_id ||
            saved.campaign_id !== binding!.campaign_id ||
            saved.criteria_revision !== prior.criteria_revision ||
            saved.source_revision !== Number(prior.source_revision) ||
            saved.identity_revision !== Number(prior.identity_revision) ||
            saved.source_hash !== prior.source_hash ||
            saved.identity_hash !== prior.identity_hash ||
            saved.projection_format !== prior.projection_format)
        )
          fail('data.identity_stored_invalid');
        if (saved && publish) {
          const conflict = identityVersionConflict(saved, publish);
          if (conflict) fail(conflict);
        }
        const projection = publish ?? saved;
        if (!projection) return { state: 'missing', projection: null, sources: [] };
        const stored = await db
          .selectFrom('onecrew_discovery_rows')
          .selectAll()
          .where('table_id', '=', input.table_id)
          .orderBy('search_hit_id')
          .limit(10001)
          .execute();
        const rows = stored.map((row) => ({
          ...row,
          source_revision: Number(row.source_revision),
        }));
        const conflict = identitySourceConflict(projection, rows);
        if (conflict) {
          if (publish) fail(conflict);
          return { state: 'sources_changed', projection: null, sources: [] };
        }
        // Verify the source records still exist and their protected cells agree in
        // this same transaction. Deleted rows must never survive as author groups.
        const fields = await db
          .selectFrom('onecrew_discovery_fields')
          .selectAll()
          .where('table_id', '=', input.table_id)
          .execute();
        const fieldIds = new Map(fields.map((field) => [field.role, field.field_id]));
        if (
          fields.length !== projectionFields.length ||
          projectionFields.some(
            (field) =>
              !table
                .getFields()
                .some(
                  (actual) =>
                    actual.id().toString() === fieldIds.get(field.role) &&
                    actual.type().toString() === API_TO_TEABLE[field.data_type],
                ),
          )
        )
          fail('data.projection_field_conflict');
        const reader = database.container.resolve<ITableRecordQueryRepository>(
          v2CoreTokens.tableRecordQueryRepository,
        );
        const sources: DiscoveryCreatorView['sources'] = [];
        const fieldTypes = includeSources
          ? await readFieldTypes(input.table_id, executionContext)
          : new Map<string, DataType>();
        for (let offset = 0; offset < rows.length; offset += 100) {
          const batch = rows.slice(offset, offset + 100);
          const records = unwrap(
            await reader.find(
              executionContext,
              table,
              RecordByIdsSpec.create(batch.map((row) => unwrap(RecordId.create(row.row_id)))),
              {
                mode: 'stored',
                includeTotal: false,
                projectionFieldIds: table
                  .getFields()
                  .filter(
                    (field) =>
                      includeSources || [...fieldIds.values()].includes(field.id().toString()),
                  )
                  .map((field) => field.id()),
              },
            ),
          );
          const byId = new Map(records.records.map((record) => [record.id, record.fields]));
          if (
            batch.some(
              (row) =>
                !byId.has(row.row_id) ||
                projectionFields.some(
                  (field) =>
                    (byId.get(row.row_id)![fieldIds.get(field.role)!] ?? null) !==
                    (projectionValues(row)[field.role] ?? null),
                ),
            )
          ) {
            if (publish) fail('data.projection_readback_conflict');
            return { state: 'sources_changed', projection: null, sources: [] };
          }
          if (includeSources) {
            for (const row of batch) {
              sources.push({
                search_hit_id: row.search_hit_id,
                verdict: row.verdict,
                reasons: row.reasons,
                contacts: row.contacts,
                row: {
                  id: row.row_id,
                  values: Object.fromEntries(
                    Object.entries(byId.get(row.row_id)!).map(([id, value]) => [
                      id,
                      decodeValue(fieldTypes.get(id) ?? 'string', value),
                    ]),
                  ),
                },
              });
            }
          }
        }
        if (publish && (!saved || publish.identity_revision !== saved.identity_revision)) {
          const values = {
            table_id: input.table_id,
            projection_format: publish.projection_format,
            criteria_revision: publish.criteria_revision,
            source_revision: publish.source_revision,
            source_hash: publish.source_hash,
            identity_revision: publish.identity_revision,
            identity_hash: publish.identity_hash,
            projection: JSON.stringify(publish),
            updated_at: new Date().toISOString(),
          };
          await db
            .insertInto('onecrew_discovery_identity_snapshots')
            .values(values)
            .onConflict((c) => c.column('table_id').doUpdateSet(values))
            .execute();
        }
        return { state: 'current', projection, sources };
      }),
    );
  return {
    dbDir,
    async publishDiscoveryIdentity(ctx, input) {
      const parsed = discoveryIdentitySchema.safeParse(input);
      if (!parsed.success)
        throw new TableDataError('Invalid identity projection', 400, 'data.identity_invalid');
      const { sources: _sources, ...state } = await identitySnapshot(ctx, parsed.data, parsed.data);
      return state;
    },
    async getDiscoveryIdentity(ctx, input) {
      const parsed = discoveryIdentityReadSchema.safeParse(input);
      if (!parsed.success)
        throw new TableDataError('Invalid identity query', 400, 'data.identity_invalid');
      const { sources: _sources, ...state } = await identitySnapshot(ctx, parsed.data);
      return state;
    },
    getDiscoveryCreatorView: (ctx, tableId) =>
      identitySnapshot(ctx, { table_id: tableId }, undefined, true),
    applyDiscoveryProjection: (ctx, input) => applyProjection(ctx, input),
    ingestDiscoverySources: (ctx, input) => applyProjection(ctx, input, true),
    async ensureDiscoveryCampaignTable(ctx, input) {
      const parsed = discoveryTableSchema.safeParse(input);
      if (!parsed.success)
        throw new TableDataError('Invalid discovery table', 400, 'data.projection_invalid');
      return write(() =>
        runAtomic(ctx, async (executionContext) => {
          const db = dbFor(executionContext);
          const existing = await db
            .selectFrom('onecrew_discovery_tables')
            .selectAll()
            .where('tenant_id', '=', ctx.tenantId)
            .where('campaign_id', '=', parsed.data.campaign_id)
            .executeTakeFirst();
          if (existing) {
            if (existing.projection_format !== 1)
              throw new TableDataError(
                'Unsupported discovery binding',
                409,
                'data.projection_table_conflict',
              );
            let updated = await get(ctx, existing.table_id, executionContext);
            const currentTypes = await readFieldTypes(existing.table_id, executionContext);
            const missing = parsed.data.columns.filter((column) => {
              const fields = updated
                .getFields()
                .filter((field) => field.name().toString() === column.name);
              if (
                fields.length > 1 ||
                (fields.length === 1 &&
                  currentTypes.get(fields[0].id().toString()) !== column.data_type)
              )
                throw new TableDataError(
                  'Discovery field type conflict',
                  409,
                  'data.projection_field_conflict',
                );
              return fields.length === 0;
            });
            for (const column of missing) {
              const command = unwrap(
                CreateFieldCommand.create(
                  {
                    baseId: ctx.baseId,
                    tableId: existing.table_id,
                    field: { name: column.name, type: API_TO_TEABLE[column.data_type] },
                  },
                  { preloadedTable: updated },
                ),
              );
              updated = (await runCommand<CreateFieldResult>(ctx, command, executionContext)).table;
            }
            await saveFieldTypes(existing.table_id, updated.getFields(), missing, executionContext);
            // Only migrate owner-generated legacy labels. The persisted name
            // then survives timezone changes, midnight and repeated syncs.
            if (
              [
                `OneCrew Sources ${parsed.data.campaign_id}`,
                `OneCrew Campaign ${parsed.data.campaign_id}`,
                ...(parsed.data.previous_auto_name ? [parsed.data.previous_auto_name] : []),
              ].includes(updated.name().toString()) &&
              updated.name().toString() !== parsed.data.name
            ) {
              const command = unwrap(
                RenameTableCommand.create({
                  baseId: ctx.baseId,
                  tableId: existing.table_id,
                  name: parsed.data.name,
                }),
              );
              updated = (
                await runCommand<{ table: TableAggregate }>(ctx, command, executionContext)
              ).table;
            }
            return tableDto(updated, executionContext);
          }
          const columns: ColumnInput[] = [
            ...parsed.data.columns,
            ...projectionFields.map(({ name, data_type }) => ({ name, data_type })),
          ];
          const names = columns.map((column) => column.name.trim().toLocaleLowerCase());
          if (new Set(names).size !== names.length)
            throw new TableDataError(
              'Discovery field names conflict',
              409,
              'data.projection_field_name_conflict',
            );
          const result = await runCommand<CreateTableResult>(
            ctx,
            unwrap(
              CreateTableCommand.create({
                baseId: ctx.baseId,
                name: parsed.data.name,
                fields: columns.map((column, index) => ({
                  name: column.name,
                  type: API_TO_TEABLE[column.data_type],
                  ...(index === 0 ? { isPrimary: true } : {}),
                })),
              }),
            ),
            executionContext,
          );
          const tableId = result.table.id().toString();
          await saveFieldTypes(tableId, result.table.getFields(), columns, executionContext);
          await db
            .insertInto('onecrew_discovery_tables')
            .values({
              table_id: tableId,
              tenant_id: ctx.tenantId,
              campaign_id: parsed.data.campaign_id,
              projection_format: 1,
              created_at: new Date().toISOString(),
            })
            .execute();
          for (const field of projectionFields) {
            const created = result.table
              .getFields()
              .filter((entry) => entry.name().toString() === field.name);
            if (created.length !== 1)
              throw new TableDataError(
                'Discovery field creation failed',
                409,
                'data.projection_field_conflict',
              );
            await db
              .insertInto('onecrew_discovery_fields')
              .values({
                table_id: tableId,
                field_id: created[0].id().toString(),
                role: field.role,
              })
              .execute();
          }
          return tableDto(result.table, executionContext);
        }),
      );
    },
    async getDiscoveryCampaignTable(ctx, campaignId) {
      if (!discoveryCampaignSchema.safeParse({ campaign_id: campaignId }).success)
        throw new TableDataError('Invalid discovery campaign', 400, 'data.projection_invalid');
      return write(() =>
        runAtomic(ctx, async (executionContext) => {
          const binding = await dbFor(executionContext)
            .selectFrom('onecrew_discovery_tables')
            .selectAll()
            .where('tenant_id', '=', ctx.tenantId)
            .where('campaign_id', '=', campaignId)
            .executeTakeFirst();
          if (!binding) return null;
          if (binding.projection_format !== 1)
            throw new TableDataError(
              'Unsupported discovery binding',
              409,
              'data.projection_table_conflict',
            );
          return tableDto(await get(ctx, binding.table_id, executionContext), executionContext);
        }),
      );
    },
    async getDiscoveryState(ctx, input) {
      const parsed = discoveryStateSchema.safeParse(input);
      if (!parsed.success)
        throw new TableDataError('Invalid discovery state query', 400, 'data.projection_invalid');
      const {
        table_id: tableId,
        campaign_id: campaignId,
        after_hit_id: after,
        limit,
      } = parsed.data;
      return write(() =>
        runAtomic(ctx, async (executionContext) => {
          const table = await get(ctx, tableId, executionContext);
          const db = dbFor(executionContext);
          const binding = await db
            .selectFrom('onecrew_discovery_tables')
            .selectAll()
            .where('table_id', '=', tableId)
            .executeTakeFirst();
          if (
            binding &&
            (binding.tenant_id !== ctx.tenantId ||
              binding.campaign_id !== campaignId ||
              binding.projection_format !== 1)
          )
            throw new TableDataError(
              'Discovery table ownership changed',
              409,
              'data.projection_table_conflict',
            );
          if (!binding)
            return {
              table_id: tableId,
              campaign_id: campaignId,
              bound: false,
              rows: [],
              has_more: false,
            };
          const stored = await db
            .selectFrom('onecrew_discovery_rows')
            .selectAll()
            .where('table_id', '=', tableId)
            .where('search_hit_id', '>', after ?? '')
            .orderBy('search_hit_id')
            .limit(limit + 1)
            .execute();
          const rows: ProjectionRow[] = stored.slice(0, limit).map((row) => ({
            row_id: row.row_id,
            search_hit_id: row.search_hit_id,
            candidate_id: row.candidate_id,
            native_author_id: row.native_author_id,
            verdict: row.verdict,
            reasons: row.reasons,
            evidence_version: row.evidence_version,
            contacts: row.contacts,
            criteria_revision: row.criteria_revision,
            source_revision: Number(row.source_revision),
          }));
          if (rows.length) {
            const owned = await db
              .selectFrom('onecrew_discovery_fields')
              .selectAll()
              .where('table_id', '=', tableId)
              .execute();
            const fields = new Map(owned.map((field) => [field.role, field.field_id]));
            if (
              owned.length !== projectionFields.length ||
              projectionFields.some(
                (field) =>
                  !table
                    .getFields()
                    .some(
                      (stored) =>
                        stored.id().toString() === fields.get(field.role) &&
                        stored.type().toString() === API_TO_TEABLE[field.data_type],
                    ),
              )
            )
              throw new TableDataError(
                'Discovery fields changed',
                409,
                'data.projection_field_conflict',
              );
            const reader = database.container.resolve<ITableRecordQueryRepository>(
              v2CoreTokens.tableRecordQueryRepository,
            );
            const result = unwrap(
              await reader.find(
                executionContext,
                table,
                RecordByIdsSpec.create(rows.map((row) => unwrap(RecordId.create(row.row_id)))),
                {
                  mode: 'stored',
                  includeTotal: false,
                  projectionFieldIds: table
                    .getFields()
                    .filter((field) => [...fields.values()].includes(field.id().toString()))
                    .map((field) => field.id()),
                },
              ),
            );
            const records = new Map(result.records.map((record) => [record.id, record.fields]));
            if (rows.some((row) => !records.has(row.row_id)))
              throw new TableDataError(
                'Discovery row was removed',
                409,
                'data.projection_row_missing',
              );
            if (
              rows.some((row) => {
                const values = projectionValues(row);
                return projectionFields.some(
                  (field) =>
                    (records.get(row.row_id)![fields.get(field.role)!] ?? null) !==
                    (values[field.role] ?? null),
                );
              })
            )
              throw new TableDataError(
                'Discovery values changed',
                409,
                'data.projection_readback_conflict',
              );
          }
          return {
            table_id: tableId,
            campaign_id: campaignId,
            bound: true,
            rows,
            has_more: stored.length > limit,
          };
        }),
      );
    },
    async listTables(ctx, page, size, search) {
      const result = await runQuery<ListTablesResult>(
        ctx,
        unwrap(
          ListTablesQuery.create({
            baseId: ctx.baseId,
            q: search || undefined,
            limit: size,
            offset: (page - 1) * size,
          }),
        ),
      );
      const tables = await Promise.all(result.tables.map((table) => tableDto(table)));
      return { tables, page, size, has_more: tables.length === size };
    },
    async getTable(ctx, tableId) {
      return tableDto(await get(ctx, tableId));
    },
    async createTable(ctx, name, columns) {
      return write(async () => {
        const fields = columns.map((column, index) => ({
          name: column.name,
          type: API_TO_TEABLE[column.data_type],
          ...(index === 0 ? { isPrimary: true } : {}),
        }));
        return runAtomic(ctx, async (executionContext) => {
          const result = await runCommand<CreateTableResult>(
            ctx,
            unwrap(CreateTableCommand.create({ baseId: ctx.baseId, name, fields })),
            executionContext,
          );
          await saveFieldTypes(
            result.table.id().toString(),
            result.table.getFields(),
            columns,
            executionContext,
          );
          return tableDto(result.table, executionContext);
        });
      });
    },
    async addColumns(ctx, tableId, columns) {
      return write(async () => {
        return runAtomic(ctx, async (executionContext) => {
          let updated = await get(ctx, tableId, executionContext);
          // Service validation can race another queued write. Do not let a
          // same-name add rewrite a newly owned field's data-type metadata.
          const requestedNames = new Set(
            columns.map((column) => column.name.trim().toLocaleLowerCase()),
          );
          await requireWritableFields(
            tableId,
            updated
              .getFields()
              .filter((field) =>
                requestedNames.has(field.name().toString().trim().toLocaleLowerCase()),
              )
              .map((field) => field.id().toString()),
            executionContext,
          );
          for (const column of columns) {
            const command = unwrap(
              CreateFieldCommand.create(
                {
                  baseId: ctx.baseId,
                  tableId,
                  field: { name: column.name, type: API_TO_TEABLE[column.data_type] },
                },
                { preloadedTable: updated },
              ),
            );
            const result = await runCommand<CreateFieldResult>(ctx, command, executionContext);
            updated = result.table;
          }
          await saveFieldTypes(tableId, updated.getFields(), columns, executionContext);
          return tableDto(updated, executionContext);
        });
      });
    },
    async queryRows(ctx, tableId, page, size, sort) {
      const table = await get(ctx, tableId);
      if (sort && !table.getFields().some((field) => field.id().toString() === sort.fieldId))
        throw new TableDataError(
          `unknown column id: ${sort.fieldId}`,
          404,
          'data.column_not_found',
        );
      const fieldTypes = await readFieldTypes(tableId);
      const query = unwrap(
        ListTableRecordsQuery.create({
          tableId,
          limit: size,
          offset: (page - 1) * size,
          includeTotal: true,
          sort: sort ? [sort] : undefined,
          fieldKeyType: 'id',
        }),
      );
      const result = await runQuery<ListTableRecordsResult>(ctx, query);
      const rows = result.records.map((record) => ({
        id: record.id,
        values: Object.fromEntries(
          Object.entries(record.fields).map(([fieldId, value]) => [
            fieldId,
            decodeValue(fieldTypes.get(fieldId) ?? 'string', value),
          ]),
        ),
      }));
      return {
        rows,
        page,
        size,
        has_more: result.hasMore ?? rows.length === size,
        total: result.total,
      };
    },
    async createRows(ctx, tableId, rows) {
      return write(() =>
        runAtomic(ctx, async (executionContext) => {
          const table = await get(ctx, tableId, executionContext);
          await requireWritableFields(
            tableId,
            rows.flatMap((row) => Object.keys(row.values)),
            executionContext,
          );
          const fieldTypes = await readFieldTypes(tableId, executionContext);
          const byId = new Map(
            table
              .getFields()
              .map((field) => [
                field.id().toString(),
                fieldTypes.get(field.id().toString()) ??
                  TEABLE_TO_DEFAULT[field.type().toString()] ??
                  'string',
              ]),
          );
          for (const row of rows) {
            for (const fieldId of Object.keys(row.values)) {
              if (!byId.has(fieldId))
                throw new TableDataError(
                  `unknown column id: ${fieldId}`,
                  404,
                  'data.column_not_found',
                );
            }
          }
          const command = unwrap(
            CreateRecordsCommand.create({
              tableId,
              records: rows.map((row) => ({
                fields: Object.fromEntries(
                  Object.entries(row.values).map(([fieldId, value]) => [
                    fieldId,
                    encodeValue(byId.get(fieldId) ?? 'string', value),
                  ]),
                ),
              })),
              typecast: false,
              fieldKeyType: 'id',
            }),
          );
          const result = await runCommand<{
            records: Array<{
              id(): { toString(): string };
              fields(): {
                entries(): ReadonlyArray<{
                  fieldId: { toString(): string };
                  value: { toValue(): unknown };
                }>;
              };
            }>;
          }>(ctx, command, executionContext);
          if (result.records.length !== rows.length)
            throw new TableDataError(
              'The whole row batch exceeds the table row limit',
              400,
              'validation.limit.rows_per_table_max',
              {
                max: TABLE_CAPACITY.maxRows,
                requested: rows.length,
                actual: result.records.length,
              },
            );
          return result.records.map((record) => asCreatedRow(record, byId));
        }),
      );
    },
    async updateRows(ctx, tableId, rows) {
      return write(async () => {
        const table = await get(ctx, tableId);
        await requireWritableFields(
          tableId,
          rows.flatMap((row) => Object.keys(row.values)),
        );
        const records = await runQuery<ListTableRecordsResult>(
          ctx,
          unwrap(
            ListTableRecordsQuery.create({
              tableId,
              limit: Math.min(100, rows.length),
              offset: 0,
              includeTotal: false,
              selectedRecordIds: rows.map((row) => row.id),
              fieldKeyType: 'id',
            }),
          ),
        );
        const requested = new Set(rows.map((row) => row.id));
        if (records.records.filter((record) => requested.has(record.id)).length !== requested.size)
          throw new TableDataError(
            'One or more rows do not belong to this table',
            404,
            'data.row_not_found',
          );
        const fieldTypes = await readFieldTypes(tableId);
        const byId = new Map(
          table
            .getFields()
            .map((field) => [
              field.id().toString(),
              fieldTypes.get(field.id().toString()) ??
                TEABLE_TO_DEFAULT[field.type().toString()] ??
                'string',
            ]),
        );
        for (const row of rows) {
          for (const fieldId of Object.keys(row.values)) {
            if (!byId.has(fieldId))
              throw new TableDataError(
                `unknown column id: ${fieldId}`,
                404,
                'data.column_not_found',
              );
          }
        }
        const command = unwrap(
          UpdateRecordsCommand.create({
            tableId,
            records: rows.map((row) => ({
              id: row.id,
              fields: Object.fromEntries(
                Object.entries(row.values).map(([fieldId, value]) => [
                  fieldId,
                  encodeValue(byId.get(fieldId) ?? 'string', value),
                ]),
              ),
            })),
            typecast: false,
            fieldKeyType: 'id',
          }),
        );
        await runCommand<UpdateRecordsResult>(ctx, command);
        const fresh = await this.queryRows(ctx, tableId, 1, 1000);
        return rows.map((row) => ({
          id: row.id,
          values: fresh.rows.find((value) => value.id === row.id)?.values ?? {},
        }));
      });
    },
    async importFile(ctx, input) {
      return write(async () => {
        if (input.extension === 'csv') {
          const command = unwrap(
            ImportCsvCommand.createFromBuffer({
              baseId: ctx.baseId,
              csvData: input.data,
              tableName: input.name,
              maxRowCount: TABLE_CAPACITY.maxRows,
              useFirstRowAsHeader: true,
            }),
          );
          const result = await runCommand<ImportCsvResult>(ctx, command);
          return tableDto(result.table);
        }
        const command = unwrap(
          ImportExcelCommand.createFromBuffer({
            baseId: ctx.baseId,
            excelData: input.data,
            tableName: input.name,
            maxRowCount: TABLE_CAPACITY.maxRows,
            useFirstRowAsHeader: true,
            fileType: 'xlsx',
            sheetName: input.sheetName,
          }),
        );
        const result = await runCommand<ImportExcelResult>(ctx, command);
        return tableDto(result.table);
      });
    },
    async renameColumn(ctx, tableId, columnId, name) {
      return write(async () => {
        const table = await get(ctx, tableId);
        if (!table.getFields().some((field) => field.id().toString() === columnId))
          throw new TableDataError(`unknown column id: ${columnId}`, 404, 'data.column_not_found');
        await requireWritableFields(tableId, [columnId]);
        const command = unwrap(
          UpdateFieldCommand.create({
            baseId: ctx.baseId,
            tableId,
            fieldId: columnId,
            field: { name },
          }),
        );
        const result = await runCommand<UpdateFieldResult>(ctx, command);
        return tableDto(result.table);
      });
    },
    async deleteRows(ctx, tableId, rowIds) {
      return write(async () => {
        await get(ctx, tableId);
        const records = await runQuery<ListTableRecordsResult>(
          ctx,
          unwrap(
            ListTableRecordsQuery.create({
              tableId,
              limit: Math.min(100, rowIds.length),
              offset: 0,
              includeTotal: false,
              selectedRecordIds: rowIds,
              fieldKeyType: 'id',
            }),
          ),
        );
        const requested = new Set(rowIds);
        const matching = records.records.filter((record) => requested.has(record.id));
        if (matching.length !== requested.size)
          throw new TableDataError(
            'One or more rows do not belong to this table',
            404,
            'data.row_not_found',
          );
        const command = unwrap(DeleteRecordsCommand.create({ tableId, recordIds: rowIds }));
        await runCommand<DeleteRecordsResult>(ctx, command);
        return rowIds;
      });
    },
    async close() {
      await database.close();
    },
  } satisfies TableData;
};
