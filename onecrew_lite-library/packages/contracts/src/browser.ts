import { z } from 'zod';

export const extensionIdSchema = z.string().regex(/^[a-p]{32}$/, 'invalid extension id');
export const connectSchema = z.object({
  extension_id: extensionIdSchema,
});
export const browserOpenInputSchema = z.object({ url: z.string().url() });
export const browserTabsResultSchema = z.object({
  tabs: z.array(
    z.object({
      tab_id: z.number().int(),
      url: z.string().url(),
      title: z.string(),
    }),
  ),
});
export const browserSnapshotResultSchema = z.object({
  tab_id: z.number().int(),
  url: z.string().url(),
  title: z.string(),
  text: z.string(),
});
export const resultSchema = z.object({
  id: z.string().min(1),
  result: z.unknown().optional(),
  error: z.object({ message: z.string() }).optional(),
});
