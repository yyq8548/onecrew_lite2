import { z } from 'zod';
import type { TableData, TableDto as DataTableDto, RowDto } from './data.js';
import type { TenantContext } from '../tenancy/service.js';
import { TableDataError } from './data.js';
import { recommendationRows } from './recommendation-result.js';
import { recommendationCapture, type RecommendationCaptureMetadata } from './recommendation-capture.js';
import { ExcelImportAdapter } from '@teable/v2-import';
import { PapaparseCsvParser } from '@teable/v2-adapter-csv-parser-papaparse';
import {
  buildCreatorWorkbook,
  buildTableWorkbook,
  cleanFileNamePart,
  exportFilename,
} from './files.js';
import {
  TABLE_CAPACITY,
  createRowsSchema,
  rowQuerySchema,
  tableAddColumnsSchema,
  tableCreateSchema,
  tableListSchema,
  tableIdSchema,
  importTableSchema,
  renameColumnSchema,
  deleteRowsSchema,
  exportLocaleSchema,
  updateRowsSchema,
  valueSchemas,
  type ColumnInput,
  type CreateRowsInput,
  type RowQueryInput,
  type TableCreateInput,
  type TableListInput,
  type UpdateRowsInput,
} from '@onecrew/contracts/tables';

export type ServiceTableDto = DataTableDto & { web_url: string };
export type RowWriteResult = { table_id: string; web_url: string; rows: RowDto[] };
export type TableExport = { bytes: Uint8Array; filename: string };
export type TableService = ReturnType<typeof createTableService>;

/** Validate and canonicalize the public URL used for table detail links. */
export const normalizePublicWebUrl = (value: string): string => {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error('PUBLIC_WEB_URL must be a valid http(s) URL');
  }
  if (
    !['http:', 'https:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    value.includes('?') ||
    value.includes('#')
  ) {
    throw new Error(
      'PUBLIC_WEB_URL must use http or https and cannot contain credentials, query parameters, or a fragment',
    );
  }
  return url.href.replace(/\/+$/, '');
};

const duplicateColumnCheck = (columns: ColumnInput[], existing: Iterable<string> = []) => {
  const names = new Set([...existing].map((name) => name.toLocaleLowerCase()));
  for (const column of columns) {
    const key = column.name.toLocaleLowerCase();
    if (names.has(key))
      throw new TableDataError(
        `duplicate column name: ${column.name}`,
        409,
        'validation.duplicate_column',
      );
    names.add(key);
  }
};

const validateValues = (table: DataTableDto, values: Record<string, unknown>) => {
  for (const [columnId, value] of Object.entries(values)) {
    const column = table.columns.find((candidate) => candidate.id === columnId);
    if (!column)
      throw new TableDataError(`unknown column id: ${columnId}`, 400, 'validation.unknown_column');
    if (value === null) continue;
    const parsed = valueSchemas[column.data_type].safeParse(value);
    if (!parsed.success)
      throw new TableDataError(
        `invalid ${column.data_type} value for column ${column.name}`,
        400,
        'validation.cell_value',
        parsed.error.flatten(),
      );
  }
};

export const createTableService = (data: TableData, publicWebUrl: string) => {
  const tableUrl = (tableId: string) => `${publicWebUrl}/library/${encodeURIComponent(tableId)}`;
  const withWebUrl = (table: DataTableDto): ServiceTableDto => ({
    ...table,
    web_url: tableUrl(table.id),
  });
  return {
    async createTable(ctx: TenantContext, input: TableCreateInput): Promise<ServiceTableDto> {
      const parsed = tableCreateSchema.parse(input);
      duplicateColumnCheck(parsed.columns);
      return withWebUrl(await data.createTable(ctx, parsed.name, parsed.columns));
    },
    async listTables(
      ctx: TenantContext,
      input: TableListInput = { page: 1, size: 20 },
    ): Promise<{ tables: ServiceTableDto[]; page: number; size: number; has_more: boolean }> {
      const parsed = tableListSchema.parse(input);
      const result = await data.listTables(ctx, parsed.page, parsed.size, parsed.q);
      return { ...result, tables: result.tables.map(withWebUrl) };
    },
    async getTable(ctx: TenantContext, tableId: string): Promise<ServiceTableDto> {
      if (!tableIdSchema.safeParse(tableId).success)
        throw new TableDataError('table_id is required', 400, 'validation.table_id');
      return withWebUrl(await data.getTable(ctx, tableId));
    },
    async getDiscoveryCreatorView(ctx: TenantContext, tableId: string) {
      const id = tableIdSchema.parse(tableId);
      const material = await data.getDiscoveryCreatorView(ctx, id);
      const table = await data.getTable(ctx, id);
      return { ...material, recommendations: material.state === 'current'
        ? recommendationRows(table, material) : [] };
    },
    async captureRecommendations(ctx: TenantContext, tableId: string, input: RecommendationCaptureMetadata) {
      const id = tableIdSchema.parse(tableId);
      const material = await data.getDiscoveryCreatorView(ctx, id);
      const table = await data.getTable(ctx, id);
      const capture = recommendationCapture(table, material, input);
      return { archive: capture.bytes.toString('utf8'), content_hash: capture.content_hash,
        record_count: capture.record_count, schema_count: capture.schema_count, table_name: table.name,
        table_id: table.id, criteria_revision: material.projection!.criteria_revision };
    },
    async addColumns(
      ctx: TenantContext,
      tableId: string,
      input: { columns: ColumnInput[] },
    ): Promise<ServiceTableDto> {
      const parsed = tableAddColumnsSchema.omit({ table_id: true }).parse(input);
      const table = await this.getTable(ctx, tableId);
      duplicateColumnCheck(
        parsed.columns,
        table.columns.map((column) => column.name),
      );
      return withWebUrl(await data.addColumns(ctx, tableId, parsed.columns));
    },
    async queryRows(
      ctx: TenantContext,
      tableId: string,
      input: Omit<RowQueryInput, 'table_id'>,
    ): Promise<{ rows: RowDto[]; page: number; size: number; has_more: boolean; total: number }> {
      const parsed = rowQuerySchema.parse({ table_id: tableId, ...input });
      if (parsed.sort_field) {
        const table = await this.getTable(ctx, parsed.table_id);
        const column = table.columns.find((candidate) => candidate.id === parsed.sort_field);
        if (!column)
          throw new TableDataError(
            `unknown column id: ${parsed.sort_field}`,
            400,
            'validation.sort_field',
          );
        if (
          column.data_type === 'people' ||
          column.data_type === 'company' ||
          column.data_type === 'score'
        ) {
          throw new TableDataError(
            `${column.data_type} columns cannot be sorted as structured JSON`,
            400,
            'validation.sort_field',
          );
        }
      }
      return data.queryRows(
        ctx,
        parsed.table_id,
        parsed.page,
        parsed.size,
        parsed.sort_field ? { fieldId: parsed.sort_field, order: parsed.sort_order! } : undefined,
      );
    },
    async createRows(
      ctx: TenantContext,
      tableId: string,
      input: { rows: CreateRowsInput['rows'] },
    ): Promise<RowWriteResult> {
      const parsed = createRowsSchema.parse({ table_id: tableId, ...input });
      const table = await this.getTable(ctx, parsed.table_id);
      parsed.rows.forEach((row) => validateValues(table, row.values));
      return {
        table_id: parsed.table_id,
        web_url: tableUrl(parsed.table_id),
        rows: await data.createRows(ctx, parsed.table_id, parsed.rows),
      };
    },
    async updateRows(
      ctx: TenantContext,
      tableId: string,
      input: { rows: UpdateRowsInput['rows'] },
    ): Promise<RowWriteResult> {
      const parsed = updateRowsSchema.parse({ table_id: tableId, ...input });
      const table = await this.getTable(ctx, parsed.table_id);
      parsed.rows.forEach((row) => validateValues(table, row.values));
      return {
        table_id: parsed.table_id,
        web_url: tableUrl(parsed.table_id),
        rows: await data.updateRows(ctx, parsed.table_id, parsed.rows),
      };
    },
    async importFile(
      ctx: TenantContext,
      input: {
        fileName: string;
        name?: string;
        sheet_name?: string;
        data: Uint8Array;
      },
    ) {
      const parsed = importTableSchema.parse(input);
      if (parsed.data.byteLength > 10 * 1024 * 1024)
        throw new TableDataError(
          'Import file exceeds the 10 MiB limit',
          413,
          'validation.file_size',
        );
      const extension = parsed.fileName.toLocaleLowerCase().split('.').at(-1);
      if (extension !== 'csv' && extension !== 'xlsx')
        throw new TableDataError(
          'Only .csv and .xlsx files are supported',
          400,
          'validation.file_type',
        );
      if (
        extension === 'xlsx' &&
        (parsed.data.byteLength < 4 ||
          parsed.data[0] !== 0x50 ||
          parsed.data[1] !== 0x4b ||
          parsed.data[2] !== 0x03 ||
          parsed.data[3] !== 0x04)
      )
        throw new TableDataError('Invalid XLSX file', 400, 'validation.import');
      const sourceBaseName = parsed.fileName.replace(/\\/g, '/').split('/').pop() ?? '';
      const inferredName = cleanFileNamePart(sourceBaseName.replace(/\.(csv|xlsx)$/i, '')).trim();
      let headers: string[];
      let importedRowCount: number;
      if (extension === 'csv') {
        const result = new PapaparseCsvParser().parse(
          { type: 'buffer', data: parsed.data },
          { hasHeader: true },
        );
        if (result.isErr())
          throw new TableDataError(
            result.error.message,
            400,
            result.error.code ?? 'validation.import',
            result.error.details,
          );
        headers = [...result.value.headers];
        importedRowCount = result.value.rowCount ?? 0;
      } else {
        const result = await new ExcelImportAdapter().analyze(
          { type: extension, data: parsed.data, fileName: parsed.fileName },
          { sheetName: parsed.sheet_name, skipFirstNLines: 1 },
          10_001,
        );
        if (result.isErr())
          throw new TableDataError(
            result.error.message,
            400,
            result.error.code ?? 'validation.import',
            result.error.details,
          );
        headers = [...result.value.headers];
        importedRowCount = result.value.sampleRows.length;
      }
      const hasHeader = headers.some((header) => header.trim().length > 0);
      if (!hasHeader || headers.length > TABLE_CAPACITY.maxColumns)
        throw new TableDataError(
          'Import must contain between 1 and 100 columns',
          400,
          'validation.column_limit',
        );
      if (importedRowCount > TABLE_CAPACITY.maxRows)
        throw new TableDataError(
          'Import exceeds the 10,000 row limit',
          400,
          'validation.row_limit',
        );
      const table = await data.importFile(ctx, {
        name: parsed.name ?? (inferredName ? inferredName.slice(0, 255) : 'Imported table'),
        extension,
        data: parsed.data,
        sheetName: parsed.sheet_name,
      });
      return withWebUrl(table);
    },
    async renameColumn(
      ctx: TenantContext,
      tableId: string,
      columnId: string,
      input: { name: string },
    ): Promise<ServiceTableDto> {
      const parsed = renameColumnSchema.parse(input);
      const table = await this.getTable(ctx, tableId);
      const current = table.columns.find((column) => column.id === columnId);
      if (!current)
        throw new TableDataError(`unknown column id: ${columnId}`, 404, 'data.column_not_found');
      duplicateColumnCheck(
        [{ name: parsed.name, data_type: current.data_type }],
        table.columns.filter((column) => column.id !== columnId).map((column) => column.name),
      );
      return withWebUrl(await data.renameColumn(ctx, tableId, columnId, parsed.name));
    },
    async deleteRows(
      ctx: TenantContext,
      tableId: string,
      input: { row_ids: string[] },
    ): Promise<{ deleted_ids: string[] }> {
      const parsed = deleteRowsSchema.parse(input);
      const unique = [...new Set(parsed.row_ids)];
      if (unique.length !== parsed.row_ids.length)
        throw new TableDataError('row_ids must be unique', 400, 'validation.row_ids');
      await this.getTable(ctx, tableId);
      return { deleted_ids: await data.deleteRows(ctx, tableId, parsed.row_ids) };
    },
    async exportTable(
      ctx: TenantContext,
      tableId: string,
      localeInput: unknown = 'zh-CN',
    ): Promise<TableExport> {
      const locale = exportLocaleSchema.parse(localeInput);
      const table = await this.getTable(ctx, tableId);
      if (
        table.columns.some((column) => column.name === 'Campaign ID') &&
        table.columns.some((column) => column.name === 'Search hit ID')
      ) {
        const material = await data.getDiscoveryCreatorView(ctx, tableId);
        if (material.state !== 'current')
          throw new TableDataError(
            'Creator projection is not current; synchronize before exporting',
            409,
            'data.export_projection_not_current',
          );
        return {
          bytes: buildCreatorWorkbook(table, material, locale),
          filename: exportFilename(table.name),
        };
      }
      const maxDataRows = 1_048_575;
      const exportLimitError = () =>
        new TableDataError(
          'Export exceeds the Excel worksheet capacity of 1,048,575 data rows (one header row)',
          413,
          'validation.export_row_limit',
        );
      const exportPageSize = 100;
      const first = await data.queryRows(ctx, tableId, 1, exportPageSize);
      if (first.total > maxDataRows) throw exportLimitError();
      const rows = [...first.rows];
      if (rows.length > maxDataRows) throw exportLimitError();
      let page = 2;
      let hasMore = first.has_more;
      while (hasMore) {
        const result = await data.queryRows(ctx, tableId, page, exportPageSize);
        rows.push(...result.rows);
        if (rows.length > maxDataRows) throw exportLimitError();
        if (result.rows.length === 0)
          throw new TableDataError(
            'Rows changed while the export was being read; retry the export',
            409,
            'data.export_changed',
          );
        hasMore = result.has_more;
        page += 1;
      }
      if (rows.length < first.total)
        throw new TableDataError(
          'Rows changed while the export was being read; retry the export',
          409,
          'data.export_changed',
        );
      return {
        bytes: buildTableWorkbook(table, rows, locale),
        filename: exportFilename(table.name),
      };
    },
  };
};

export const toErrorBody = (error: unknown) => {
  if (error instanceof TableDataError)
    return {
      error: {
        code: error.code,
        message: error.message,
        ...(error.details ? { details: error.details } : {}),
      },
    };
  if (error instanceof z.ZodError)
    return {
      error: { code: 'validation.input', message: error.message, details: error.flatten() },
    };
  if (error instanceof SyntaxError)
    return { error: { code: 'validation.json', message: 'Request body must be valid JSON' } };
  return {
    error: {
      code: 'internal.error',
      message: error instanceof Error ? error.message : 'Unexpected error',
    },
  };
};
