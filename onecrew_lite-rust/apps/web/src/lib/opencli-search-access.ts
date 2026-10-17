/**
 * Reviewed OneCrew search paths for OpenCLI 1.8.8 (2026-09-30).
 * This describes search requirements, not a live login/success assertion.
 * See docs/development/opencli-search-access-2026-09-30.md for evidence.
 * COOKIE/browser:true alone is never treated as a login requirement.
 */
export type OpenCliSearchAccess = "login_required" | "extra_entitlement" | "public_browser" | "public_read" | "conditional";

const requiresLogin = new Set([
  "bilibili", "instagram", "twitter", "linkedin", "facebook", "pinterest", "pinterest-pins", "pinterest-boards",
  "youtube", "douyin", "weibo", "jike", "linux-do",
  "linkedin-learning", "douban-books", "douban-music",
]);
const publicBrowser = new Set([
  "weixin", "hupu", "google", "google-images", "duckduckgo", "yahoo",
  "dribbble-shots", "dribbble-designers",
]);

export function openCliSearchAccess(platform: string, browserRequired: boolean): OpenCliSearchAccess {
  if (!browserRequired) return "public_read";
  if (platform === "linkedin-salesnav") return "extra_entitlement";
  if (requiresLogin.has(platform)) return "login_required";
  if (publicBrowser.has(platform)) return "public_browser";
  // TikTok, Reddit (search/subreddit), Medium and Nowcoder have conditional
  // session errors; Tieba reads page results. No signed-out live proof yet.
  return "conditional";
}

export function openCliSearchNeedsLoginCheck(platform: string, browserRequired: boolean): boolean {
  const access = openCliSearchAccess(platform, browserRequired);
  return access !== "public_read" && access !== "public_browser";
}
