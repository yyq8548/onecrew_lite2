import { z } from 'zod';

export const youtubeUrlSchema = z
  .string()
  .url()
  .refine((value) => {
    try {
      const url = new URL(value);
      return (
        url.protocol === 'https:' &&
        (url.hostname === 'youtube.com' || url.hostname === 'www.youtube.com') &&
        !url.username &&
        !url.password &&
        !url.port
      );
    } catch {
      return false;
    }
  }, 'YouTube URL required');

export const searchKolsInputSchema = z.object({
  query: z.string().trim().min(1).max(200),
  limit: z.number().int().min(1).max(50),
});

const videoEvidenceSchema = z.object({
  title: z.string().min(1),
  url: youtubeUrlSchema,
  video_id: z.string().min(1),
  description: z.string().optional(),
  views_text: z.string().optional(),
  published_text: z.string().optional(),
});

const channelEvidenceSchema = z.object({
  platform_id: z.string().min(1),
  name: z.string().min(1),
  url: youtubeUrlSchema,
  image_url: z
    .string()
    .url()
    .refine((value) => {
      try {
        const parsed = new URL(value);
        return parsed.protocol === 'https:' && !parsed.username && !parsed.password;
      } catch {
        return false;
      }
    }, 'HTTPS URL required')
    .optional(),
  description: z.string().optional(),
  subscriber_text: z.string().optional(),
  matched_videos: z.array(videoEvidenceSchema).max(3),
  recent_videos: z.array(videoEvidenceSchema).max(3),
  warnings: z.array(z.string()).optional(),
});

export const channelsResultSchema = z.object({
  channels: z.array(channelEvidenceSchema).max(50),
  has_more: z.boolean(),
  stop_reason: z.enum(['limit_reached', 'exhausted', 'page_limit', 'video_limit', 'time_limit']),
  warnings: z.array(z.string()),
});

export const youtubeOperations = {
  'youtube.discover_kols': {
    inputSchema: searchKolsInputSchema,
    resultSchema: channelsResultSchema,
    timeoutMs: 240_000,
  },
} as const;
