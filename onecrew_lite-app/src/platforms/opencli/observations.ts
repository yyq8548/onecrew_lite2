import { z } from "zod";

const count = z.number().int().nonnegative().safe();
// 仅保留同一作品原生响应中已有的观察值；缺失字段不推算为零。
export const nativeContentObservations = z.strictObject({
  published_at: z.string().min(1).max(120).nullable().optional(),
  view_count: count.nullable().optional(),
  like_count: count.nullable().optional(),
  comment_count: count.nullable().optional(),
  share_count: count.nullable().optional(),
  save_count: count.nullable().optional(),
  score: z.number().int().safe().nullable().optional(),
  duration_seconds: z.number().nonnegative().finite().nullable().optional(),
  text_completeness: z.enum(["complete", "truncated", "unknown"]).optional(),
});
