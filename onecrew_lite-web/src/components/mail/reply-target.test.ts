import { describe, expect, it } from "vitest";

import { resolveEmailReplyTarget } from "@/components/mail/reply-target";
import type { EmailMessageView } from "@/lib/creator";

function message(
  id: string,
  direction: EmailMessageView["direction"],
  occurredAt: number,
  internetMessageId: string | null,
): EmailMessageView {
  return {
    id,
    thread_id: "thread-1",
    provider_message_id: `provider-${id}`,
    internet_message_id: internetMessageId,
    direction,
    from: "creator@example.com",
    to: ["me@example.com"],
    cc: [],
    reply_to: [],
    references: [],
    subject: "Hello",
    text: "Body",
    html: null,
    delivery_status: "delivered",
    occurred_at: occurredAt,
  };
}

describe("email reply target", () => {
  it("keeps the existing Creator route reply ahead of the manual fallback", () => {
    const displayed = message("outbound", "outbound", 30, "<outbound@example.com>");
    const result = resolveEmailReplyTarget({
      creatorTarget: { creatorId: "creator-1", contactRouteId: "route-1", to: "creator@example.com" },
      displayedMessage: displayed,
      threadMessages: [message("inbound", "inbound", 20, "<inbound@example.com>")],
      allowManualThreadReply: true,
    });
    expect(result).toEqual({
      creatorId: "creator-1",
      contactRouteId: "route-1",
      to: "creator@example.com",
      message: displayed,
    });
  });

  it("uses the latest inbound Message-ID for an unowned manual thread and leaves to empty", () => {
    const latestInbound = message("latest-inbound", "inbound", 40, "<latest@example.com>");
    const result = resolveEmailReplyTarget({
      creatorTarget: null,
      displayedMessage: message("outbound", "outbound", 50, "<outbound@example.com>"),
      threadMessages: [
        message("old-inbound", "inbound", 10, "<old@example.com>"),
        message("missing-message-id", "inbound", 60, null),
        latestInbound,
      ],
      allowManualThreadReply: true,
    });
    expect(result).toEqual({
      creatorId: null,
      contactRouteId: null,
      to: "",
      message: latestInbound,
    });
  });

  it("does not create a manual reply target for an owned or incapable thread", () => {
    expect(resolveEmailReplyTarget({
      creatorTarget: null,
      displayedMessage: null,
      threadMessages: [message("inbound", "inbound", 10, "<inbound@example.com>")],
      allowManualThreadReply: false,
    })).toBeNull();
  });
});
