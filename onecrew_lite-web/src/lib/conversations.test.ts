import { describe, expect, it } from "vitest";

import {
  CONVERSATION_LIST_QUERY_KEY,
  conversationListInput,
  conversationListQueryKey,
} from "@/lib/conversations";

describe("conversationListInput", () => {
  it("normalizes the unfiltered list into explicit nullable criteria", () => {
    expect(conversationListInput({ query: "  ", status: "all", channel: "all" })).toEqual({
      limit: 100,
      query: null,
      status: null,
      channel: null,
    });
  });

  it("keeps trimmed search, reply state, and channel in the query family key", () => {
    const filters = {
      query: "  @creator  ",
      status: "waiting_reply" as const,
      channel: "telegram" as const,
    };
    const input = conversationListInput(filters, 25);
    expect(input).toEqual({
      limit: 25,
      query: "@creator",
      status: "waiting_reply",
      channel: "telegram",
    });
    expect(conversationListQueryKey(filters)).toEqual([
      ...CONVERSATION_LIST_QUERY_KEY,
      { ...input, limit: 100 },
    ]);
  });
});
