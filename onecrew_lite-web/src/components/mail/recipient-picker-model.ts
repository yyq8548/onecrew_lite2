/**
 * 收件人选择器的纯状态派生（UI-04）：邮箱判断、Creator 候选筛选、
 * Creator 邮箱联系方式展开，以及建议面板的显示状态。
 */
import { emailContactRoutes, type CreatorRecord, type CreatorSummary } from "@/lib/creator";

/** 简单判断 a@b.c：只用来决定「使用邮箱 …」这一固定项是否出现，不做严格校验（真正的校验在 runtime）。 */
export function looksLikeEmail(text: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(text.trim());
}

/** 只有带邮箱联系方式的 Creator 才能当收件人：其余在建议列表里直接隐藏。 */
export function recipientCreatorCandidates(creators: readonly CreatorSummary[]): CreatorSummary[] {
  return creators.filter((creator) => creator.channels.includes("email"));
}

export type CreatorEmailChoice = { contactRouteId: string; email: string };

/** Creator 详情里的邮箱联系方式：route id 与 locator 的规范化邮箱一一对应。 */
export function creatorEmailChoices(record: CreatorRecord | null | undefined): CreatorEmailChoice[] {
  return emailContactRoutes(record).map((route) => ({
    contactRouteId: route.route.id,
    email: route.locator.normalized_value,
  }));
}

export type RecipientSelection = {
  creatorId: string;
  contactRouteId: string;
  to: string;
  displayName: string;
};

/** 建议面板最多显示的 Creator 条数：更多的靠继续输入缩小范围。 */
export const RECIPIENT_SUGGESTION_LIMIT = 8;

export type RecipientSuggestionState =
  | { kind: "loading" }
  | { kind: "error"; message: string | null }
  | { kind: "empty" }
  | { kind: "ready"; creators: CreatorSummary[] };

/**
 * 建议面板的显示状态：防抖尚未追上输入或查询未返回时为加载中；查询失败显示错误；
 * 否则按候选（含邮箱渠道、最多 RECIPIENT_SUGGESTION_LIMIT 条）区分空态与列表。
 */
export function recipientSuggestionState(input: {
  query: string;
  debouncedQuery: string;
  loading: boolean;
  error: unknown;
  creators: readonly CreatorSummary[] | undefined;
}): RecipientSuggestionState {
  if (input.query !== input.debouncedQuery || input.loading) return { kind: "loading" };
  if (input.error) {
    const message = input.error instanceof Error
      ? input.error.message
      : typeof input.error === "string" ? input.error : "";
    return { kind: "error", message: message || null };
  }
  const creators = recipientCreatorCandidates(input.creators ?? []).slice(0, RECIPIENT_SUGGESTION_LIMIT);
  if (creators.length === 0) return { kind: "empty" };
  return { kind: "ready", creators };
}
