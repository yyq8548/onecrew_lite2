/** Dated field evidence, separate from the live command and browser checks. */
export const OPENCLI_EVIDENCE_DATE = "2026-09-28";
export const OPENCLI_EVIDENCE_VERSION = "1.8.8";

const passed = new Set([
  "youtube", "tiktok", "instagram", "twitter", "linkedin", "facebook", "pinterest", "reddit",
  "bilibili", "weixin", "douyin", "medium", "substack", "apple-podcasts", "tieba",
  "bluesky", "jike", "hackernews", "nowcoder", "hupu", "stackoverflow",
  "pinterest-pins", "pinterest-boards", "linkedin-learning", "archive", "weread", "brave",
  "duckduckgo", "yahoo", "dribbble-shots", "dribbble-designers", "google-images",
  "stackoverflow-users", "v2ex-node", "reddit-subreddit", "devto-tag", "lesswrong-tag",
  "lobsters-tag", "medium-tag", "stackoverflow-tag", "juejin-hot", "producthunt-posts",
  "hf-spaces", "hf-models", "hf-datasets",
]);

const stopped = {
  xiaohongshu: "navigation",
  rednote: "login",
  weibo: "login",
  "linux-do": "login",
  sinablog: "no_creator_source",
  "1point3acres": "login_403",
  "linkedin-salesnav": "entitlement",
  google: "opaque_redirect",
  "douban-books": "login",
  "douban-music": "login_invalid_output",
} as const;

export type OpenCliEvidenceReason = (typeof stopped)[keyof typeof stopped];
export type OpenCliLiveEvidence =
  | { status: "passed"; sampleCount: number }
  | { status: "stopped"; reason: OpenCliEvidenceReason }
  | { status: "not_tested" };

export function openCliLiveEvidence(platform: string): OpenCliLiveEvidence {
  if (passed.has(platform)) return { status: "passed", sampleCount: platform === "google-images" ? 1 : 5 };
  if (Object.prototype.hasOwnProperty.call(stopped, platform)) {
    return { status: "stopped", reason: stopped[platform as keyof typeof stopped] };
  }
  return { status: "not_tested" };
}

export const OPENCLI_EVIDENCE_TOTALS = { passed: passed.size, stopped: Object.keys(stopped).length } as const;

/** Observations about access at the dated field test, never a live sign-in state. */
export function openCliHistoricalAccess(platform: string, browserRequired: boolean):
  "login_wall" | "extra_entitlement" | "login_recovered" | "public_read" | "unknown" {
  if (platform === "linkedin-salesnav") return "extra_entitlement";
  if (["rednote", "weibo", "linux-do", "1point3acres", "douban-books", "douban-music"].includes(platform)) return "login_wall";
  if (platform === "instagram") return "login_recovered";
  return browserRequired ? "unknown" : "public_read";
}
