// 收件箱右栏打开的是哪条线程,以及它和列表里的行怎么对上。
//
// 一条线程有两种身份:挂载键 (`key`,决定 MessageWorkspace 什么时候重挂载)
// 和会话 ID(决定列表里哪一行是当前打开的)。两者必须分开:手动发起的线程
// 一开始没有会话 ID,首次同步或首次发送才把会话行建出来,而那一刻不能让
// 工作区重挂载 —— 否则刚发完就重跑一次自动同步。

import type { ConversationSummary } from "@/lib/conversations";

/** 右栏当前打开的线程:既有会话行,或手动发起的草稿线程。 */
export type OpenThread = {
  /** MessageWorkspace 的挂载键。认下会话 ID 时不会变。 */
  key: string;
  creatorId: string;
  contactRouteId: string;
  operatorId: string;
  accountLabel: string;
  /** 已有会话行的 ID;「新建对话」还没有会话,首次同步或首次发送后才有。 */
  conversationId: string | null;
};

/** 打开列表里的一行。 */
export function openThreadForConversation(summary: ConversationSummary): OpenThread {
  return {
    key: `${summary.conversation_id}:${summary.operator_account_ref_id}`,
    creatorId: summary.creator_id,
    contactRouteId: summary.contact_route_id,
    operatorId: summary.operator_account_ref_id,
    accountLabel: summary.operator_account_label,
    conversationId: summary.conversation_id,
  };
}

/** 手动向一个 Creator 的建联通道发起线程,此时还没有会话。 */
export function openThreadForDraft(input: {
  creatorId: string;
  contactRouteId: string;
  operatorId: string;
  accountLabel: string;
}): OpenThread {
  return {
    key: `draft:${input.creatorId}:${input.contactRouteId}:${input.operatorId}`,
    ...input,
    conversationId: null,
  };
}

/** 认下工作区刚学到的会话 ID。挂载键不动,所以工作区不重挂载;已经是这个
 * ID 就原样返回,免得多一次渲染。 */
export function threadAfterConversationOpened(
  current: OpenThread | null,
  conversationId: string,
): OpenThread | null {
  if (!current || current.conversationId === conversationId) return current;
  return { ...current, conversationId };
}

/** 列表里的这一行是否就是当前打开的线程。按会话 ID 加操作账号判定 —— 同一
 * 个会话换账号发送是另一条线程,而草稿线程还没有会话 ID,任何行都不匹配。 */
export function conversationRowIsOpen(
  thread: OpenThread | null,
  row: Pick<ConversationSummary, "conversation_id" | "operator_account_ref_id">,
): boolean {
  return thread?.conversationId === row.conversation_id
    && thread.operatorId === row.operator_account_ref_id;
}
