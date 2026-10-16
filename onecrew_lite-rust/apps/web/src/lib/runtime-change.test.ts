import { describe, expect, it } from "vitest";

import { EMAIL_TEMPLATE_LIST_QUERY_KEY, MAILBOX_QUERY_KEY } from "@/components/mail/queries";
import {
  CONNECTED_ACCOUNTS_QUERY_KEY,
  CREATORS_QUERY_KEY,
} from "@/lib/creator";
import { queryKeysForRuntimeChangeScopes } from "@/lib/runtime-change";
import { CREATOR_GROUPS_QUERY_KEY } from "@/lib/creator-groups";
import {
  YOUTUBE_CREDENTIALS_QUERY_KEY,
} from "@/lib/runtime-query";

describe("runtime change query invalidation mapping", () => {
  it("refreshes local credential projections and derived mailboxes together", () => {
    expect(queryKeysForRuntimeChangeScopes(["service_credentials"])).toEqual([
      YOUTUBE_CREDENTIALS_QUERY_KEY,
      MAILBOX_QUERY_KEY,
    ]);
  });

  it("refreshes draft and thread lists, details, and their folder counts", () => {
    const keys = queryKeysForRuntimeChangeScopes(["email_drafts", "email_threads"]);

    expect(keys).toContainEqual(["creator-email-folders"]);
    expect(keys).toContainEqual(["creator-email-drafts"]);
    expect(keys).toContainEqual(["creator-email-draft"]);
    expect(keys).toContainEqual(["creator-email-threads"]);
    expect(keys).toContainEqual(["creator-email-thread"]);
  });

  it("refreshes every route-scoped outreach projection", () => {
    expect(queryKeysForRuntimeChangeScopes(["outreach"])).toEqual([
      ["creator-outreach-target"],
      ["creator-message-preparation"],
      ["creator-contact-route-last-contact-account"],
    ]);
  });


  it("keeps Creator rows and authoritative folder counts fresh together", () => {
    expect(queryKeysForRuntimeChangeScopes(["creators"])).toEqual([
      CREATORS_QUERY_KEY,
      CREATOR_GROUPS_QUERY_KEY,
    ]);
    expect(queryKeysForRuntimeChangeScopes(["creator_groups"])).toEqual([
      CREATOR_GROUPS_QUERY_KEY,
    ]);
  });

  it("keeps a thread sync off the provider-backed probes", () => {
    // 同步只失效本地读、列表,和侧边栏那个纯计数读。把权限探测(一次
    // provider 往返 + 密钥恢复)放进来会让每次同步都白跑一遍;把那个
    // 「queryFn 就是 sync」的查询放进来则直接成环 —— 两者都发生过。
    expect(queryKeysForRuntimeChangeScopes(["conversations"])).toEqual([
      ["creator-conversation"],
      ["creator-conversation-list"],
      ["creator-inbox-attention"],
    ]);
  });

  it("refreshes the sidebar badge from both inboxes and nowhere else", () => {
    // 徽标的两个数各有一个来源域,而且只有这两个域会动它。
    for (const scope of ["conversations", "email_threads"] as const) {
      expect(queryKeysForRuntimeChangeScopes([scope])).toContainEqual([
        "creator-inbox-attention",
      ]);
    }
    expect(queryKeysForRuntimeChangeScopes(["creators"])).not.toContainEqual([
      "creator-inbox-attention",
    ]);
  });

  it("deduplicates all scopes against an overlapping named scope", () => {
    const keys = queryKeysForRuntimeChangeScopes(["all", "creators"]);
    const serialized = keys.map((queryKey) => JSON.stringify(queryKey));

    expect(new Set(serialized).size).toBe(keys.length);
    expect(keys).toContainEqual(CONNECTED_ACCOUNTS_QUERY_KEY);
    expect(keys).toContainEqual(CREATORS_QUERY_KEY);
    expect(keys).toContainEqual(CREATOR_GROUPS_QUERY_KEY);
    expect(keys).toContainEqual(EMAIL_TEMPLATE_LIST_QUERY_KEY);
  });
});
