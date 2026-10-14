import type { ResultSnapshot, SearchPlanEditorDraft } from "@mcp-ui/lib/result-contract";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

export type SearchPlanScope = SearchPlanEditorDraft["scopes"][number];

// Operation-bound scopes with each tagged connection kind. Arguments are the
// operation-owned input the registry validated; the App treats them as data.
export const INSTAGRAM_SCOPE: SearchPlanScope = {
  operation_id: "instagram.users.search",
  schema_revision: "1".repeat(64),
  connection: { kind: "operator_account", toolkit: "instagram", operator_account_id: "operator-demo" },
  arguments: { query: "cooking", count: 20 },
};
export const YOUTUBE_SCOPE: SearchPlanScope = {
  operation_id: "youtube.channels.search",
  schema_revision: "2".repeat(64),
  connection: { kind: "service_credential", toolkit: "youtube", credential_id: "credential-demo" },
  arguments: { query: "cooking", count: 20 },
};
export const X_SCOPE: SearchPlanScope = {
  operation_id: "x.posts.search.advanced",
  schema_revision: "3".repeat(64),
  connection: { kind: "operator_account", toolkit: "x", operator_account_id: "operator-demo" },
  arguments: { any_words: ["cooking"], count: 20 },
};

// The x.posts.search.advanced scope of a live Codex prepare call, verbatim
// except for a synthetic operator account id.
export const X_TRANSCRIPT_SCOPE: SearchPlanScope = {
  operation_id: "x.posts.search.advanced",
  schema_revision: "sha256:3141718064de652188f95f21bba512d9770b303314dfc821909abaa158a4557b",
  connection: { kind: "operator_account", operator_account_id: "11111111-2222-4333-8444-555555555555", toolkit: "x" },
  arguments: {
    any_words: ["RWA", "tokenization", "real-world assets"], all_words: ["crypto"], language: "tr", since: "2026-08-01", until: null,
    result_mode: "latest", replies: "exclude", links: "any", count: 100, exact_phrase: null, exclude_words: ["giveaway", "airdrop"],
    from_accounts: [], mentioning_accounts: [], to_accounts: [], min_likes: null, min_replies: null, min_reposts: null, hashtags: [],
  },
};
export const X_TRANSCRIPT_PLAN: SearchPlanEditorDraft = {
  description: "在 X 上发现并核验 5 个位于土耳其或明确面向土耳其市场、讨论 RWA（real-world assets）与 crypto 的账号。硬性标准：近期活跃（从 2026-08-01 起有公开帖子），内容为原创且有实质信息/分析/行业讨论，排除纯转发、重复推广、抽奖、无实质水文；先用帖子发现作者，再读取每个候选的公开资料与近期帖子核验。仅做只读发现，不保存到 OneCrew CRM、不联系。",
  keywords: ["Turkey", "Türkiye", "RWA", "real-world assets", "crypto", "blockchain", "tokenization", "原创", "非水文"],
  scopes: [X_TRANSCRIPT_SCOPE],
};
// The two YouTube scopes of a live prepare call, with a synthetic credential id.
const YOUTUBE_TRANSCRIPT_QUERY = "Turkey Türkiye Turkish Türkçe RWA crypto real world assets tokenization";
const YOUTUBE_CREDENTIAL = { kind: "service_credential", toolkit: "youtube", credential_id: "66666666-7777-4888-8999-000000000000" } as const;
export const YOUTUBE_TRANSCRIPT_PLAN: SearchPlanEditorDraft = {
  description: "在 YouTube 发现并筛选恰好 5 个与土耳其区相关、围绕 RWA crypto/现实世界资产代币化持续创作的频道。硬性条件：近期有公开发布（优先近 90 天），内容需是原创且有实质分析、访谈、研究、教育或项目解读；排除纯价格喊单、重复转载、抽奖、泛泛营销和明显水文。只做公开资料发现与结果展示，不保存到 CRM，不联系创作者。",
  keywords: ["YouTube", "Turkey", "Türkiye", "Turkish", "Türkçe", "RWA", "real world assets", "tokenization", "crypto", "recent activity", "original substantive content", "exclude spam", "exactly 5"],
  scopes: [
    { operation_id: "youtube.channels.search", schema_revision: "sha256:6fe9bbfb7d80a29c1ed57464287d27de1a45ae909199f9e61a662c295c0fdf17", connection: YOUTUBE_CREDENTIAL, arguments: { query: YOUTUBE_TRANSCRIPT_QUERY, count: 20 } },
    { operation_id: "youtube.videos.search", schema_revision: "sha256:6fe9bbfb7d80a29c1ed57464287d27de1a45ae909199f9e61a662c295c0fdf17", connection: YOUTUBE_CREDENTIAL, arguments: { query: YOUTUBE_TRANSCRIPT_QUERY, count: 30 } },
  ],
};

export const SEARCH_PLAN: SearchPlanEditorDraft = {
  description: "Find US cooking creators with ≥ 1,000 followers.",
  keywords: ["cooking"],
  scopes: [INSTAGRAM_SCOPE],
};

export function searchPlanSnapshot(draft: SearchPlanEditorDraft = SEARCH_PLAN, revision = 2): ResultSnapshot {
  return {
    ...SNAPSHOT,
    binding: { ...SNAPSHOT.binding, revision },
    batch: { outcomes: [{ call_id: "call.search-plan", operation_id: "onecrew.search.plan.prepare", disposition: { kind: "not_run", reason: "search_plan_confirmation_required" }, summary: "Review search plan", resource_refs: [] }] },
    presentation: { kind: "search_plan_editor", title: "Review search plan", summary: "Edit acceptance criteria and keywords, then confirm before searching.", sections: [{ kind: "fields", heading: "Search plan", fields: [{ label: "Acceptance criteria", value: draft.description }] }] },
    pending_action: { kind: "search_plan", draft, draft_digest: `sha256:${revision.toString(16).padStart(64, "0")}` },
  };
}
