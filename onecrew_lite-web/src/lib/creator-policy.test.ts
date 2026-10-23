import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";

import { RuntimeUnavailable } from "@/components/runtime-unavailable";
import { appI18n } from "@/i18n";
import { resources } from "@/i18n/resources";
import {
  conversationSyncInput,
  creatorInvoke,
  emailContactRoutes,
  relationshipContextInput,
  sendDirectMessageInput,
  contactTargetPolicy,
  updateRelationshipInput,
  type ContactAction,
  type ContactTargetResolutionResult,
  type CreatorContactRouteRecord,
  type CreatorRecord,
  type IdentityConflict,
} from "@/lib/creator";

function route(
  id: string,
  channel: CreatorContactRouteRecord["locator"]["channel"],
): CreatorContactRouteRecord {
  return {
    route: {
      id,
      creator_id: "creator-1",
      locator_id: `locator-${id}`,
      confidence: "exact",
      created_at: 1,
      updated_at: 1,
    },
    locator: {
      id: `locator-${id}`,
      channel,
      kind: channel === "email" ? "email_address" : "handle",
      normalized_value: channel === "email" ? "creator@example.com" : "creator",
      display_value: channel === "email" ? "creator@example.com" : "@creator",
      created_at: 1,
      updated_at: 1,
    },
    observations: [],
    allowed_actions: [],
  };
}

function resolution(
  status: ContactTargetResolutionResult["status"],
  allowedActions: ContactAction[],
  options: Pick<ContactTargetResolutionResult, "browser_url" | "conflict"> = {},
): ContactTargetResolutionResult {
  return {
    creator_id: "creator-1",
    route: route("route-x", "x"),
    operator_account: {
      id: "account-1",
      platform: "x",
      kind: "profile",
      label: "Operator",
      active: true,
      availability: "available",
      created_at: 1,
      updated_at: 1,
    },
    status,
    allowed_actions: allowedActions,
    ...options,
  };
}

describe("contact target policy", () => {
  it("allows resolved actions only when runtime returns them", () => {
    expect(contactTargetPolicy(resolution(
      "resolved",
      ["follow", "direct_message", "open_profile"],
      { browser_url: "https://x.com/creator" },
    ))).toMatchObject({
      status: "resolved",
      canFollow: true,
      canDirectMessage: true,
      canOpenBrowser: true,
      browserUrl: "https://x.com/creator",
    });
  });

  it("keeps conflicts and blocks write actions outside resolved state", () => {
    const conflict: IdentityConflict = {
      id: "conflict-1",
      conflict_key: "x:creator",
      kind: "target_identity",
      creator_id: "creator-1",
      evidence: {},
      state: "open",
      created_at: 1,
    };
    const policy = contactTargetPolicy(resolution(
      "conflict",
      ["follow", "direct_message", "open_profile"],
      { browser_url: "https://x.com/creator", conflict },
    ));

    expect(policy).toMatchObject({
      status: "conflict",
      canFollow: false,
      canDirectMessage: false,
      canOpenBrowser: true,
      browserUrl: "https://x.com/creator",
      conflict,
    });
  });

  it("requires both a browser URL and an allowed read/open action", () => {
    expect(contactTargetPolicy(resolution("browser_only", [], {
      browser_url: "https://x.com/creator",
    })).canOpenBrowser).toBe(false);
    expect(contactTargetPolicy(resolution("browser_only", ["open_profile"])).canOpenBrowser)
      .toBe(false);
  });
});

describe("contact route and runtime-unavailable contracts", () => {
  it("keeps every private-message and relationship action scoped to its selected route", () => {
    const context = relationshipContextInput("creator-1", "route-1", "target-1", "operator-1");
    expect(context).toEqual({
      creator_id: "creator-1",
      contact_route_id: "route-1",
      resolved_target_id: "target-1",
      operator_account_ref_id: "operator-1",
    });
    expect(conversationSyncInput(context, "thread-1", 100)).toEqual({
      ...context,
      external_thread_id: "thread-1",
      limit: 100,
    });
    expect(updateRelationshipInput(context, "follow", "relationship-key")).toEqual({
      ...context,
      action: "follow",
      idempotency_key: "relationship-key",
    });
    expect(sendDirectMessageInput(context, "hello", false, "message-key")).toEqual({
      ...context,
      body: "hello",
      allow_unfollowed_recipient: false,
      idempotency_key: "message-key",
    });
  });

  it("returns every existing email route", () => {
    const first = route("email-1", "email");
    const second = route("email-2", "email");
    const record: CreatorRecord = {
      creator: {
        id: "creator-1",
        display_name: "Creator",
        tags: [],
        follower_counts: {},
        created_at: 1,
        updated_at: 1,
      },
      group: null,
      relationship: {
        creator_id: "creator-1",
        stage: "prospect",
        created_at: 1,
        updated_at: 1,
      },
      platform_accounts: [],
      account_links: [],
      account_observations: [],
      observation_sources: [],
      origins: [],
      contact_routes: [
        first,
        second,
        route("x", "x"),
      ],
    };

    expect(emailContactRoutes(record)).toEqual([first, second]);
    expect(emailContactRoutes(null)).toEqual([]);
  });

  it("renders the explicit desktop-runtime unavailable state", async () => {
    if (!appI18n.isInitialized) {
      await appI18n.init({ fallbackLng: "zh-CN", lng: "zh-CN", resources });
    }
    await expect(creatorInvoke("onecrew_i18n_contract_test")).rejects.toThrow(
      resources["zh-CN"].common.runtimeUnavailable.description,
    );
    const markup = renderToStaticMarkup(createElement(
      I18nextProvider,
      { i18n: appI18n },
      createElement(RuntimeUnavailable),
    ));

    expect(markup).toContain(resources["zh-CN"].common.runtimeUnavailable.title);
    expect(markup).toContain(resources["zh-CN"].common.runtimeUnavailable.description);
    expect(markup).not.toContain("Reacher");
  });
});
