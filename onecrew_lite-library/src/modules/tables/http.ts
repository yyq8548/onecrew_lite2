import { Hono } from 'hono';
import { stream } from 'hono/streaming';
import { onDiscoveryChanged } from './discovery-events.js';
import { recommendationCaptureMetadataSchema } from './recommendation-capture.js';
import type { Context } from 'hono';
import type { ClientErrorStatusCode, ServerErrorStatusCode } from 'hono/utils/http-status';
import { zValidator } from '@hono/zod-validator';
import { z } from 'zod';
import type { TableService } from './service.js';
import type { TenantHttpEnv } from '../tenancy/service.js';
import { toErrorBody } from './service.js';
import { TableDataError } from './data.js';
import {
  createRowsSchema,
  tableAddColumnsSchema,
  tableCreateSchema,
  tableListFields,
  updateRowsSchema,
  tableParamSchema,
  rowQueryFields,
  validateRowSort,
  renameColumnSchema,
  columnParamSchema,
  rowParamSchema,
  exportQuerySchema,
  importTableSchema,
} from '@onecrew/contracts/tables';

type ErrorStatusCode = ClientErrorStatusCode | ServerErrorStatusCode;

const sendError = (c: Context, error: unknown) => {
  const body = toErrorBody(error);
  const status =
    error && typeof error === 'object' && 'status' in error && typeof error.status === 'number'
      ? error.status
      : body.error.code.startsWith('validation.')
        ? 400
        : 500;
  return c.json(body, status as ErrorStatusCode);
};

const validationHook = (result: { success: boolean; error?: unknown }, c: Context) => {
  if (!result.success)
    return c.json(
      {
        error: {
          code: 'validation.input',
          message: 'Request input is invalid',
          details: result.error,
        },
      },
      400,
    );
};

const queryNumber = (schema: typeof import('@onecrew/contracts/tables').pageSchema) =>
  z
    .string()
    .optional()
    .transform((value) => (value === undefined ? undefined : Number(value)))
    .pipe(schema);
const tableListQuerySchema = z.object({
  ...tableListFields,
  page: queryNumber(tableListFields.page),
  size: queryNumber(tableListFields.size),
});
const rowQueryHttpSchema = z
  .object({
    ...rowQueryFields,
    page: queryNumber(rowQueryFields.page),
    size: queryNumber(rowQueryFields.size),
  })
  .superRefine(validateRowSort);
const importFormSchema = z.object({
  file: z.instanceof(File),
  name: importTableSchema.shape.name,
  sheet_name: importTableSchema.shape.sheet_name,
});

export const createTableRoutes = (service: TableService) => {
  return new Hono<TenantHttpEnv>()
    .post(
      '/import',
      async (c, next) => {
        if (!c.req.header('content-type')?.toLocaleLowerCase().startsWith('multipart/form-data'))
          return sendError(
            c,
            new TableDataError(
              'Import request must use multipart/form-data',
              400,
              'validation.content_type',
            ),
          );
        try {
          await c.req.parseBody();
        } catch {
          return sendError(
            c,
            new TableDataError(
              'Import request contains invalid multipart data',
              400,
              'validation.multipart',
            ),
          );
        }
        await next();
      },
      zValidator('form', importFormSchema, validationHook),
      async (c) => {
        try {
          const { file, name, sheet_name } = c.req.valid('form');
          return c.json(
            await service.importFile(c.get('tenant'), {
              fileName: file.name,
              data: new Uint8Array(await file.arrayBuffer()),
              ...(name !== undefined ? { name } : {}),
              ...(sheet_name !== undefined ? { sheet_name } : {}),
            }),
            201,
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .post('/', zValidator('json', tableCreateSchema, validationHook), async (c) => {
      try {
        return c.json(await service.createTable(c.get('tenant'), c.req.valid('json')), 201);
      } catch (error) {
        return sendError(c, error);
      }
    })
    .get('/', zValidator('query', tableListQuerySchema, validationHook), async (c) => {
      try {
        return c.json(await service.listTables(c.get('tenant'), c.req.valid('query')));
      } catch (error) {
        return sendError(c, error);
      }
    })
    .get('/:tableId', zValidator('param', tableParamSchema, validationHook), async (c) => {
      try {
        return c.json(await service.getTable(c.get('tenant'), c.req.valid('param').tableId));
      } catch (error) {
        return sendError(c, error);
      }
    })
    .get(
      '/:tableId/creator-updates',
      zValidator('param', tableParamSchema, validationHook),
      async (c) => {
        const tenant = c.get('tenant');
        const { tableId } = c.req.valid('param');
        try {
          await service.getTable(tenant, tableId);
        } catch (error) {
          return sendError(c, error);
        }
        c.header('Cache-Control', 'no-store');
        c.header('Content-Type', 'application/x-ndjson');
        return stream(c, async (output) => {
          let dirty = false;
          let writing = false;
          const flush = async () => {
            if (writing || output.aborted || output.closed) return;
            writing = true;
            try {
              while (dirty && !output.aborted && !output.closed) {
                dirty = false;
                await output.write('{"changed":true}\n');
              }
            } finally {
              writing = false;
            }
          };
          await new Promise<void>((resolve) => {
            const unsubscribe = onDiscoveryChanged(tenant.tenantId, tableId, () => {
              dirty = true;
              void flush().catch(() => {
                unsubscribe();
                resolve();
              });
            });
            output.onAbort(() => {
              unsubscribe();
              resolve();
            });
            // Refresh once after subscription closes the initial-read race.
            dirty = true;
            void flush().catch(() => {
              unsubscribe();
              resolve();
            });
          });
        });
      },
    )
    .get('/:tableId/creators', zValidator('param', tableParamSchema, validationHook), async (c) => {
      c.header('Cache-Control', 'no-store');
      try {
        return c.json(
          await service.getDiscoveryCreatorView(c.get('tenant'), c.req.valid('param').tableId),
        );
      } catch (error) {
        return sendError(c, error);
      }
    })
    .post('/:tableId/recommendation-capture',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('json', recommendationCaptureMetadataSchema, validationHook), async (c) => {
        c.header('Cache-Control', 'no-store');
        try {
          return c.json(await service.captureRecommendations(c.get('tenant'),
            c.req.valid('param').tableId, c.req.valid('json')));
        } catch (error) { return sendError(c, error); }
      })
    .post(
      '/:tableId/columns',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('json', tableAddColumnsSchema.omit({ table_id: true }), validationHook),
      async (c) => {
        try {
          return c.json(
            await service.addColumns(
              c.get('tenant'),
              c.req.valid('param').tableId,
              c.req.valid('json'),
            ),
            201,
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .patch(
      '/:tableId/columns/:columnId',
      zValidator('param', columnParamSchema, validationHook),
      zValidator('json', renameColumnSchema, validationHook),
      async (c) => {
        try {
          const params = c.req.valid('param');
          return c.json(
            await service.renameColumn(
              c.get('tenant'),
              params.tableId,
              params.columnId,
              c.req.valid('json'),
            ),
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .delete(
      '/:tableId/rows/:rowId',
      zValidator('param', rowParamSchema, validationHook),
      async (c) => {
        try {
          const params = c.req.valid('param');
          return c.json(
            await service.deleteRows(c.get('tenant'), params.tableId, { row_ids: [params.rowId] }),
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .get(
      '/:tableId/export',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('query', exportQuerySchema, validationHook),
      async (c) => {
        try {
          const result = await service.exportTable(
            c.get('tenant'),
            c.req.valid('param').tableId,
            c.req.valid('query').locale ?? 'zh-CN',
          );
          const disposition = `attachment; filename="export.xlsx"; filename*=UTF-8''${encodeURIComponent(result.filename)}`;
          return c.body(new Uint8Array(result.bytes), 200, {
            'content-type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
            'content-disposition': disposition,
          });
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .get(
      '/:tableId/rows',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('query', rowQueryHttpSchema, validationHook),
      async (c) => {
        try {
          return c.json(
            await service.queryRows(
              c.get('tenant'),
              c.req.valid('param').tableId,
              c.req.valid('query'),
            ),
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .post(
      '/:tableId/rows',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('json', createRowsSchema.omit({ table_id: true }), validationHook),
      async (c) => {
        try {
          return c.json(
            await service.createRows(
              c.get('tenant'),
              c.req.valid('param').tableId,
              c.req.valid('json'),
            ),
            201,
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    )
    .patch(
      '/:tableId/rows',
      zValidator('param', tableParamSchema, validationHook),
      zValidator('json', updateRowsSchema.omit({ table_id: true }), validationHook),
      async (c) => {
        try {
          return c.json(
            await service.updateRows(
              c.get('tenant'),
              c.req.valid('param').tableId,
              c.req.valid('json'),
            ),
          );
        } catch (error) {
          return sendError(c, error);
        }
      },
    );
};
