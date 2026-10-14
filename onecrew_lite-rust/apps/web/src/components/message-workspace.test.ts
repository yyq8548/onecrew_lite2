import { describe, expect, it } from "vitest";

import { socialChannels } from "@/components/message-workspace";
import MESSAGE_WORKSPACE_SOURCE from "@/components/message-workspace.tsx?raw";
import type {
  ContactChannel,
  CreatorContactRouteRecord,
  CreatorRecord,
  PlatformAccount,
} from "@/lib/creator";

const TELEGRAM_ACCOUNT_ID = "tg-account";

function route(
  channel: ContactChannel,
  value: string,
  { observedAccountId = null }: { observedAccountId?: string | null } = {},
): CreatorContactRouteRecord {
  const id = `${channel}-${value}`;
  return {
    route: { id: `${id}-route`, creator_id: "creator", locator_id: id, confidence: "exact", created_at: 1, updated_at: 1 },
    locator: {
      id,
      channel,
      kind: channel === "email" ? "email_address" : "handle",
      normalized_value: value,
      display_value: value,
      created_at: 1,
      updated_at: 1,
    },
    observations: observedAccountId
      ? [{
        observation: {
          id: `${id}-observation`,
          locator_id: id,
          source_id: `${id}-source`,
          platform_account_id: observedAccountId,
          observed_value: value,
          observed_at: 1,
        },
        source: { id: `${id}-source`, kind: "initial_platform_account", metadata: {}, observed_at: 1 },
      }]
      : [],
    allowed_actions: [],
  };
}

function creatorRecord(
  routes: CreatorContactRouteRecord[],
  accounts: PlatformAccount[] = [],
): CreatorRecord {
  return {
    creator: { id: "creator", display_name: "Creator", tags: [], follower_counts: {}, created_at: 1, updated_at: 1, archived_at: null },
    group: null,
    relationship: { creator_id: "creator", stage: "prospect", created_at: 1, updated_at: 1 },
    platform_accounts: accounts,
    account_links: [],
    account_observations: [],
    observation_sources: [],
    origins: [],
    contact_routes: routes,
  };
}

function telegramAccount(targetKind: PlatformAccount["target_kind"]): PlatformAccount {
  return { id: TELEGRAM_ACCOUNT_ID, platform: "telegram", target_kind: targetKind, external_id: "1", created_at: 1, updated_at: 1 };
}

describe("the DM channels a Creator offers", () => {
  it("keeps only the platforms that carry a direct-message thread", () => {
    const record = creatorRecord([
      route("x", "alpha"),
      route("instagram", "alpha"),
      route("telegram", "alpha"),
      route("email", "alpha@example.com"),
      route("youtube", "alpha"),
      route("tiktok", "alpha"),
      route("website", "https://alpha.example"),
      route("link_hub", "https://links.example/alpha"),
    ]);
    expect(socialChannels(record).map((channel) => channel.platform)).toEqual(["x", "instagram", "telegram"]);
  });

  it("reads the handle out of a profile URL and keeps one channel per handle", () => {
    const record = creatorRecord([
      route("x", "https://x.com/alpha"),
      route("x", "@Alpha"),
      route("instagram", "https://www.instagram.com/alpha/"),
    ]);
    expect(socialChannels(record).map((channel) => `${channel.platform}:${channel.handle}`))
      .toEqual(["x:alpha", "instagram:alpha"]);
  });

  it("holds a Telegram route for identification until a user account is observed through it", () => {
    const unlinked = socialChannels(creatorRecord([route("telegram", "alpha")]));
    expect(unlinked.map((channel) => channel.requiresIdentification)).toEqual([true]);

    const observed = [route("telegram", "alpha", { observedAccountId: TELEGRAM_ACCOUNT_ID })];
    expect(socialChannels(creatorRecord(observed, [telegramAccount("user")]))[0].requiresIdentification).toBe(false);
    // A channel or a bot is not the human user a direct message needs, so the
    // route still has to be identified.
    expect(socialChannels(creatorRecord(observed, [telegramAccount("channel")]))[0].requiresIdentification).toBe(true);
  });

  it("never asks X or Instagram to be identified first", () => {
    const record = creatorRecord([route("x", "alpha"), route("instagram", "alpha")]);
    expect(socialChannels(record).every((channel) => !channel.requiresIdentification)).toBe(true);
  });

  it("has nothing to offer without a Creator", () => {
    expect(socialChannels(null)).toEqual([]);
  });
});

describe("sending a message refreshes the thread it just wrote to", () => {
  const sendMutation = MESSAGE_WORKSPACE_SOURCE.slice(
    MESSAGE_WORKSPACE_SOURCE.indexOf("const send = useMutation({"),
    MESSAGE_WORKSPACE_SOURCE.indexOf("const supportsFollow"),
  );

  it("keys the local re-read on the conversation the send returned", () => {
    expect(sendMutation).toContain("adoptRecordedConversation(recorded.record.conversation.id)");
    expect(sendMutation).toContain('queryKey: ["creator-conversation", conversationId]');
  });

  it("never refetches the observer whose key the same handler just moved", () => {
    // observer 的 options 要等渲染后的 effect 才更新,所以在 onSuccess 里
    // refetch 会用上一轮的闭包按 conversation_id: null 再读一次:runtime 拒绝
    // 这个参数,错误又落在所有尚无会话行的线程共用的那个 key 上。
    expect(sendMutation).not.toContain("conversation.refetch()");
  });
});
