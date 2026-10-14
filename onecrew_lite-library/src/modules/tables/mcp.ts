import { authenticatedResult, type AccountTools } from '../access/mcp.js';
import { McpServer } from '@modelcontextprotocol/server';
import type { TableService } from './service.js';
import {
  createRowsSchema,
  rowQuerySchema,
  tableAddColumnsSchema,
  tableCreateSchema,
  tableGetSchema,
  tableListSchema,
  updateRowsSchema,
} from '@onecrew/contracts/tables';

/** Register the seven table operations directly on the official SDK server. */
export const registerTableTools = (
  server: McpServer,
  service: TableService,
  account: AccountTools,
) => {
  server.registerTool(
    'table_create',
    {
      title: 'Create dynamic table',
      description: [
        'Create tables in the current signed-in account’s private space.',
        'Create an empty persisted table, returning real column IDs and web_url; insert records separately with table_create_rows. Columns use {name, data_type} from the schema.',
        'For creator/KOL search results, default to exactly three columns: creator (people; company for an explicitly brand-only list), channel link (website), and match evaluation (score). The entity cell combines name, image_url and identity {platform, platform_id}; put the channel URL in the website cell. Score combines level and evidence-based reason, rendered as a badge with hover explanation.',
        'Unless the user explicitly requests otherwise, do not split name/avatar or score/reason into separate columns, and do not add biography, source, retrieval date, notes or platform ID columns. General-purpose tables use the business columns needed for the task; do not add a score automatically.',
        'Use only known image URLs and native platform IDs. Evaluate search matches from available evidence, not assumed performance; use null when evidence is insufficient. After an uncertain response, inspect table_list before retrying creation.',
      ].join(' '),
      inputSchema: tableCreateSchema,
    },
    async ({ name, columns }) =>
      authenticatedResult(account, (ctx) => service.createTable(ctx, { name, columns })),
  );
  server.registerTool(
    'table_list',
    {
      title: 'List dynamic tables',
      description:
        'List tables in the current account’s private space with 1-based pagination. Size is capped at 100.',
      inputSchema: tableListSchema,
    },
    async ({ page, size, q }) =>
      authenticatedResult(account, (ctx) => service.listTables(ctx, { page, size, q })),
  );
  server.registerTool(
    'table_get',
    {
      title: 'Get dynamic table',
      description:
        'Get a table schema in the current account’s private space, including real column IDs, data_type and web_url. Row values must be keyed by these IDs, not names. A table_create result already supplies the IDs; reuse it when available. Read records separately with table_query_rows.',
      inputSchema: tableGetSchema,
    },
    async ({ table_id }) => authenticatedResult(account, (ctx) => service.getTable(ctx, table_id)),
  );
  server.registerTool(
    'table_add_columns',
    {
      title: 'Add table columns',
      description:
        'Add 1 to 100 columns to a table in the current account’s private space with {name, data_type}. Names must be unique; all columns are validated before writing. Add only user-requested or task-required business columns; do not invent auxiliary source/date/notes columns or split rich entity/score cells. Do not restructure an existing table without a user request.',
      inputSchema: tableAddColumnsSchema,
    },
    async ({ table_id, columns }) =>
      authenticatedResult(account, (ctx) => service.addColumns(ctx, table_id, { columns })),
  );
  server.registerTool(
    'table_query_rows',
    {
      title: 'Query table rows',
      description:
        'Query rows from a table in the current account’s private space, keyed by real column IDs. Pages start at 1, size is at most 100; continue while has_more is true. Supply a real sort_field ID with sort_order; people, company and score do not support sorting. Use this tool to inspect records after an uncertain write before retrying.',
      inputSchema: rowQuerySchema,
    },
    async ({ table_id, page, size, sort_field, sort_order }) =>
      authenticatedResult(account, (ctx) =>
        service.queryRows(ctx, table_id, { page, size, sort_field, sort_order }),
      ),
  );
  server.registerTool(
    'table_create_rows',
    {
      title: 'Create table rows',
      description: [
        'Insert 1 to 100 rows into a table in the current account’s private space. Use the real column IDs and data_type returned by table_create or table_get as values keys.',
        'An entity value is {name, image_url?, url?, identity?:{platform, platform_id}}; preserve known native IDs from search results, never infer IDs from names or handles. Omit unknown images and identity; never fabricate them.',
        'A score value is {level: excellent|good|medium|low, reason?:string}. For search matches, give a brief reason based on search summaries and user criteria; use null when evidence is insufficient. Put explanations in reason, not another column. subscriber_text is display text, not an exact integer.',
        'Returns {table_id, web_url, rows} under structuredContent.result. Report the actual written count and exact detail link. Results are not saved until this operation succeeds; inspect existing rows before retrying an uncertain write.',
      ].join(' '),
      inputSchema: createRowsSchema,
    },
    async ({ table_id, rows }) =>
      authenticatedResult(account, (ctx) => service.createRows(ctx, table_id, { rows })),
  );
  server.registerTool(
    'table_update_rows',
    {
      title: 'Update table rows',
      description:
        'Partially update 1 to 100 rows in a table in the current account’s private space by real record ID and column IDs. Unspecified cells remain unchanged; null clears a cell. A supplied people/company or score object replaces the entire cell: first read and preserve its existing image_url, url, identity or reason unless intentionally changing them. Values are checked against column data_type. Returns {table_id, web_url, rows}; report confirmed updates and the detail link.',
      inputSchema: updateRowsSchema,
    },
    async ({ table_id, rows }) =>
      authenticatedResult(account, (ctx) => service.updateRows(ctx, table_id, { rows })),
  );
};
