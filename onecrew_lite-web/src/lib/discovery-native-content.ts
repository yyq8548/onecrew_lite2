import { z } from "zod";

const numeric = z.string().regex(/^[1-9][0-9]{0,19}$/);
const text = (bytes: number) => z.string().refine(value => new TextEncoder().encode(value).length <= bytes && !value.includes("\u0000"));
const label = text(512).refine(value => value.trim().length > 0 && !/[\u0000-\u001f\u007f]/.test(value));
const links = z.array(z.string().url().max(2048)).max(10);
const linkedinHandle = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9_-]{0,99}$/).refine(value => !["me", "self"].includes(value.toLowerCase()));
export const linkedinAccount = z.string().regex(/^ACo[A-Za-z0-9_-]{16,125}$/);
const linkedinBody = z.object({ account_id: linkedinAccount, author_handle: linkedinHandle, profile_url: z.string().url(),
  post_id: numeric, source_url: z.string().url(), text: text(16000).refine(value => value.trim().length > 0),
  text_truncated: z.literal(true), evidence_source: z.literal("visible_author_activity") }).strict();
const weiboBody = z.object({ post_id: numeric, author_id: numeric, author: label, source_url: z.string().url(),
  text: text(16000), text_truncated: z.boolean(), text_completeness_known: z.boolean(), is_repost: z.literal(false) }).strict();
const zhihuAuthor = { author_id: z.string().regex(/^[a-f0-9]{32}$/), url_token: z.string().regex(/^[A-Za-z0-9_-]{1,128}$/),
  author_label: label, author_url: z.string().url(), source_url: z.string().url(), text: text(16000).refine(value => value.trim().length > 0), public_links: links };
const zhihuArticle = z.object({ ...zhihuAuthor, article_id: numeric, title: text(2000) }).strict();
const zhihuAnswer = z.object({ ...zhihuAuthor, answer_id: numeric, question_id: numeric }).strict();
const caption = z.object({ post_id: z.string(), author_id: z.string(), source_url: z.string().url(),
  text: text(16000), text_truncated: z.boolean() }).strict();

/** Exact owner receipt shapes. Search titles and arbitrary JSON cannot pass as native text. */
export const additionalNativeContentSchema = z.union([linkedinBody, weiboBody, zhihuArticle, zhihuAnswer]);
export const additionalNativePlatforms = ["linkedin", "pixiv", "weibo", "zhihu", "instagram", "facebook"] as const;
type Claim = { platform: string; author_id: string; source_url: string };

export function additionalNativeContentBound(claim: Claim, value: unknown): boolean {
  if (claim.platform === "linkedin") {
    const result = linkedinBody.safeParse(value);
    return result.success && result.data.author_handle === claim.author_id
      && result.data.profile_url === claim.source_url && claim.source_url === `https://www.linkedin.com/in/${result.data.author_handle}/`
      && result.data.source_url === `https://www.linkedin.com/feed/update/urn:li:activity:${result.data.post_id}/`;
  }
  if (claim.platform === "weibo") {
    const result = weiboBody.safeParse(value);
    return result.success && result.data.author_id === claim.author_id && result.data.source_url === claim.source_url
      && new RegExp(`^https://weibo\\.com/${result.data.author_id}/[A-Za-z0-9]{1,20}$`).test(claim.source_url);
  }
  if (claim.platform === "zhihu") {
    const result = z.union([zhihuArticle, zhihuAnswer]).safeParse(value);
    if (!result.success) return false;
    const body = result.data;
    return body.author_id === claim.author_id && body.author_url === `https://www.zhihu.com/people/${body.url_token}`
      && body.source_url === claim.source_url && body.source_url === ("article_id" in body
        ? `https://zhuanlan.zhihu.com/p/${body.article_id}` : `https://www.zhihu.com/question/${body.question_id}/answer/${body.answer_id}`);
  }
  const result = caption.safeParse(value);
  if (!result.success || result.data.author_id !== claim.author_id) return false;
  const body = result.data;
  if (claim.platform === "instagram") return /^[1-9][0-9]{0,24}$/.test(body.post_id) && /^[1-9][0-9]{0,24}$/.test(body.author_id)
    && /^https:\/\/www\.instagram\.com\/[a-z0-9_.]{1,30}\/$/.test(claim.source_url)
    && /^https:\/\/www\.instagram\.com\/(p|reel)\/[A-Za-z0-9_-]{1,32}\/$/.test(body.source_url)
    && new TextEncoder().encode(body.text).length <= 8000;
  if (claim.platform === "pixiv") return numeric.safeParse(body.author_id).success && numeric.safeParse(body.post_id).success
    && body.source_url === claim.source_url && claim.source_url === `https://www.pixiv.net/artworks/${body.post_id}` && body.text.trim().length > 0;
  if (claim.platform === "facebook") return numeric.safeParse(body.author_id).success && /^[1-9][0-9]{0,24}$/.test(body.post_id)
    && body.source_url === claim.source_url && claim.source_url === `https://www.facebook.com/reel/${body.post_id}`
    && body.text.trim().length > 0 && new TextEncoder().encode(body.text).length <= 8000;
  return false;
}

/** The saved review uses a native ID even when the read claim uses a page handle. */
export function additionalNativeReviewBound(platform: string, claim: Claim, value: unknown, author: string, profile: string, hitSource: string): boolean {
  if (!additionalNativeContentBound(claim, value) || !value || typeof value !== "object") return false;
  if (platform === "linkedin") {
    const body = linkedinBody.parse(value);
    return body.account_id === author && body.profile_url === profile && claim.source_url === hitSource;
  }
  if (claim.author_id !== author || (platform !== "facebook" && claim.source_url !== hitSource)) return false;
  if (platform === "zhihu") return z.union([zhihuArticle, zhihuAnswer]).parse(value).author_url === profile;
  if (platform === "weibo") return profile === `https://weibo.com/u/${author}`;
  if (platform === "pixiv") return profile === `https://www.pixiv.net/users/${author}`;
  if (platform === "instagram") return profile === hitSource;
  if (platform === "facebook") return profile === `https://www.facebook.com/profile.php?id=${author}`;
  return false;
}
