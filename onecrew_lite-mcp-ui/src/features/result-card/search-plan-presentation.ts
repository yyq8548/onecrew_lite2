import type { SearchPlanEditorDraft } from "@mcp-ui/lib/result-contract";
import { translate, type AppLocale, type CopyKey } from "@mcp-ui/lib/i18n";

export type SearchPlanScope = SearchPlanEditorDraft["scopes"][number];
export type ScopeCriterion = { label: string; value: string };
export type SearchScopePreview = {
  toolkit: string;
  kind: string;
  summary: string;
  criteria: ScopeCriterion[];
  details: string;
};

// A read-only, business-language rendering of the operation-bound public
// scope. It labels known search criteria for display only: it never compiles
// a query, validates or edits arguments, or chooses a provider action. The
// operation id, connection identifiers, and raw argument keys stay off the
// card; an unknown operation or key still gets a readable generic label.
const SEARCH_KINDS: Record<string, CopyKey> = {
  "x.posts.search.advanced": "searchKindXPosts",
  "youtube.channels.search": "searchKindYoutubeChannels",
  "youtube.videos.search": "searchKindYoutubeVideos",
  "instagram.users.search": "searchKindInstagramUsers",
  "telegram.communities.search": "searchKindTelegramCommunities",
};

// Display order and label of each known criterion key.
const CRITERIA: [string, CopyKey][] = [
  ["query", "searchCriteriaQuery"], ["keyword_search", "searchCriteriaKeywords"],
  ["all_words", "searchCriteriaAllWords"], ["exact_phrase", "searchCriteriaExactPhrase"], ["any_words", "searchCriteriaAnyWords"],
  ["exclude_words", "searchCriteriaExcludeWords"], ["hashtags", "searchCriteriaHashtags"],
  ["from_accounts", "searchCriteriaFromAccounts"], ["to_accounts", "searchCriteriaToAccounts"], ["mentioning_accounts", "searchCriteriaMentioningAccounts"],
  ["country_codes", "searchCriteriaCreatorRegions"], ["state_provinces", "searchCriteriaStateProvinces"],
  ["language", "searchCriteriaLanguage"], ["languages", "searchCriteriaCreatorLanguages"],
  ["since", "searchCriteriaSince"], ["until", "searchCriteriaUntil"],
  ["min_followers", "searchCriteriaFollowers"], ["max_followers", "searchCriteriaFollowers"],
  ["min_avg_views", "searchCriteriaAverageViews"], ["max_avg_views", "searchCriteriaAverageViews"],
  ["min_median_views", "searchCriteriaMedianViews"], ["max_median_views", "searchCriteriaMedianViews"],
  ["min_engagement_rate", "searchCriteriaEngagementRate"], ["max_engagement_rate", "searchCriteriaEngagementRate"],
  ["min_creator_price", "searchCriteriaCreatorPrice"], ["max_creator_price", "searchCriteriaCreatorPrice"], ["creator_price_currency", "searchCriteriaPriceCurrency"],
  ["content_label_ids", "searchCriteriaContentLabels"], ["industry_label_ids", "searchCriteriaIndustryLabels"],
  ["follower_country_codes", "searchCriteriaFollowerRegions"], ["follower_gender_ratio", "searchCriteriaFollowerGender"], ["follower_age", "searchCriteriaFollowerAge"],
  ["min_likes", "searchCriteriaLikes"], ["min_replies", "searchCriteriaReplyCount"], ["min_reposts", "searchCriteriaReposts"],
  ["replies", "searchCriteriaReplies"], ["links", "searchCriteriaLinks"], ["result_mode", "searchCriteriaResultMode"],
  ["sort_field", "searchCriteriaSortField"], ["sort_order", "searchCriteriaSortOrder"],
  ["count", "searchCriteriaCount"], ["page_size", "searchCriteriaCount"],
];
const CRITERIA_LABELS = new Map(CRITERIA);
const CRITERIA_ORDER = new Map(CRITERIA.map(([key], index) => [key, index]));
// The main criteria shown on the one-line scope summary.
const SUMMARY_KEYS = new Set(["query", "keyword_search", "all_words", "exact_phrase", "any_words", "hashtags", "country_codes", "language", "languages", "min_followers", "since"]);
// Execution controls and opaque identifiers are not search criteria.
const HIDDEN_KEYS = new Set(["search_plan_policy", "confirmed_plan_ref", "cursor", "page", "request_id", "connection_id", "account_id"]);
// A wrapper object whose fields are the criteria themselves.
const WRAPPER_KEYS = new Set(["request"]);
const REGION_KEYS = new Set(["country_codes", "follower_country_codes"]);
const LANGUAGE_KEYS = new Set(["language", "languages"]);
const DATE_KEYS = new Set(["since", "until"]);
const COUNT_KEYS = new Set(["content_label_ids", "industry_label_ids"]);
const ENUM_VALUES: Record<string, Record<string, CopyKey>> = {
  replies: { any: "searchCriteriaAny", include: "searchCriteriaInclude", exclude: "searchCriteriaExclude" },
  links: { any: "searchCriteriaAny", include: "searchCriteriaInclude", exclude: "searchCriteriaExclude" },
  result_mode: { latest: "searchCriteriaLatest", top: "searchCriteriaTop" },
  sort_field: { RELEVANCE: "searchCriteriaRelevance", FOLLOWERS: "searchCriteriaFollowers", MEDIAN_VIEWS: "searchCriteriaMedianViews", ENGAGEMENT_RATE: "searchCriteriaEngagementRate" },
  sort_order: { ASC: "searchCriteriaAscending", DESC: "searchCriteriaDescending" },
};

type Entry = [string, unknown];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function ordered(entries: Entry[]): Entry[] {
  const rank = (key: string) => CRITERIA_ORDER.get(key) ?? CRITERIA.length;
  return entries.map((entry, index) => ({ entry, index })).sort((a, b) => rank(a.entry[0]) - rank(b.entry[0]) || a.index - b.index).map(({ entry }) => entry);
}

function displayName(locale: AppLocale, type: "language" | "region", code: string): string {
  try { return new Intl.DisplayNames([locale], { type }).of(code) ?? code; } catch { return code; }
}

function scalar(locale: AppLocale, key: string, value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "number") {
    const number = new Intl.NumberFormat(locale).format(value);
    return key.startsWith("min_") ? `≥ ${number}` : key.startsWith("max_") ? `≤ ${number}` : number;
  }
  if (typeof value !== "string") return String(value);
  const text = value.trim();
  if (!text) return null;
  const enumKey = ENUM_VALUES[key]?.[text];
  if (enumKey) return translate(locale, enumKey);
  if (REGION_KEYS.has(key)) return displayName(locale, "region", text);
  if (LANGUAGE_KEYS.has(key)) return displayName(locale, "language", text);
  const date = DATE_KEYS.has(key) ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(text) : null;
  if (date) return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeZone: "UTC" }).format(Date.UTC(Number(date[1]), Number(date[2]) - 1, Number(date[3])));
  const gender = key === "follower_gender_ratio" ? /^(FEMALE|MALE)_(\d+)$/.exec(text) : null;
  if (gender) return `${translate(locale, gender[1] === "FEMALE" ? "searchCriteriaFemale" : "searchCriteriaMale")} ≥ ${gender[2]}%`;
  return text;
}

function criterionLines(locale: AppLocale, key: string, value: unknown, parent: string | null): ScopeCriterion[] {
  if (HIDDEN_KEYS.has(key)) return [];
  const known = CRITERIA_LABELS.get(key);
  const own = known ? translate(locale, known) : key.replace(/_/g, " ");
  const label = parent ? `${parent} · ${own}` : own;
  if (isRecord(value)) return entryLines(locale, Object.entries(value), WRAPPER_KEYS.has(key) ? parent : label);
  if (Array.isArray(value)) {
    if (value.some(isRecord)) return value.flatMap((item, index) => criterionLines(locale, key, item, parent).map((line) => ({ ...line, label: `${line.label} ${index + 1}` })));
    const items = value.map((item) => scalar(locale, key, item)).filter((item): item is string => item !== null);
    if (!items.length) return [];
    return [{ label, value: COUNT_KEYS.has(key) ? translate(locale, "searchCriteriaSelectedCount", { count: items.length }) : items.join(translate(locale, "listSeparator")) }];
  }
  const text = scalar(locale, key, value);
  return text === null ? [] : [{ label, value: text }];
}

function entryLines(locale: AppLocale, entries: Entry[], parent: string | null): ScopeCriterion[] {
  return ordered(entries).flatMap(([key, value]) => criterionLines(locale, key, value, parent));
}

function summaryLines(locale: AppLocale, args: Record<string, unknown>): ScopeCriterion[] {
  const flat = Object.entries(args).flatMap(([key, value]): Entry[] => WRAPPER_KEYS.has(key) && isRecord(value) ? Object.entries(value) : [[key, value]]);
  return entryLines(locale, flat.filter(([key]) => SUMMARY_KEYS.has(key)), null);
}

export function searchScopeKind(locale: AppLocale, operationId: string): string {
  return translate(locale, SEARCH_KINDS[operationId] ?? "searchKindGeneric");
}

export function scopeCriteria(locale: AppLocale, args: SearchPlanScope["arguments"]): ScopeCriterion[] {
  return entryLines(locale, Object.entries(args), null);
}

export function searchScopePreviews(draft: SearchPlanEditorDraft, locale: AppLocale): SearchScopePreview[] {
  const line = (criterion: ScopeCriterion) => translate(locale, "searchCriterion", criterion);
  return draft.scopes.map((scope): SearchScopePreview => {
    const criteria = scopeCriteria(locale, scope.arguments);
    return {
      toolkit: scope.connection.toolkit,
      kind: searchScopeKind(locale, scope.operation_id),
      summary: summaryLines(locale, scope.arguments).map(line).join(" · "),
      criteria,
      details: criteria.map(line).join("\n"),
    };
  });
}
