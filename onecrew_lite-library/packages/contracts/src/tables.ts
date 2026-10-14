import { z } from 'zod';

export const TABLE_CAPACITY = { maxRows: 10_000, maxColumns: 100 } as const;

export const dataTypeSchema = z
  .enum([
    'string',
    'int',
    'float',
    'bool',
    'date',
    'people',
    'company',
    'website',
    'email',
    'telephone',
    'score',
  ])
  .describe(
    'Fixed semantic type. Use exactly one of string, int, float, bool, date, people, company, website, email, telephone, or score.',
  );
export type DataType = z.infer<typeof dataTypeSchema>;

const httpUrlSchema = z
  .string()
  .url()
  .refine((value) => /^https?:\/\//i.test(value), 'URL must use http or https');
const identitySchema = z
  .object({
    platform: z.string().trim().min(1).describe('Platform name'),
    platform_id: z
      .string()
      .trim()
      .min(1)
      .describe(
        'Known native platform ID from source results; never infer from name, handle or URL',
      ),
  })
  .strict()
  .describe('{platform:string, platform_id:string}');
export const peopleValueSchema = z
  .object({
    name: z.string().trim().min(1).describe('Display name'),
    image_url: httpUrlSchema
      .optional()
      .describe('Known image URL, not a profile page; omit when unavailable'),
    url: httpUrlSchema.optional().describe('Profile or entity URL displayed with its name'),
    identity: identitySchema
      .optional()
      .describe('Preserve known platform and native ID inside this cell; hidden in the UI'),
  })
  .strict()
  .describe(
    '{name:string, image_url?:http(s) URL, url?:http(s) URL, identity?:{platform:string, platform_id:string}}',
  );
export const companyValueSchema = peopleValueSchema.describe(
  '{name:string, image_url?:http(s) URL, url?:http(s) URL, identity?:{platform:string, platform_id:string}}',
);
export const scoreValueSchema = z
  .object({
    level: z.enum(['excellent', 'good', 'medium', 'low']),
    reason: z
      .string()
      .optional()
      .describe(
        'Evidence-based explanation shown on hover; for search matches use the returned summaries and user criteria, not invented performance',
      ),
  })
  .strict()
  .describe("{level:'excellent'|'good'|'medium'|'low', reason?:string}");

const dateValueSchema = z
  .union([z.iso.date(), z.iso.datetime({ offset: true })])
  .describe('Valid ISO date YYYY-MM-DD or ISO timestamp with timezone offset');
const telephoneValueSchema = z.string().trim().min(3).max(64);

export const cellValueSchema = z
  .union([
    z.string(),
    z.number().finite(),
    z.boolean(),
    peopleValueSchema,
    companyValueSchema,
    scoreValueSchema,
    z.null(),
  ])
  .describe('A scalar, fixed people/company object, fixed score object, or null');
export type PeopleValue = z.infer<typeof peopleValueSchema>;
export type ScoreValue = z.infer<typeof scoreValueSchema>;
export type CellValue = z.infer<typeof cellValueSchema>;

export const columnSchema = z
  .object({
    name: z.string().trim().min(1).max(255),
    data_type: dataTypeSchema.describe(
      'Semantic type controlling validation, storage encoding, rendering, and sorting',
    ),
  })
  .strict()
  .describe('{name:string, data_type:fixed semantic type}');
export type ColumnInput = z.infer<typeof columnSchema>;

export const tableCreateSchema = z
  .object({
    name: z.string().trim().min(1).max(255),
    columns: z.array(columnSchema).min(1).max(TABLE_CAPACITY.maxColumns),
  })
  .strict();
export const pageSchema = z.number().int().positive().default(1);
export const sizeSchema = z.number().int().positive().max(100).default(20);
export const tableListFields = {
  page: pageSchema,
  size: sizeSchema,
  q: z.string().trim().max(255).optional(),
};
export const tableListSchema = z.object(tableListFields).strict();
export const tableIdSchema = z.string().min(1);
export const tableParamSchema = z.object({ tableId: tableIdSchema }).strict();
export const columnParamSchema = z
  .object({ tableId: tableIdSchema, columnId: z.string().min(1) })
  .strict();
export const rowParamSchema = z
  .object({ tableId: tableIdSchema, rowId: z.string().min(1) })
  .strict();
export const tableGetSchema = z.object({ table_id: tableIdSchema }).strict();
export const tableAddColumnsSchema = z
  .object({
    table_id: tableIdSchema,
    columns: z.array(columnSchema).min(1).max(TABLE_CAPACITY.maxColumns),
  })
  .strict();
export const importTableSchema = z
  .object({
    fileName: z.string().trim().min(1),
    name: z.string().trim().min(1).max(255).optional(),
    sheet_name: z.string().trim().min(1).optional(),
    data: z.instanceof(Uint8Array).refine((value) => value.byteLength > 0),
  })
  .strict();
export const renameColumnSchema = z
  .object({
    name: z.string().trim().min(1).max(255),
  })
  .strict();
export const deleteRowsSchema = z
  .object({
    row_ids: z.array(z.string().min(1)).min(1).max(100),
  })
  .strict();
export const exportLocaleSchema = z.enum(['zh-CN', 'en']).default('zh-CN');
export const exportQuerySchema = z.object({ locale: z.enum(['zh-CN', 'en']).optional() }).strict();

export const rowQueryFields = {
  page: pageSchema,
  size: sizeSchema,
  sort_field: z.string().min(1).optional(),
  sort_order: z.enum(['asc', 'desc']).optional(),
};
export const validateRowSort = (
  value: { sort_field?: string; sort_order?: 'asc' | 'desc' },
  ctx: z.RefinementCtx,
) => {
  if (value.sort_field && !value.sort_order)
    ctx.addIssue({
      code: 'custom',
      path: ['sort_order'],
      message: 'sort_order is required with sort_field',
    });
};
export const rowQuerySchema = z
  .object({ table_id: tableIdSchema, ...rowQueryFields })
  .superRefine(validateRowSort);

export const rowValuesSchema = z
  .record(z.string(), cellValueSchema)
  .default({})
  .describe(
    'Map real column IDs from table_create/table_get to values matching each column data_type; use null for unknown values',
  );
export const createRowsSchema = z
  .object({
    table_id: tableIdSchema,
    rows: z
      .array(z.object({ values: rowValuesSchema }).strict())
      .min(1)
      .max(100),
  })
  .strict();
export const updateRowsSchema = z
  .object({
    table_id: tableIdSchema,
    rows: z
      .array(z.object({ id: z.string().min(1), values: rowValuesSchema }).strict())
      .min(1)
      .max(100),
  })
  .strict();

export const valueSchemas: Record<DataType, z.ZodType> = {
  string: z.string(),
  int: z.number().int(),
  float: z.number().finite(),
  bool: z.boolean(),
  date: dateValueSchema,
  people: peopleValueSchema,
  company: companyValueSchema,
  website: httpUrlSchema,
  email: z.string().email(),
  telephone: telephoneValueSchema,
  score: scoreValueSchema,
};

export type TableCreateInput = z.infer<typeof tableCreateSchema>;
export type TableListInput = z.infer<typeof tableListSchema>;
export type TableGetInput = z.infer<typeof tableGetSchema>;
export type TableAddColumnsInput = z.infer<typeof tableAddColumnsSchema>;
export type RowQueryInput = z.infer<typeof rowQuerySchema>;
export type CreateRowsInput = z.infer<typeof createRowsSchema>;
export type UpdateRowsInput = z.infer<typeof updateRowsSchema>;

/** Presentation priority; a hidden entry is an opportunity, never an email address. */
export function preferredContacts<T extends { kind: string }>(contacts: T[]): T[] {
  return contacts.some(c => c.kind === 'email')
    ? contacts.filter(c => c.kind !== 'hidden_email') : contacts;
}
export function contactDisplay(contact: {
  kind: string;
  normalized_value: string;
  original_value: string;
  source_excerpt?: string;
}): string {
  if (contact.kind === 'hidden_email') return `隐藏邮箱，在更多信息手动查看 — ${contact.normalized_value}`;
  if (contact.kind === 'social_profile' && isSocialProfile(contact.normalized_value)
      && new URL(contact.normalized_value).hostname.replace(/^www\./, '') === 'tiktok.com')
    return `TikTok 私信：${contact.normalized_value}`;
  if (contact.kind === 'social_profile' && isSocialProfile(contact.normalized_value)) {
    const host = new URL(contact.normalized_value).hostname.replace(/^www\./, '');
    if (host === 'douyin.com') return `抖音主页：${contact.normalized_value}`;
    if (host === 'facebook.com') return `Facebook 主页：${contact.normalized_value}`;
    if (host === 'youtube.com') return `YouTube 主页：${contact.normalized_value}`;
  }
  const labels: Record<string, string> = { wechat: '微信', wechat_official: '公众号', whatsapp: 'WhatsApp', telegram: 'Telegram' };
  const label = labels[contact.kind];
  const remark = ['wechat', 'wechat_official'].includes(contact.kind)
    ? (contact.source_excerpt ?? contact.original_value).match(/[（(]([^）)]{1,100})[）)]/)?.[1] : null;
  return `${label ? `${label}：` : ''}${contact.normalized_value}${remark ? `（${remark}）` : ''}`;
}
export function isSocialProfile(value: string): boolean {
  try {
    const u = new URL(value);
    if (u.protocol === 'https:' && ['facebook.com', 'www.facebook.com'].includes(u.hostname)
        && u.pathname === '/profile.php' && !u.username && !u.password && !u.port && !u.hash) {
      const pairs: [string, string][] = [];
      u.searchParams.forEach((value, key) => pairs.push([key, value]));
      return pairs.length === 1 && pairs[0][0] === 'id' && /^[1-9]\d{0,19}$/.test(pairs[0][1]);
    }
    if (
      u.protocol !== 'https:' ||
      u.username ||
      u.password ||
      u.port ||
      u.search ||
      u.hash ||
      value.includes('...') ||
      value.includes('…')
    )
      return false;
    const parts = u.pathname.replace(/^\/+|\/+$/g, '').split('/'),
      host = u.hostname.replace(/^www\./, '');
    const handle = (s: string) => /^[\p{L}\p{N}_.%_-]{1,128}$/u.test(s);
    if (['instagram.com', 'x.com', 'twitter.com', 'facebook.com'].includes(host))
      return (
        parts.length === 1 &&
        handle(parts[0]) &&
        ![
          'home',
          'explore',
          'search',
          'i',
          'accounts',
          'reel',
          'reels',
          'p',
          'groups',
          'watch',
          'login',
          'share',
          'marketplace',
        ].includes(parts[0])
      );
    if (host === 'youtube.com')
      return (
        (parts.length === 1 && parts[0].startsWith('@') && handle(parts[0].slice(1))) ||
        (parts.length === 2 && parts[0] === 'channel' && /^UC[A-Za-z0-9_-]{22}$/.test(parts[1]))
      );
    if (host === 'douyin.com')
      return parts.length === 2 && parts[0] === 'user' && /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(parts[1]);
    if (host === 'tiktok.com')
      return parts.length === 1 && parts[0].startsWith('@') && handle(parts[0].slice(1));
    if (host === 'space.bilibili.com') return parts.length === 1 && /^\d+$/.test(parts[0]);
    if (host === 'linkedin.com') return parts.length === 2 && parts[0] === 'in' && handle(parts[1]);
    if (host === 'reddit.com') return parts.length === 2 && ['user', 'u'].includes(parts[0]) && handle(parts[1]);
    if (host === 'bsky.app')
      return parts.length === 2 && parts[0] === 'profile' && handle(parts[1]);
    return false;
  } catch {
    return false;
  }
}
