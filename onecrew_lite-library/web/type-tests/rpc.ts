import { api, request } from '../src/lib/api';
import type { InferResponseType } from 'hono/client';

const list = api['dynamic-tables'].$get({ query: { page: '1', size: '100', q: 'people' } });
type ListedTable = InferResponseType<(typeof api)['dynamic-tables'][':tableId']['$get'], 200>;
const imported = api['dynamic-tables'].import.$post({ form: { file: new File([], 'table.csv') } });

void request(list);
void request(imported);
const success: Promise<ListedTable> = request(
  api['dynamic-tables'][':tableId'].$get({ param: { tableId: 'table-1' } }),
);
void success;

// @ts-expect-error The API has no access path at this level.
api.access.$get();
// @ts-expect-error Table detail requires its path parameter.
api['dynamic-tables'][':tableId'].$get();
// @ts-expect-error A table name must be a string.
api['dynamic-tables'].$post({ json: { name: 4, columns: [{ name: 'A', data_type: 'string' }] } });
api['dynamic-tables'][':tableId'].rows.$get({
  param: { tableId: 'table-1' },
  query: {
    page: '1',
    size: '50',
    sort_field: 'name',
    // @ts-expect-error This enum only allows asc and desc.
    sort_order: 'ascending',
  },
});
