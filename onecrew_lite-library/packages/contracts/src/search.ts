import { z } from 'zod';
export const searchCreateSchema = z.object({
  platform: z.literal('youtube'),
  query: z.string().trim().min(1).max(200),
  limit: z.number().int().min(1).max(50),
});
export const searchPageSchema = z.object({
  page: z.coerce.number().int().min(1).default(1),
  size: z.coerce.number().int().min(1).max(100).default(20),
});
export const searchIdSchema = z.object({ search_id: z.string().min(1) });
export const searchMcpPageSchema = z.object({
  search_id: z.string().min(1),
  page: z.number().int().min(1).default(1),
  size: z.number().int().min(1).max(100).default(20),
});
