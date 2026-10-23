// DM inbox 的 typed Tauri 客户端与查询键(UI-05/08):列表是纯本地读,
// 单线程刷新仍是 MessageWorkspace 里的显式 sync。

import { creatorInvoke, type CreatorPlatform, type ReplyTriageBrief } from "@/lib/creator";
import type { Platform } from "@/types";

export type ConversationListStatus = "waiting_reply" | "replied";

export type ConversationListFilters = {
  query: string;
  status: "all" | ConversationListStatus;
  channel: "all" | Platform;
};

export type ListConversationsInput = {
  limit?: number;
  cursor?: string;
  query?: string | null;
  status?: ConversationListStatus | null;
  channel?: Platform | null;
};

export type ConversationLastMessage = {
  direction: string;
  preview: string;
  sent_at: number;
};

export type ConversationSummary = {
  conversation_id: string;
  creator_id: string;
  creator_display_name: string;
  /** Creator 的展示头像(首次收录账号优先,缺失则顺延)——同一个人
   *  在各处是同一张脸;peer_handle 才是这条线程的对端 handle。 */
  creator_avatar_url?: string | null;
  peer_handle?: string | null;
  channel: CreatorPlatform | "email" | "website" | "link_hub";
  operator_account_ref_id: string;
  operator_account_label: string;
  contact_route_id: string;
  resolved_target_id: string;
  provider_thread_id: string;
  last_message?: ConversationLastMessage | null;
  /** Agent 对本会话最新一条来信的判读,没判读过就没有。 */
  triage?: ReplyTriageBrief | null;
  updated_at: number;
};

export type ConversationPage = {
  items: ConversationSummary[];
  next_cursor?: string | null;
};

export const CONVERSATION_LIST_QUERY_KEY = ["creator-conversation-list"] as const;

export function conversationListInput(
  filters: ConversationListFilters,
  limit = 100,
): ListConversationsInput {
  return {
    limit,
    query: filters.query.trim() || null,
    status: filters.status === "all" ? null : filters.status,
    channel: filters.channel === "all" ? null : filters.channel,
  };
}

export function conversationListQueryKey(filters: ConversationListFilters) {
  const input = conversationListInput(filters);
  return [...CONVERSATION_LIST_QUERY_KEY, input] as const;
}

export function listConversations(input?: ListConversationsInput): Promise<ConversationPage> {
  return creatorInvoke("onecrew_conversation_list", { input: input ?? {} });
}
