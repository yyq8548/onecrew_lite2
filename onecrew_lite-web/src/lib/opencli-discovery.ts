import { z } from "zod";
import { isU64 } from "@onecrew/runtime-contracts/result-wire";
import { stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
const u64 = z.custom<number | bigint>(isU64);
import { invoke } from "./local-runtime";
import { additionalNativeContentSchema, additionalNativePlatforms, additionalNativeContentBound, linkedinAccount } from "./discovery-native-content";

const runSchema = z.object({ platform: z.string(), profile_ref: z.string(), status: z.string(), calls_used: z.number(), stop_code: z.string().nullable(), updated_at: z.number().int().optional() });
const campaignSchema = z.object({
  id: z.string(), phase: z.string(), revision: z.number(), brief: z.string(),
  opencli_version: z.string(),
  criteria: z.object({ min_subscribers: u64.optional(), min_followers: u64.optional(), region: z.string().optional(), language: z.string().optional(), category: z.string().optional(), requires_ai_tool_demo: z.boolean().optional(), requires_public_tool_url: z.boolean().optional() }).passthrough(),
  target_count: z.number(), max_calls: z.number().nullable(), used_calls: z.number(),
  scopes: z.array(z.object({ platform: z.string(), profile_ref: z.string(), keywords: z.array(z.string()) })),
  platform_runs: z.array(runSchema),
});
const hitSchema = z.object({
  id: z.string(), platform: z.string(), source_url: z.string().url(), title: z.string(), author_label: z.string(),
  search_keyword: z.string(), captured_at: z.number(), profile_ref: z.string().min(1),
  author_id: z.string().min(1).max(100).nullable(), author_url: z.string().url().nullable(),
  author_observed_at: z.number().int().nullable(),
}).refine(hit => (hit.author_id === null) === (hit.author_url === null)
  && (hit.author_id === null) === (hit.author_observed_at === null));
const candidateSchema = z.object({
  id: z.string(), platform: z.string(), platform_account_id: z.string(),
  display_name: z.string(), profile_url: z.string().url(), verdict: z.enum(["pending", "qualified", "rejected"]),
  evidence: z.object({
    manual_profile_contact: z.boolean().optional(),
    manual_wechat_accounts: z.array(z.object({ kind: z.enum(["wechat", "wechat_official"]), value: z.string().min(1).max(40), source_url: z.string().url(), native_author_id: z.string() })).max(10).optional(),
    associated_social_accounts: z.array(z.object({
      kind: z.literal("social_profile"), normalized_value: z.string().url(),
      source_url: z.string().url(), captured_at: z.number().int().positive(),
      native_author_id: z.string(), attribution: z.enum(["author_published", "unconfirmed"]),
    })).max(100).optional(),
    contact_status: z.object({ has_contact: z.boolean(), manual_email_required: z.boolean() }).optional(),
    source_urls: z.array(z.string().url()),
    public_links: z.array(z.string().url()).optional(),
    assessment: z.object({ reasons: z.array(z.string()), subscribers_normalized: u64.nullable(),
      pinterest_scale: z.object({ state: z.enum(["passed", "failed", "unknown"]), passed_by: z.array(z.enum(["followers", "monthly_views"])).optional() }).optional(),
      match_rating: z.object({ level: z.enum(["excellent", "good", "medium", "low"]), reason: z.string(), source_url: z.string().url() }).nullable().optional(),
    }).optional(),
  }).passthrough(),
});
const countsSchema = z.object({ qualified: z.number().int().nonnegative(), pending: z.number().int().nonnegative(), rejected: z.number().int().nonnegative() });
export type DiscoveryCampaign = z.infer<typeof campaignSchema>;
export type DiscoveryHit = z.infer<typeof hitSchema>;
export type DiscoveryCandidate = z.infer<typeof candidateSchema>;
export type DiscoveryCandidateCounts = z.infer<typeof countsSchema>;
const campaignSummarySchema = z.object({
  id: z.string().min(1), brief: z.string(), phase: z.string(),
  target_count: z.number().int().nonnegative(), used_calls: z.number().int().nonnegative(),
  max_calls: z.number().int().nonnegative().nullable(), updated_at: z.number().int(),
}).strict();
export type DiscoveryCampaignSummary = z.infer<typeof campaignSummarySchema>;
// Same bounded ASCII identity accepted by the Campaign owner, including MCP IDs.
export function validDiscoveryCampaignId(id: string): boolean {
  return /^[A-Za-z0-9_-]{1,128}$/.test(id);
}
export async function listRecentDiscoveryCampaigns(): Promise<DiscoveryCampaignSummary[]> {
  return z.array(campaignSummarySchema).max(5).parse(await invoke<unknown>("onecrew_discovery_campaigns_recent", {}));
}
export async function prepareDiscoveryDraft(input: {
  id: string; brief: string; profile_ref: string; opencli_version: string;
  target_count: number | null; max_calls: number | null;
  read_budget?: { mode: "default" | "explicit"; limit: number | null };
  min_followers_override?: number | bigint | null;
  scope_overrides?: { platform: string; keywords: string[] }[];
  refinement?: { campaign_id: string; expected_revision: number } | null;
  criteria_overrides?: { region: string | null; language: string | null; category: string | null; requires_ai_tool_demo: boolean; requires_public_tool_url: boolean };
}): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_draft_prepare", { input }));
}
const librarySyncSchema = z.object({
  campaign_id: z.string(), table_id: z.string(), web_url: z.string().url(),
  total_hits: u64, inserted_rows: u64,
});
export type DiscoveryLibrarySync = z.infer<typeof librarySyncSchema>;

export async function getDiscoveryLibrary(campaignId: string): Promise<DiscoveryLibrarySync | null> {
  const result = librarySyncSchema.nullable().parse(await invoke<unknown>("onecrew_discovery_library_get", { campaignId }));
  if (result && result.campaign_id !== campaignId) throw new Error("library_sync_campaign_mismatch");
  return result;
}

export async function syncDiscoveryLibrary(campaignId: string): Promise<DiscoveryLibrarySync> {
  const result = librarySyncSchema.parse(await invoke<unknown>("onecrew_discovery_library_sync", { campaignId }));
  if (result.campaign_id !== campaignId) throw new Error("library_sync_campaign_mismatch");
  return result;
}

/** Keep node, community, tag and category scopes separate from free-text search. */
export const TAG_SEARCH_PLATFORMS = ["devto-tag", "lesswrong-tag", "lobsters-tag", "medium-tag", "stackoverflow-tag"] as const;
export const CATEGORY_SEARCH_PLATFORMS = ["juejin-hot", "producthunt-posts"] as const;

const categoryValues: Record<string, string[]> = {
  "juejin-hot": ["backend", "frontend", "android", "ios", "ai"],
  "producthunt-posts": ["ai-agents", "ai-coding-agents", "ai-code-editors", "ai-chatbots", "ai-workflow-automation", "vibe-coding", "developer-tools", "productivity", "design-creative", "marketing-sales", "no-code-platforms", "llms", "finance", "social-community", "engineering-development"],
};

export function discoveryScopeKeywords(platform: string, keywords: string, v2exNodes: string, redditCommunities: string, tags: Record<string, string> = {}, categories: Record<string, string> = {}): string[] {
  const input = platform === "v2ex-node" ? v2exNodes
    : platform === "reddit-subreddit" ? redditCommunities
      : TAG_SEARCH_PLATFORMS.some(source => source === platform) ? tags[platform] ?? ""
        : CATEGORY_SEARCH_PLATFORMS.some(source => source === platform) ? categories[platform] ?? "" : keywords;
  return input.split(/[,，\n]/).map(value => value.trim()).filter(Boolean);
}

export function validV2exNodes(nodes: string[]): boolean {
  return nodes.length > 0 && nodes.length <= 20
    && new Set(nodes).size === nodes.length
    && nodes.every(node => /^[a-z0-9][a-z0-9_-]{0,63}$/.test(node));
}

export function validRedditCommunities(communities: string[]): boolean {
  return communities.length > 0 && communities.length <= 20
    && new Set(communities.map(name => name.toLowerCase())).size === communities.length
    && communities.every(name => /^[A-Za-z0-9_]{1,64}$/.test(name));
}

export function validScopedTags(tags: string[]): boolean {
  return tags.length > 0 && tags.length <= 20
    && new Set(tags.map(tag => tag.toLowerCase())).size === tags.length
    && tags.every(tag => tag.length <= 160 && !/[\x00-\x1f\x7f]/.test(tag));
}

export function validCategoryScopes(platform: string, categories: string[]): boolean {
  return categories.length > 0 && categories.length <= 20
    && new Set(categories).size === categories.length
    && categories.every(category => categoryValues[platform]?.includes(category));
}

/** Blank scope fields are optional; supplied fields still use native validators. */
export function validDiscoveryScope(platform: string, values: string[]): boolean {
  return platform === "v2ex-node" ? validV2exNodes(values)
    : platform === "reddit-subreddit" ? validRedditCommunities(values)
      : CATEGORY_SEARCH_PLATFORMS.some(source => source === platform) ? validCategoryScopes(platform, values)
        : validScopedTags(values);
}

const contactLookupSchema = z.object({
 campaign_id: z.string(), expected_revision: z.number().int().positive(), candidate_id:z.string(), profile_ref:z.string(),
 method:z.enum(["maigret","search"]), native_author_id:z.string().min(1), author_label:z.string(), username:z.string().nullable(),
 query:z.string(), call_number:z.number().int().nonnegative(),status:z.enum(["planned","reading","completed","failed"]),
 links:z.array(z.string().url()).max(5),stop_code:z.string().nullable(),
});
const stepSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("search"), platform: z.string(), profile_ref: z.string() }),
  z.object({ kind: z.literal("read_video"), profile_ref: z.string(), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_transcript"), profile_ref: z.string(), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_profile"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_identity"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_facebook_identity"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_source_author"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_web_source"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_content"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_content_targets"), search_hit_id: z.string() }),
  z.object({ kind: z.literal("read_contact_lookup"), lookup:contactLookupSchema }),
  z.object({ kind: z.literal("read_public_page"), profile_ref: z.string(), candidate_id: z.string(), requested_url: z.string().url() }),
  z.object({ kind: z.enum(["busy", "target_reached", "budget_exhausted", "needs_evidence", "search_exhausted"]) }),
]);
const transcriptSchema = z.object({
  search_hit_id: z.string(), candidate_id: z.string(), source_url: z.string().url(),
  profile_ref: z.string(), platform_account_id: z.string(),
  status: z.enum(["reading", "completed", "unavailable", "failed"]),
  segments: z.array(z.object({ timestamp: z.string(), speaker: z.string(), text: z.string() })).max(200),
  truncated: z.boolean(), stop_code: z.string().nullable(), call_number: z.number().int().positive(), captured_at: z.number(),
});
const profileSchema = z.object({
  claim: z.object({ campaign_id: z.string(), expected_revision: z.number().int().positive(), search_hit_id: z.string(),
    platform: z.string(), profile_ref: z.string(), author_id: z.string(), author_url: z.string().url(),
    source_url: z.string().url(), call_number: z.number().int().positive() }),
  status: z.enum(["reading", "completed", "failed"]), native_profile: z.record(z.string(), z.unknown()).nullable(),
  stop_code: z.string().nullable(), captured_at: z.number().int(),
}).refine(read => read.status === "completed" ? read.native_profile !== null && read.stop_code === null
  : read.native_profile === null && (read.status === "failed" ? read.stop_code !== null : read.stop_code === null));
const contentSchema = z.object({
  language_sample: z.object({ author_id: z.string(), source_url: z.string().url(), text: z.string().max(16000),
    comments: z.array(z.object({ source_url: z.string().url(), text: z.string().max(2000) }).strict()).max(5) }).strict().optional(),
  claim: z.object({ campaign_id: z.string(), expected_revision: z.number().int().positive(), search_hit_id: z.string(),
    platform: z.enum(["bilibili", "twitter", "reddit", "facebook", "youtube", "tiktok", "douyin", "instagram", "weibo", "zhihu", "pinterest", "pixiv", "linkedin", "google", "yahoo"]), profile_ref: z.string(), author_id: z.string(), source_url: z.string().url(),
    call_number: z.number().int().positive() }),
  status: z.enum(["reading", "completed", "failed"]), native_content: z.union([additionalNativeContentSchema, z.object({
    bvid: z.string(), author_id: z.string(), author: z.string(), source_url: z.string().url(),
    title: z.string(), description: z.string(), description_truncated: z.boolean(),
    requires_payment: z.boolean(), pay_preview: z.boolean(), payment_status_known: z.boolean().optional(),
  }), z.object({
    post_id: z.string().regex(/^[1-9][0-9]{0,19}$/),
    author_id: z.string().regex(/^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/), author_label: z.string().min(1).max(512),
    profile_url: z.string().url(), source_url: z.string().url(), text: z.string().min(1).max(12000),
    text_truncated: z.boolean(), evidence_source: z.literal("visible_native_video_caption"),
  }).strict(), z.object({
    post_id: z.string(), author_id: z.string(), source_url: z.string().url(), text: z.string(), text_truncated: z.boolean(),
  }).strict(), z.object({
    author_id: z.string(), profile_url: z.string().url(), sampled_posts: z.number().int().min(1).max(5),
    excluded_retweets: z.number().int().min(0).max(5), identity_kind: z.literal("mutable_handle"),
    original_posts: z.array(z.object({ id: z.string(), source_url: z.string().url(), text: z.string(),
      text_truncated: z.boolean(), created_at: z.string(), has_media: z.boolean(), quotes_another_post: z.boolean() })).max(5),
    }), z.object({
      adapter_version: z.literal("1.0.0"),
      identity: z.object({ adapter_version: z.literal("1.0.0"),
        post_id: z.string().regex(/^[1-9][0-9]{0,19}$/), account_id: z.string().regex(/^[1-9][0-9]{0,19}$/),
        handle: z.string().regex(/^[a-z0-9_]{1,15}$/), profile_url: z.string().url(), source_url: z.string().url(),
        identity_kind: z.literal("native_post_author_id"), evidence_source: z.literal("x_tweet_detail"),
      }).strict(),
      text: z.string().min(1), text_truncated: z.boolean(), text_source: z.enum(["legacy_full_text", "note_tweet"]),
      created_at: z.string(), has_media: z.boolean(), quotes_another_post: z.boolean(),
      expanded_urls: z.array(z.string().url()).max(20),
    }).strict()]).nullable(), stop_code: z.string().nullable(), captured_at: z.number().int(),
}).refine(read => read.status === "completed" ? read.stop_code === null && (read.native_content !== null || read.language_sample !== undefined)
  : read.native_content === null && (read.status === "failed" ? read.stop_code !== null : read.stop_code === null))
  .refine(read => !read.language_sample || (read.status === "completed" && read.native_content === null
    && read.language_sample.author_id === read.claim.author_id && read.language_sample.source_url === read.claim.source_url
    && (read.language_sample.text.trim().length > 0 || read.language_sample.comments.length > 0)
    && new TextEncoder().encode(read.language_sample.text).length <= (read.claim.platform === "douyin" ? 12000 : ["linkedin", "weibo", "zhihu"].includes(read.claim.platform) ? 16000 : 8000)
    && !read.language_sample.text.includes("\u0000") && read.language_sample.comments.every(c => !c.text.includes("\u0000")
      && new TextEncoder().encode(c.text).length <= 2000 && (["douyin", "linkedin", "zhihu"].includes(read.claim.platform) || c.text.trim().length > 0))))
  .refine(read => {
    const content = read.native_content;
    if (["google", "yahoo"].includes(read.claim.platform)) {
      const sample = read.language_sample;
      const prefix = `${read.claim.source_url}&lc=`;
      return content === null && /^UC[A-Za-z0-9_-]{22}$/.test(read.claim.author_id)
        && /^https:\/\/www\.youtube\.com\/watch\?v=[A-Za-z0-9_-]{11}$/.test(read.claim.source_url)
        && (!sample ? read.status !== "completed" :
          new Set(sample.comments.map(c => c.source_url)).size === sample.comments.length
          && sample.comments.every(c => c.source_url.startsWith(prefix) && /^[A-Za-z0-9_.-]{1,200}$/.test(c.source_url.slice(prefix.length))));
    }
    if (content && (additionalNativePlatforms as readonly string[]).includes(read.claim.platform))
      return additionalNativeContentBound(read.claim, content);
    if (read.claim.platform === "linkedin") {
      const sample = read.language_sample;
      if (!sample) return read.status !== "completed";
      const prefix = `${read.claim.source_url}#comment-`;
      return linkedinAccount.safeParse(read.claim.author_id).success
        && /^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:[1-9][0-9]{0,19}\/$/.test(read.claim.source_url)
        && sample.text.trim().length > 0 && new Set(sample.comments.map(c => c.source_url)).size === sample.comments.length
        && sample.comments.every(c => c.source_url.startsWith(prefix) && /^[1-9][0-9]{0,19}$/.test(c.source_url.slice(prefix.length)));
    }
    if (content && "evidence_source" in content) return read.claim.platform === "douyin" && "author_id" in content
      && content.author_id === read.claim.author_id && content.source_url === read.claim.source_url
      && content.source_url === `https://www.douyin.com/video/${content.post_id}`
      && content.profile_url === `https://www.douyin.com/user/${content.author_id}`;
    if (read.claim.platform === "douyin") {
      const sample = read.language_sample;
      if (!sample) return read.status !== "completed";
      const prefix = `${read.claim.source_url}#comment-`;
      return /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(read.claim.author_id)
        && /^https:\/\/www\.douyin\.com\/video\/[1-9][0-9]{0,19}$/.test(read.claim.source_url)
        && sample.text.trim().length > 0 && new Set(sample.comments.map(c => c.source_url)).size === sample.comments.length
        && sample.comments.every(c => c.source_url.startsWith(prefix) && /^[1-9][0-9]{0,19}$/.test(c.source_url.slice(prefix.length)));
    }
    if (!content || !("identity" in content)) return true; // Historical receipts retain their original shape.
    const identity = content.identity;
    const source = `https://x.com/${identity.handle}/status/${identity.post_id}`;
    return read.claim.platform === "twitter" && identity.handle === read.claim.author_id
      && identity.profile_url === `https://x.com/${identity.handle}` && identity.source_url === source
      && [source, `https://x.com/i/status/${identity.post_id}`].includes(read.claim.source_url);
  });
const identitySchema = z.object({
  claim: z.object({ campaign_id: z.string(), expected_revision: z.number().int().positive(), search_hit_id: z.string(),
    profile_ref: z.string(), author_handle: z.string().regex(/^[a-z0-9_]{1,15}$/),
    source_url: z.string().url(), post_id: z.string().regex(/^[1-9][0-9]{0,19}$/), call_number: z.number().int().positive() }),
  status: z.enum(["reading", "completed", "failed"]), account_id: z.string().regex(/^[1-9][0-9]{0,19}$/).nullable(),
  observed_source_url: z.string().url().nullable(), stop_code: z.string().nullable(), captured_at: z.number().int(),
}).refine(read => {
  const expected = `https://x.com/${read.claim.author_handle}/status/${read.claim.post_id}`;
  if (read.claim.source_url !== expected && read.claim.source_url !== `https://x.com/i/status/${read.claim.post_id}`) return false;
  return read.status === "completed" ? read.account_id !== null && read.observed_source_url === expected && read.stop_code === null
    : read.account_id === null && read.observed_source_url === null && (read.status === "failed" ? read.stop_code !== null : read.stop_code === null);
});
const publicPageSchema = z.object({
  candidate_id: z.string(), requested_url: z.string().url(), search_hit_id: z.string(), source_url: z.string().url(),
  source_excerpt: z.string(), profile_ref: z.string(), platform_account_id: z.string(),
  call_number: z.number().int().positive(), status: z.enum(["reading", "completed", "failed"]),
  document_url: z.string().url().nullable(), title: z.string(), markdown_excerpt: z.string(),
  truncated: z.boolean(), stop_code: z.string().nullable(), captured_at: z.number().int(),
}).refine(read => read.status === "completed"
  ? read.document_url !== null && read.title.trim().length > 0 && read.markdown_excerpt.trim().length > 0 && read.stop_code === null
  : read.document_url === null && read.title === "" && read.markdown_excerpt === "" && !read.truncated
    && (read.status === "failed" ? read.stop_code !== null : read.stop_code === null));
const sourceAuthorSchema = z.object({
  claim: z.object({ campaign_id: z.string(), expected_revision: z.number().int().positive(), search_hit_id: z.string(),
    platform: z.literal("douyin"), profile_ref: z.string(), source_url: z.string().url(), post_id: z.string().regex(/^[1-9][0-9]{0,19}$/), call_number: z.number().int().positive() }).strict(),
  status: z.enum(["completed", "failed"]), author_id: z.string().regex(/^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/).nullable(),
  profile_url: z.string().url().nullable(), duplicate_of: z.string().nullable(), stop_code: z.string().nullable(), captured_at: z.number().int(),
}).strict().refine(read => read.claim.source_url === `https://www.douyin.com/video/${read.claim.post_id}`
  && (read.status === "completed" ? read.author_id !== null && read.profile_url === `https://www.douyin.com/user/${read.author_id}` && read.stop_code === null
    : read.author_id === null && read.profile_url === null && read.duplicate_of === null && read.stop_code !== null));
const webSourceSchema = z.object({
  claim: z.object({campaign_id:z.string(),expected_revision:z.number().int().positive(),search_hit_id:z.string(),
    origin_platform:z.enum(["google","yahoo"]),profile_ref:z.string(),source_url:z.string().url(),target_platform:z.literal("youtube"),
    video_id:z.string().regex(/^[A-Za-z0-9_-]{11}$/),call_number:z.number().int().positive()}).strict(),
  status:z.enum(["completed","failed"]),native_author_id:z.string().regex(/^UC[A-Za-z0-9_-]{22}$/).nullable(),candidate_id:z.string().min(1).nullable(),is_primary_source:z.boolean(),
  native_material:z.object({adapterVersion:z.literal("1.0.0"),videoId:z.string(),channelId:z.string(),sourceUrl:z.string().url(),
    title:z.string().min(1),author:z.string().min(1),subscribers:z.string(),description:z.string().min(1),
    descriptionSource:z.literal("ytInitialPlayerResponse.videoDetails.shortDescription"),displayDescription:z.string(),
    links:z.array(z.object({url:z.string().url(),label:z.string(),source:z.enum(["shortDescription","description.runs","attributedDescription.commandRuns"])}).strict()).max(100),
    playabilityStatus:z.literal("OK")}).strict().nullable(),stop_code:z.string().nullable(),captured_at:z.number().int().nonnegative(),
}).strict().refine(read=>{
  const source=new URL(read.claim.source_url);
  const parts=source.pathname.split("/");
  const ids=source.searchParams.getAll("v");
  const nativeTarget=source.pathname==="/watch" ? ids.length===1 && ids[0]===read.claim.video_id
    : parts.length===3 && ["shorts","live"].includes(parts[1]) && parts[2]===read.claim.video_id;
  return source.protocol==="https:" && ["youtube.com","www.youtube.com"].includes(source.hostname)
    && !source.username && !source.password && !source.port && !source.hash && nativeTarget
    && (read.status==="completed" ? read.native_author_id!==null && read.candidate_id!==null && read.stop_code===null && read.native_material!==null
      && read.native_material.channelId===read.native_author_id && read.native_material.videoId===read.claim.video_id
      && read.native_material.sourceUrl===`https://www.youtube.com/watch?v=${read.claim.video_id}`
      && new TextEncoder().encode(stringifyApplicationJson(read.native_material)).length<=65536
      : read.native_author_id===null && read.candidate_id===null && !read.is_primary_source && read.native_material===null && read.stop_code!==null);
});
const facebookIdentitySourceSchema = z.string().regex(/^https:\/\/www\.facebook\.com\/[A-Za-z0-9.]{1,100}\/?$/).refine(source => {
  const handle = new URL(source).pathname.replace(/^\//, "").replace(/\/$/, "").toLowerCase();
  return !/^[0-9.]+$/.test(handle) && !["about", "ads", "business", "challenge", "checkpoint", "directory", "events", "friends", "gaming", "groups", "help", "home", "login", "login.php", "marketplace", "messages", "notifications", "pages", "photo", "photos", "privacy", "profile", "profile.php", "recover", "reel", "reels", "search", "settings", "share", "stories", "two_step_verification", "watch"].includes(handle);
});
const facebookIdentitySchema = z.object({
  claim: z.object({ campaign_id:z.string(), search_hit_id:z.string(), expected_revision:z.number().int().positive(), profile_ref:z.string().min(1), source_url:facebookIdentitySourceSchema, call_number:z.number().int().positive() }).strict(),
  status:z.enum(["completed","failed"]),
  native_identity:z.object({ adapter_version:z.literal("1.0.0"), source_url:facebookIdentitySourceSchema, id:z.string().regex(/^[1-9][0-9]{0,19}$/), display_name:z.string().min(1).max(200).refine(name=>name.trim().length>0&&!/[\u0000-\u001f\u007f]/.test(name)), profile_url:facebookIdentitySourceSchema, evidence_source:z.literal("native_profile_header_response") }).strict().nullable(),
  stop_code:z.string().min(1).max(128).nullable(), captured_at:z.number().int(),
}).strict().refine(read=>read.status==="completed"
  ? read.stop_code===null && read.native_identity!==null && read.native_identity.source_url===read.claim.source_url
    && read.native_identity.profile_url.replace(/\/$/,"").toLowerCase()===read.claim.source_url.replace(/\/$/,"").toLowerCase()
    && new TextEncoder().encode(stringifyApplicationJson(read.native_identity)).length<=4000
  : read.stop_code!==null && read.native_identity===null);
const advanceResultSchema = z.object({ step: stepSchema, campaign: campaignSchema, counts: countsSchema,
  facebook_identity:facebookIdentitySchema.optional(),
  web_source:webSourceSchema.optional(),
  source_author: sourceAuthorSchema.optional(),
  source_author_timings: z.object({ content_ms: z.number().int().nonnegative().nullable(), profile_ms: z.number().int().nonnegative().nullable() }).strict().optional(),
  content_targets: z.object({ claim: z.object({ campaign_id: z.string(), expected_revision: z.number().int().positive(), search_hit_id: z.string(),
    platform: z.literal("facebook"), profile_ref: z.string(), author_id: z.string(), author_url: z.string().url(), source_url: z.string().url(), call_number: z.number().int().positive() }),
    status: z.enum(["completed", "failed"]), source_urls: z.array(z.string().regex(/^https:\/\/www\.facebook\.com\/reel\/[1-9][0-9]{0,24}$/)).min(1).max(3).nullable(),
    stop_code: z.string().nullable(), captured_at: z.number().int() }).optional(),
  provider_stop_code: z.string().min(1).max(128).optional(),
  public_page: publicPageSchema.optional(),
  contact_lookup:contactLookupSchema.optional(),
  transcript: transcriptSchema.nullable(), profile: profileSchema.nullable(), content: contentSchema.nullable(), identity: identitySchema.nullable() });
function committedProviderStop(result: z.infer<typeof advanceResultSchema>): boolean {
  return !!result.provider_stop_code && result.campaign.platform_runs.some(run =>
    run.stop_code === result.provider_stop_code && ["unauthorized", "technical_stop", "risk_control_stop"].includes(run.status));
}
const advanceSchema = advanceResultSchema
  .refine(result=>result.step.kind==="read_facebook_identity"
    ? result.facebook_identity!==undefined && result.facebook_identity.claim.campaign_id===result.campaign.id
      && result.facebook_identity.claim.expected_revision===result.campaign.revision
      && result.facebook_identity.claim.search_hit_id===result.step.search_hit_id
      && result.facebook_identity.claim.call_number===result.campaign.used_calls
      && result.campaign.scopes.some(scope=>scope.platform==="facebook" && scope.profile_ref===result.facebook_identity?.claim.profile_ref)
    : result.facebook_identity===undefined)
  .refine(result=>result.step.kind==="read_web_source"
    ? result.web_source!==undefined && result.web_source.claim.campaign_id===result.campaign.id
      && result.web_source.claim.expected_revision===result.campaign.revision
      && result.web_source.claim.search_hit_id===result.step.search_hit_id
      && result.web_source.claim.call_number===result.campaign.used_calls
      && result.campaign.scopes.some(scope=>scope.platform===result.web_source?.claim.origin_platform && scope.profile_ref===result.web_source?.claim.profile_ref)
    : result.web_source===undefined)
  .refine(result => result.source_author_timings === undefined || result.step.kind === "read_source_author")
  .refine(result => result.step.kind === "read_source_author"
    ? result.source_author !== undefined && result.source_author.claim.campaign_id === result.campaign.id
      && result.source_author.claim.expected_revision === result.campaign.revision
      && result.source_author.claim.search_hit_id === result.step.search_hit_id
      && result.source_author.claim.call_number === result.campaign.used_calls
      && result.campaign.scopes.some(scope => scope.platform === "douyin" && scope.profile_ref === result.source_author?.claim.profile_ref)
    : result.source_author === undefined)
  .refine(result => result.step.kind === "read_contact_lookup"
    ? result.contact_lookup !== undefined && result.contact_lookup.campaign_id === result.campaign.id
      && result.contact_lookup.expected_revision === result.campaign.revision
      && result.contact_lookup.candidate_id === result.step.lookup.candidate_id
      && result.contact_lookup.method === result.step.lookup.method
      && result.contact_lookup.native_author_id === result.step.lookup.native_author_id
      && result.contact_lookup.call_number === result.campaign.used_calls
      && (result.contact_lookup.status === "completed" ? result.contact_lookup.stop_code === null
        : result.contact_lookup.status === "failed" && result.contact_lookup.stop_code !== null && result.contact_lookup.links.length === 0)
    : result.contact_lookup === undefined)
  .refine(result => result.step.kind === "read_content_targets"
    ? result.content_targets !== undefined && result.content_targets.claim.campaign_id === result.campaign.id
      && result.content_targets.claim.expected_revision === result.campaign.revision
      && result.content_targets.claim.search_hit_id === result.step.search_hit_id
      && result.content_targets.claim.call_number === result.campaign.used_calls
      && (result.content_targets.status === "completed" ? result.content_targets.source_urls !== null && result.content_targets.stop_code === null
        : result.content_targets.source_urls === null && result.content_targets.stop_code !== null)
    : result.content_targets === undefined)
  .refine(result => result.step.kind === "read_identity"
    ? result.identity === null ? committedProviderStop(result) : result.identity.claim.campaign_id === result.campaign.id
      && result.identity.claim.expected_revision === result.campaign.revision
      && result.identity.claim.search_hit_id === result.step.search_hit_id
      && result.identity.claim.call_number === result.campaign.used_calls
    : result.identity === null)
  .refine(result => result.step.kind === "read_content"
    ? result.content === null ? committedProviderStop(result) : result.content.claim.campaign_id === result.campaign.id
      && result.content.claim.expected_revision === result.campaign.revision
      && result.content.claim.search_hit_id === result.step.search_hit_id
      && result.content.claim.call_number === result.campaign.used_calls
    : result.content === null)
  .refine(result => result.step.kind === "read_public_page"
    ? result.public_page === undefined ? committedProviderStop(result) : result.public_page.candidate_id === result.step.candidate_id
      && result.public_page.status !== "reading"
      && result.public_page.profile_ref === result.step.profile_ref
      && result.public_page.requested_url === result.step.requested_url
      && result.public_page.call_number === result.campaign.used_calls
      && result.campaign.scopes.some(scope => scope.profile_ref === result.public_page?.profile_ref)
    : result.public_page === undefined);
export type DiscoveryAdvanceResult = z.infer<typeof advanceSchema>;
export type DiscoveryContentRead = z.infer<typeof contentSchema>;
export const parseDiscoveryContent = (value: unknown): DiscoveryContentRead => contentSchema.parse(value);
export type DiscoveryIdentityRead = z.infer<typeof identitySchema>;
export type DiscoveryPublicPageRead = z.infer<typeof publicPageSchema>;
export const parseDiscoveryPublicPage = (value: unknown) => publicPageSchema.parse(value);
export const parseDiscoveryTranscript = (value: unknown) => transcriptSchema.parse(value);

export const discoveryPublicPageKey = (campaignId: string, candidateId: string, requestedUrl: string) =>
  ["opencli-campaign-public-page", campaignId, candidateId, requestedUrl] as const;

// No runtime invalidation event yet. Only unfinished local receipts are polled,
// for at most twelve observations; missing/failed receipts never auto-retry.
export function publicPageRefreshInterval(status: DiscoveryPublicPageRead["status"] | undefined, observations: number, readFailed: boolean) {
  return !readFailed && status === "reading" && observations < 12 ? 5000 : false;
}

export async function getDiscoveryPublicPage(campaignId: string, candidateId: string, requestedUrl: string): Promise<DiscoveryPublicPageRead> {
  const read = publicPageSchema.parse(await invoke<unknown>("onecrew_discovery_public_page_get", {
    input: { campaign_id: campaignId, candidate_id: candidateId, requested_url: requestedUrl },
  }));
  if (read.candidate_id !== candidateId || read.requested_url !== requestedUrl) throw new Error("discovery_public_page_scope_changed");
  return read;
}

export const discoveryIdentityKey = (campaignId: string, hitId: string) => ["opencli-campaign-identity", campaignId, hitId] as const;

export async function getDiscoveryIdentity(campaignId: string, searchHitId: string): Promise<DiscoveryIdentityRead> {
  const read = identitySchema.parse(await invoke<unknown>("onecrew_discovery_identity_get", {
    input: { campaign_id: campaignId, search_hit_id: searchHitId },
  }));
  if (read.claim.campaign_id !== campaignId || read.claim.search_hit_id !== searchHitId) throw new Error("discovery_identity_mismatch");
  return read;
}

export async function getDiscoveryContent(campaignId: string, searchHitId: string): Promise<DiscoveryContentRead> {
  const read = contentSchema.parse(await invoke<unknown>("onecrew_discovery_content_get", {
    input: { campaign_id: campaignId, search_hit_id: searchHitId },
  }));
  if (read.claim.campaign_id !== campaignId || read.claim.search_hit_id !== searchHitId) throw new Error("discovery_content_scope_changed");
  return read;
}

export async function getDiscoveryProfile(campaignId: string, searchHitId: string) {
  return profileSchema.parse(await invoke<unknown>("onecrew_discovery_profile_get", {
    input: { campaign_id: campaignId, search_hit_id: searchHitId },
  }));
}

export async function getDiscoveryTranscript(campaignId: string, searchHitId: string) {
  return transcriptSchema.parse(await invoke<unknown>("onecrew_discovery_transcript_get", {
    input: { campaign_id: campaignId, search_hit_id: searchHitId },
  }));
}

export async function advanceDiscoveryCampaign(id: string, expectedRevision: number, excludedPlatforms: string[] = []): Promise<DiscoveryAdvanceResult> {
  return advanceSchema.parse(await invoke<unknown>("onecrew_discovery_advance", {
    input: { campaign_id: id, expected_revision: expectedRevision, excluded_platforms: excludedPlatforms },
  }));
}

/** One foreground run; stopping prevents the next call, not completion of an
 * already reserved read. The server, not this loop, chooses every operation. */
export async function runDiscoveryCampaign(
  initial: DiscoveryCampaign,
  signal: AbortSignal,
  onStep: (result: DiscoveryAdvanceResult) => void | Promise<void>,
  excludedPlatforms: string[] | (() => string[]) = [],
): Promise<string> {
  let usedCalls = initial.used_calls;
  while (!signal.aborted) {
    const result = await advanceDiscoveryCampaign(initial.id, initial.revision,
      typeof excludedPlatforms === "function" ? excludedPlatforms() : excludedPlatforms);
    if (result.campaign.id !== initial.id || result.campaign.revision !== initial.revision) {
      throw new Error("discovery_revision_changed");
    }
    await onStep(result);
    if (result.step.kind !== "search" && result.step.kind !== "read_video" && result.step.kind !== "read_transcript" && result.step.kind !== "read_profile" && result.step.kind !== "read_content" && result.step.kind !== "read_content_targets" && result.step.kind !== "read_identity" && result.step.kind !== "read_facebook_identity" && result.step.kind !== "read_source_author" && result.step.kind !== "read_web_source" && result.step.kind !== "read_public_page" && result.step.kind !== "read_contact_lookup") return result.step.kind;
    // A repeated or uncertain response cannot start an unbounded read loop.
    if (result.campaign.used_calls <= usedCalls || result.campaign.max_calls !== initial.max_calls) {
      throw new Error("discovery_progress_unconfirmed");
    }
    usedCalls = result.campaign.used_calls;
  }
  return "paused";
}

export function canSearchDiscovery(campaign: DiscoveryCampaign, profileRef: string, platform = "youtube"): boolean {
  const run = campaign.platform_runs.find(item => item.platform === platform && item.profile_ref === profileRef);
  const scope = campaign.scopes.find(item => item.platform === platform && item.profile_ref === profileRef);
  const firstRoundBlocked = run?.status !== "unsearched" && campaign.platform_runs.some(item => item.status === "unsearched");
  return campaign.phase === "active"
    && (campaign.max_calls === null || campaign.used_calls < campaign.max_calls)
    && !!scope && !!run && !firstRoundBlocked
    && (run.calls_used < scope.keywords.length || run.stop_code === "interrupted_read")
    && (run.status === "unsearched" || run.status === "completed"
      || (run.status === "technical_stop" && run.stop_code === "interrupted_read"));
}

export function canVerifyDiscovery(campaign: DiscoveryCampaign, profileRef: string): boolean {
  const run = campaign.platform_runs.find(item => item.platform === "youtube" && item.profile_ref === profileRef);
  const retryableTechnicalStop = ["interrupted_video_read", "opencli_timeout", "opencli_command_failed", "opencli_invalid_output", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable"];
  return campaign.phase === "active" && (campaign.max_calls === null || campaign.used_calls < campaign.max_calls)
    && !!run && (run.status === "completed"
      || (run.status === "technical_stop" && !!run.stop_code && retryableTechnicalStop.includes(run.stop_code)))
    && campaign.platform_runs.every(item => item.status !== "unsearched");
}

/** Explicit recovery only; ordinary full-source runs never replay an uncertain read. */
export function canRecoverDiscoverySearch(campaign: DiscoveryCampaign, profileRef: string, platform: string): boolean {
  const run = campaign.platform_runs.find(item => item.platform === platform && item.profile_ref === profileRef);
  const scope = campaign.scopes.find(item => item.platform === platform && item.profile_ref === profileRef);
  const repairedCodes = ["provider_endpoint_not_found", "provider_response_invalid_json", "provider_response_shape_invalid", "provider_response_http_error", "provider_source_unavailable", "opencli_command_unavailable", "opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large", "opencli_timeout"];
  return campaign.phase === "active" && (campaign.max_calls === null || campaign.used_calls < campaign.max_calls) && run?.status === "technical_stop" && !!scope
    && !campaign.platform_runs.some(item => item.status === "searching" || item.status === "unsearched")
    && (run.stop_code === "interrupted_read" || typeof run.updated_at === "number" && !!run.stop_code && repairedCodes.includes(run.stop_code) && run.calls_used < scope.keywords.length);
}

export type DiscoveryDraft = {
  id: string;
  brief: string;
  opencli_version: string;
  criteria: { min_subscribers: number | bigint; region: string; language: string; category?: string; requires_ai_tool_demo: boolean; requires_public_tool_url: boolean; pinterest_min_monthly_views_exclusive?: number; contact_preference?: "public_email_first"; requirements?: { description: string; excerpt: string }[] };
  scopes: { platform: string; profile_ref: string; keywords: string[] }[];
  target_count: number;
  max_calls: number | null;
};

export async function createDiscoveryCampaign(input: DiscoveryDraft): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_campaign_create", { input }));
}

export async function getDiscoveryCampaign(id: string): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_campaign_get", { id }));
}

export async function approveDiscoveryCampaign(id: string, expectedRevision: number): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_campaign_approve", { id, expectedRevision }));
}

export type DiscoveryTargetIncrease = {
  campaign_id: string; expected_revision: number; expected_target_count: number;
  new_target_count: number; decision_id: string;
};

export async function increaseDiscoveryTarget(input: DiscoveryTargetIncrease): Promise<DiscoveryCampaign> {
  const result = campaignSchema.parse(await invoke<unknown>("onecrew_discovery_target_increase", { input }));
  if (result.id !== input.campaign_id || result.revision !== input.expected_revision
    || result.target_count !== input.new_target_count) throw new Error("discovery_target_readback_mismatch");
  return result;
}

export async function increaseDiscoveryBudget(input: {
  campaign_id: string; expected_revision: number; expected_max_calls: number | null;
  new_max_calls: number | null; decision_id: string;
}): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_budget_increase", { input }));
}

export async function searchDiscoveryYoutubeOnce(id: string, expectedRevision: number, profileRef: string): Promise<DiscoveryCampaign> {
  const result = await invoke<unknown>("onecrew_discovery_youtube_search_once", { id, expectedRevision, profileRef, limit: 10 });
  return campaignSchema.parse(z.object({ campaign: z.unknown(), returned_hits: z.number() }).parse(result).campaign);
}

export async function searchDiscoveryOnce(id: string, expectedRevision: number, platform: string, profileRef: string, recovery?: { expected_stop_code: string; expected_stopped_at: number }): Promise<DiscoveryCampaign> {
  const result = await invoke<unknown>("onecrew_discovery_search_once", { id, expectedRevision, platform, profileRef, limit: 10, ...(recovery ? { recovery } : {}) });
  const campaign = campaignSchema.parse(z.object({ campaign: z.unknown(), returned_hits: z.number() }).parse(result).campaign);
  if (campaign.id !== id || campaign.revision !== expectedRevision) throw new Error("discovery_revision_changed");
  return campaign;
}

export async function verifyDiscoveryYoutubeHitOnce(id: string, expectedRevision: number, profileRef: string, searchHitId: string): Promise<DiscoveryCampaign> {
  return campaignSchema.parse(await invoke<unknown>("onecrew_discovery_youtube_verify_hit_once", {
    id, expectedRevision, profileRef, searchHitId,
  }));
}

export const DISCOVERY_PAGE_SIZE = 100;

/** Local keyset pagination only; loading another page never calls a provider. */
export function nextDiscoveryPage(page: readonly { id: string }[]): string | undefined {
  return page.length === DISCOVERY_PAGE_SIZE ? page.at(-1)?.id : undefined;
}

export async function listDiscoveryHits(campaignId: string, afterId: string | null = null): Promise<DiscoveryHit[]> {
  return z.array(hitSchema).parse(await invoke<unknown>("onecrew_discovery_search_hits_list", {
    input: { campaign_id: campaignId, after_id: afterId, limit: DISCOVERY_PAGE_SIZE },
  }));
}

export async function listDiscoveryCandidates(campaignId: string, afterId: string | null = null): Promise<DiscoveryCandidate[]> {
  return z.array(candidateSchema).parse(await invoke<unknown>("onecrew_discovery_candidates_list", {
    input: { campaign_id: campaignId, after_id: afterId, limit: DISCOVERY_PAGE_SIZE },
  }));
}

export async function countDiscoveryCandidates(campaignId: string): Promise<DiscoveryCandidateCounts> {
  return countsSchema.parse(await invoke<unknown>("onecrew_discovery_candidates_count", { id: campaignId }));
}
