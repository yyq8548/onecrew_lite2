import { describe, expect, it } from "vitest";

import MESSAGE_WORKSPACE_SOURCE from "@/components/message-workspace.tsx?raw";
import type { ConversationSummary } from "@/lib/conversations";
import {
  conversationRowIsOpen,
  openThreadForConversation,
  openThreadForDraft,
  threadAfterConversationOpened,
} from "@/pages/inbox-thread";

function summary(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    conversation_id: "conversation-1",
    creator_id: "creator-1",
    creator_display_name: "Creator",
    peer_handle: "creator",
    channel: "x",
    operator_account_ref_id: "account-1",
    operator_account_label: "@operator",
    contact_route_id: "route-1",
    resolved_target_id: "target-1",
    provider_thread_id: "thread-1",
    updated_at: 1,
    ...overrides,
  };
}

const draft = openThreadForDraft({
  creatorId: "creator-1",
  contactRouteId: "route-1",
  operatorId: "account-1",
  accountLabel: "@operator",
});

describe("which thread the inbox has open", () => {
  it("matches a list row by its conversation and the account sending on it", () => {
    const open = openThreadForConversation(summary());
    expect(conversationRowIsOpen(open, summary())).toBe(true);
    expect(conversationRowIsOpen(open, summary({ conversation_id: "conversation-2" }))).toBe(false);
    // The same conversation answered from a different operator account is a
    // different thread, and carries its own row.
    expect(conversationRowIsOpen(open, summary({ operator_account_ref_id: "account-2" }))).toBe(false);
    expect(conversationRowIsOpen(null, summary())).toBe(false);
  });

  it("matches no row while the thread is still a draft", () => {
    expect(draft.conversationId).toBeNull();
    expect(conversationRowIsOpen(draft, summary())).toBe(false);
  });

  it("adopts the conversation a first sync or first send created, keeping the mount key", () => {
    const adopted = threadAfterConversationOpened(draft, "conversation-1");
    expect(adopted?.conversationId).toBe("conversation-1");
    // The mount key must not move: MessageWorkspace remounting on the send it
    // just completed would re-run the automatic thread sync.
    expect(adopted?.key).toBe(draft.key);
    expect(conversationRowIsOpen(adopted, summary())).toBe(true);
  });

  it("returns the same thread when there is nothing to adopt", () => {
    const open = openThreadForConversation(summary());
    expect(threadAfterConversationOpened(open, "conversation-1")).toBe(open);
    expect(threadAfterConversationOpened(null, "conversation-1")).toBeNull();
  });

  it("keeps a draft's mount key independent of the conversation it will create", () => {
    const other = openThreadForDraft({
      creatorId: "creator-1",
      contactRouteId: "route-1",
      operatorId: "account-2",
      accountLabel: "@second",
    });
    expect(other.key).not.toBe(draft.key);
  });
});

describe("local history never waits on the provider probe", () => {
  const threadRender = MESSAGE_WORKSPACE_SOURCE.slice(
    MESSAGE_WORKSPACE_SOURCE.indexOf("<MessageScroller.Content"),
    MESSAGE_WORKSPACE_SOURCE.indexOf("</MessageScroller.Content>"),
  );

  it("renders the stored thread without waiting for the capability probe", () => {
    // An inbox row carries its conversation id, so `onecrew_conversation_get`
    // can answer from the local ledger at once. Gating the bubbles, the
    // spinner or the empty state on `targetPolicy` — which only exists after
    // `onecrew_contact_target_resolve` returns from the provider — hid history
    // the app already had.
    expect(threadRender).toContain("{threadItems.map((item) => {");
    expect(threadRender).not.toContain("targetPolicy?.canDirectMessage");
  });

  it("claims the thread is empty only once the local read and any first sync are done", () => {
    expect(MESSAGE_WORKSPACE_SOURCE).toContain(
      "const threadLoading = conversation.isFetching || syncThread.isPending;",
    );
    // The extra wait is only the empty state's: a thread with no conversation
    // id yet is about to be filled by the first sync, and the resolving notice
    // is already on screen, so a second spinner would stack on it.
    expect(MESSAGE_WORKSPACE_SOURCE).toContain(
      "const threadMayStillFill = threadLoading || (!openConversationId && target.isFetching);",
    );
    expect(threadRender).toContain("{threadLoading && !threadItems.length &&");
    expect(threadRender).toContain("{!threadMayStillFill && !conversation.error && !threadItems.length &&");
  });

  it("tells the host which conversation the thread turned out to be", () => {
    expect(MESSAGE_WORKSPACE_SOURCE).toContain("onConversationOpened?.(id)");
  });
});
