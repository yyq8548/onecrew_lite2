import type { EmailMessageView } from "@/lib/creator";

export type CreatorEmailReplyTarget = {
  creatorId: string;
  contactRouteId: string;
  to: string;
};

export type EmailReplyTarget = {
  creatorId: string | null;
  contactRouteId: string | null;
  to: string;
  message: EmailMessageView;
};

export function latestReplyableInbound(messages: EmailMessageView[]): EmailMessageView | null {
  return messages.reduce<EmailMessageView | null>((latest, message) => {
    if (message.direction !== "inbound" || !message.internet_message_id?.trim()) return latest;
    if (!latest || message.occurred_at >= latest.occurred_at) return message;
    return latest;
  }, null);
}

/** Creator-owned replies keep their existing route flow; manual is unowned-only fallback. */
export function resolveEmailReplyTarget({
  creatorTarget,
  displayedMessage,
  threadMessages,
  allowManualThreadReply,
}: {
  creatorTarget: CreatorEmailReplyTarget | null;
  displayedMessage: EmailMessageView | null;
  threadMessages: EmailMessageView[];
  allowManualThreadReply: boolean;
}): EmailReplyTarget | null {
  if (creatorTarget && displayedMessage) {
    return { ...creatorTarget, message: displayedMessage };
  }
  if (!allowManualThreadReply) return null;
  const inbound = latestReplyableInbound(threadMessages);
  if (!inbound) return null;
  return {
    creatorId: null,
    contactRouteId: null,
    to: "",
    message: inbound,
  };
}
