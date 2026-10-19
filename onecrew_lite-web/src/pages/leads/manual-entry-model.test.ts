import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";

import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import type { CreatorContactRouteRecord } from "@/lib/creator";
import type { LeadsT } from "./i18n-labels";
import {
  contactRouteDeleteDescription,
  contactRouteDeleteBlockedMessage,
  contactRouteDeleteConfirmInput,
  contactRouteDeleteImpactItems,
  contactRouteDeletePrepareInput,
  contactRouteDeleteRetryKey,
  contactRouteEditDraft,
  manualContactRouteInput,
  manualContactRouteUpdateInput,
  manualCreatorInput,
} from "./manual-entry-model";

let t: LeadsT;

beforeAll(async () => {
  const instance = i18next.createInstance();
  await instance.init({
    lng: "zh-CN",
    ns: ["leads"],
    defaultNS: "leads",
    interpolation: { escapeValue: false },
    resources: { "zh-CN": { leads: zhCNLeads } },
  });
  t = instance.getFixedT("zh-CN", "leads");
});

function manualRoute(): CreatorContactRouteRecord {
  return {
    route: {
      id: "route-1",
      creator_id: "creator-1",
      locator_id: "locator-1",
      confidence: "asserted",
      created_at: 1,
      updated_at: 2,
    },
    locator: {
      id: "locator-1",
      channel: "email",
      kind: "email_address",
      normalized_value: "creator@example.com",
      display_value: "Creator@Example.COM",
      created_at: 1,
      updated_at: 2,
    },
    observations: [{
      observation: {
        id: "observation-1",
        locator_id: "locator-1",
        source_id: "source-1",
        creator_id: "creator-1",
        platform_account_id: null,
        observed_value: " Creator@Example.COM ",
        observed_at: 1,
      },
      source: {
        id: "source-1",
        kind: "manual_entry",
        platform: null,
        source_url: "https://example.com/contact",
        external_ref: null,
        account_ref_id: null,
        metadata: {},
        observed_at: 1,
      },
    }],
    allowed_actions: ["update", "delete"],
  };
}

describe("manual Creator inputs", () => {
  it("builds the flat runtime create contract and omits empty notes", () => {
    expect(manualCreatorInput({ displayName: "  Ada  ", notes: "  " })).toEqual({
      input: { display_name: "  Ada  ", tags: [], notes: null, contact_routes: [] },
      error: null,
    });
    expect(manualCreatorInput({ displayName: " ", notes: "" }).error)
      .toBe("contactRoutes.validation.creatorNameRequired");
  });

  it("keeps route input raw so only runtime derives canonical locator facts", () => {
    const result = manualContactRouteInput({
      creatorId: "creator-1",
      channel: "email",
      value: " Creator@Example.COM ",
      sourceUrl: " ",
    });
    expect(result).toEqual({
      input: {
        creator_id: "creator-1",
        channel: "email",
        value: " Creator@Example.COM ",
        source_url: null,
      },
      error: null,
    });
    expect(Object.keys(result.input ?? {})).not.toContain("normalized_value");
    expect(Object.keys(result.input ?? {})).not.toContain("locator_kind");
    expect(manualContactRouteInput({ creatorId: "creator-1", channel: "x", value: " ", sourceUrl: "" }).error)
      .toBe("contactRoutes.validation.valueRequired");
  });

  it("prefills an editable route and builds the flat raw update contract", () => {
    const draft = contactRouteEditDraft(manualRoute());
    expect(draft).toEqual({
      creatorId: "creator-1",
      contactRouteId: "route-1",
      channel: "email",
      value: " Creator@Example.COM ",
      sourceUrl: "https://example.com/contact",
    });
    expect(manualContactRouteUpdateInput({ ...draft, value: " New@Example.COM ", sourceUrl: " https://example.com/new " })).toEqual({
      input: {
        creator_id: "creator-1",
        contact_route_id: "route-1",
        channel: "email",
        value: " New@Example.COM ",
        source_url: "https://example.com/new",
      },
      error: null,
    });
  });

  it("builds the strict delete preparation input and describes the impact check", () => {
    const route = manualRoute();
    expect(contactRouteDeletePrepareInput(route)).toEqual({ creator_id: "creator-1", contact_route_id: "route-1" });
    expect(contactRouteDeleteDescription(route, t)).toBe(
      "删除“Creator@Example.COM”前，系统会检查依赖关系与需要清理的本地数据。",
    );
    expect(contactRouteDeleteConfirmInput("operation-1")).toEqual({ operation_id: "operation-1" });
  });

  it("maps every canonical local cleanup count for the confirmation dialog", () => {
    const impact = {
      dependencies: 1,
      contact_routes: 2,
      email_threads: 3,
      email_messages: 4,
      email_drafts: 5,
      email_recipients: 6,
      direct_message_conversations: 7,
      direct_messages: 8,
      outreach_touchpoints: 9,
      contact_locators: 10,
      observation_sources: 11,
      resolved_targets: 12,
      creator_origins_detached: 14,
      audit_events: 15,
      idempotency_records: 16,
      target_resolutions: 17,
      identity_conflicts: 18,
      observations: 19,
      local_records_total: 177,
    } as const;
    expect(contactRouteDeleteImpactItems(impact, t)).toEqual(expect.arrayContaining([
      { key: "dependencies", label: "来源依赖", count: 1 },
      { key: "contact_routes", label: "建联通道", count: 2 },
      { key: "email_threads", label: "邮件线程", count: 3 },
      { key: "email_messages", label: "邮件消息", count: 4 },
      { key: "email_drafts", label: "邮件草稿", count: 5 },
      { key: "email_recipients", label: "邮件收件地址", count: 6 },
      { key: "direct_message_conversations", label: "私信会话", count: 7 },
      { key: "direct_messages", label: "私信消息", count: 8 },
      { key: "outreach_touchpoints", label: "触达记录", count: 9 },
      { key: "contact_locators", label: "建联地址", count: 10 },
      { key: "observation_sources", label: "来源记录", count: 11 },
      { key: "resolved_targets", label: "已解析目标", count: 12 },
      { key: "creator_origins_detached", label: "Creator 来源解绑", count: 14 },
    ]));
  });

  it("explains dependency-first deletion and requires a new prepare after stale confirmation", () => {
    expect(contactRouteDeleteBlockedMessage("has_derived_contact_routes", t))
      .toContain("先从叶子通道开始删除");
    expect(contactRouteDeleteBlockedMessage("root_initial_platform_account", t))
      .toBe("初次收录的根平台账号不能删除。");
    expect(contactRouteDeleteRetryKey({
      status: "stale",
      operation_id: "operation-1",
      reason: "plan_changed",
      next_tool: "onecrew.creator.contact_route.delete.prepare",
    })).toBe("contactRoutes.delete.retryStale");
    expect(contactRouteDeleteRetryKey({
      status: "expired",
      operation_id: "operation-1",
      next_tool: "onecrew.creator.contact_route.delete.prepare",
    })).toBe("contactRoutes.delete.retryExpired");
  });
});
