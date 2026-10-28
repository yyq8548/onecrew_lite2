import { z } from 'zod';
import { OpenCliError } from './errors.ts';
import type { ReviewedOpenCliRead } from './read-plan.ts';

const id = z.string().regex(/^[a-f0-9]{24}$/u);
const text = (maximum: number) => z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= maximum && !value.includes('\0'));
const label = text(512).refine(value => !!value.trim() && !/\p{Cc}/u.test(value));
export function xiaohongshuNoteId(source: string): string | null {
  const url = URL.parse(source);
  if (!url || url.origin !== 'https://www.xiaohongshu.com' || url.username || url.password || url.hash || Buffer.byteLength(source) > 2048
    || [...url.searchParams.keys()].some(key => !['xsec_token', 'xsec_source'].includes(key))
    || url.searchParams.getAll('xsec_token').length !== 1 || url.searchParams.getAll('xsec_source').length !== 1
    || !/^[A-Za-z0-9_+/=-]{1,512}$/u.test(url.searchParams.get('xsec_token') ?? '') || url.searchParams.get('xsec_source') !== 'pc_search') return null;
  return /^\/explore\/([a-f0-9]{24})$/u.exec(url.pathname)?.[1] ?? null;
}
const identity = { author_id: id, author_label: label, profile_url: z.string() };
export const xiaohongshuProfileSchema = z.strictObject({ ...identity, bio: text(8000), bio_status: z.enum(['complete', 'missing']),
  follower_count: z.number().int().nonnegative().safe().nullable(), follower_display: text(100).nullable(), identity_source: z.literal('native_profile_page'),
  visible_posts: z.array(z.strictObject({ note_id: id, title: text(2000) })).max(3),
}).refine(value => value.profile_url === `https://www.xiaohongshu.com/user/profile/${value.author_id}`);
export const xiaohongshuContentSchema = z.strictObject({ note_id: id, author_id: id, author_label: label, author_url: z.string(), source_url: z.string(),
  title: text(2000), text: text(16000).refine(value => !!value.trim()), tags: z.array(text(200)).max(30), text_truncated: z.boolean(), text_completeness_known: z.literal(true),
}).refine(value => value.author_url === `https://www.xiaohongshu.com/user/profile/${value.author_id}` && xiaohongshuNoteId(value.source_url) === value.note_id);

export function validateXiaohongshuProfile(author: string, source: string, value: unknown): Record<string, unknown> {
  const native = xiaohongshuProfileSchema.parse(value);
  if (native.author_id !== author || native.profile_url !== source) throw new Error('review_native_profile_binding_unavailable');
  return native;
}
export function validateXiaohongshuContent(author: string, source: string, value: unknown): Record<string, unknown> {
  const native = xiaohongshuContentSchema.parse(value);
  if (native.author_id !== author || native.source_url !== source) throw new Error('review_native_content_binding_unavailable');
  return native;
}
export function xiaohongshuReadPlan(kind: 'profile' | 'content', author: string, source: string): ReviewedOpenCliRead<Record<string, unknown>> {
  if (!id.safeParse(author).success || !(kind === 'profile' ? source === `https://www.xiaohongshu.com/user/profile/${author}` : xiaohongshuNoteId(source))) throw new OpenCliError('opencli_invalid_input');
  return { site: 'onecrew-xiaohongshu', name: kind, args: ['--author', author, '-f', 'json', '--', source], addons: ['onecrew-xiaohongshu', 'onecrew-browser-search'], maximumBytes: 24000,
    decode(value: unknown) {
      if (!Array.isArray(value) || value.length !== 1) throw new OpenCliError('opencli_invalid_output');
      const result = (kind === 'profile' ? xiaohongshuProfileSchema : xiaohongshuContentSchema).safeParse(value[0]);
      if (!result.success || result.data.author_id !== author || (kind === 'profile' ? (result.data as z.infer<typeof xiaohongshuProfileSchema>).profile_url : (result.data as z.infer<typeof xiaohongshuContentSchema>).source_url) !== source) throw new OpenCliError('opencli_invalid_output', null, { stage: `xiaohongshu_${kind}_identity_validation`, exit_code: 0, stdout: '', stderr: result.success ? 'Author or source mismatch' : result.error.message });
      return result.data;
    } };
}
