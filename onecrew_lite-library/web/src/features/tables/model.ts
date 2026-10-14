import { cellValueSchema, dataTypeSchema } from '@onecrew/contracts/tables';
import type { CellValue, DataType } from '@onecrew/contracts/tables';
import type { InferResponseType } from 'hono/client';
import { api } from '@/lib/api';

export type Table = InferResponseType<(typeof api)['dynamic-tables'][':tableId']['$get'], 200>;
export type Column = Table['columns'][number];
export type Row = InferResponseType<
  (typeof api)['dynamic-tables'][':tableId']['rows']['$get'],
  200
>['rows'][number];
export const DATA_TYPES = dataTypeSchema.options;
export const isStructured = (type: DataType) =>
  type === 'people' || type === 'company' || type === 'score';
export const canSort = (type: DataType) => !isStructured(type);
export const isHttpUrl = (value: unknown): value is string =>
  typeof value === 'string' &&
  /^https?:\/\//i.test(value) &&
  (() => {
    try {
      return Boolean(new URL(value));
    } catch {
      return false;
    }
  })();
export const cellValue = (value: unknown): CellValue => {
  const parsed = cellValueSchema.safeParse(value);
  return parsed.success ? parsed.data : null;
};
export const initials = (value: string) => value.trim().slice(0, 1).toUpperCase() || '•';
