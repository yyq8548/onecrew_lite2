import { describe, expect, it } from "vitest";

import { buildThreadItems, isOutgoing } from "./chat-thread";
import type { Message } from "./creator";

function message(id: string, direction: string, sentAt: number): Message {
  return {
    id,
    conversation_id: "c1",
    direction,
    body: id,
    status: "delivered",
    public_data: {},
    sent_at: sentAt,
  };
}

const NOON = Math.floor(new Date(2026, 7, 19, 12, 0, 0).getTime() / 1000);
const DAY = 86_400;

describe("thread items", () => {
  it("reads both outbound spellings as ours", () => {
    expect(isOutgoing(message("a", "outbound", NOON))).toBe(true);
    expect(isOutgoing(message("b", "sent", NOON))).toBe(true);
    expect(isOutgoing(message("c", "inbound", NOON))).toBe(false);
  });

  it("opens each day with a separator", () => {
    const items = buildThreadItems([
      message("a", "inbound", NOON),
      message("b", "inbound", NOON + DAY),
    ]);
    expect(items.map((item) => item.kind)).toEqual(["date", "message", "date", "message"]);
  });

  it("groups consecutive same-direction messages inside the window", () => {
    const items = buildThreadItems([
      message("a", "inbound", NOON),
      message("b", "inbound", NOON + 60),
      message("c", "outbound", NOON + 120),
    ]);
    const messages = items.filter((item) => item.kind === "message");
    // a 开组不收尾,b 收尾(时间/头像挂 b),c 另起一组(换了方向)。
    expect(messages.map((item) => [item.id, item.startsGroup, item.endsGroup])).toEqual([
      ["a", true, false],
      ["b", false, true],
      ["c", true, true],
    ]);
  });

  it("breaks a group when the gap exceeds the window or the day changes", () => {
    const items = buildThreadItems([
      message("a", "inbound", NOON),
      message("b", "inbound", NOON + 6 * 60),
    ]);
    const messages = items.filter((item) => item.kind === "message");
    expect(messages.every((item) => item.startsGroup && item.endsGroup)).toBe(true);
  });
});
