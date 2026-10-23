// 私聊线程的展示归约(纯函数,node 环境可单测):把按时间升序的消息
// 折成渲染项 —— 跨天插日期分隔,同向且相邻的消息成组(组内收紧间距、
// 只有组末带时间与头像),这是聊天界面的通行读法。

import type { Message } from "@/lib/creator";

/** 同一组的时间跨度上限:超过就另起一组,免得半天前后的两条粘在一起。 */
const GROUP_WINDOW_SECONDS = 5 * 60;

export type ThreadItem =
  | { kind: "date"; id: string; at: number }
  | {
      kind: "message";
      id: string;
      message: Message;
      outgoing: boolean;
      /** 组首:与上一条之间留出间距。 */
      startsGroup: boolean;
      /** 组末:显示时间;入站消息在这一条旁边显示头像。 */
      endsGroup: boolean;
    };

/** 本地时区的日序号,用来判断跨天。 */
function dayIndex(timestampSeconds: number): number {
  const date = new Date(timestampSeconds * 1000);
  return Math.floor(
    new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime() / 86_400_000,
  );
}

export function isOutgoing(message: Message): boolean {
  return message.direction === "outbound" || message.direction === "sent";
}

export function buildThreadItems(messages: readonly Message[]): ThreadItem[] {
  const items: ThreadItem[] = [];
  messages.forEach((message, index) => {
    const previous = index > 0 ? messages[index - 1] : null;
    const next = index + 1 < messages.length ? messages[index + 1] : null;
    const outgoing = isOutgoing(message);

    if (!previous || dayIndex(previous.sent_at) !== dayIndex(message.sent_at)) {
      items.push({ kind: "date", id: `date-${message.id}`, at: message.sent_at });
    }

    const continuesPrevious =
      previous != null &&
      isOutgoing(previous) === outgoing &&
      dayIndex(previous.sent_at) === dayIndex(message.sent_at) &&
      message.sent_at - previous.sent_at <= GROUP_WINDOW_SECONDS;
    const continuesIntoNext =
      next != null &&
      isOutgoing(next) === outgoing &&
      dayIndex(next.sent_at) === dayIndex(message.sent_at) &&
      next.sent_at - message.sent_at <= GROUP_WINDOW_SECONDS;

    items.push({
      kind: "message",
      id: message.id,
      message,
      outgoing,
      startsGroup: !continuesPrevious,
      endsGroup: !continuesIntoNext,
    });
  });
  return items;
}
