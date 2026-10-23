import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it, vi } from "vitest";

import { resources } from "@/i18n/resources";
import type { ToolkitDirectory } from "@/lib/toolkit-directory";
import type { Account, ServiceCredential } from "@/types";

import {
  TOOLKIT_CARD_FOOTER_CLASS,
  TOOLKIT_DETAIL_SHEET_CLASS,
  ToolkitCatalogCard,
} from "./toolkit-catalog";
import {
  ACCOUNT_TOOLKIT_SPECS,
  deriveOperatorToolkitStatus,
  deriveServiceToolkitStatus,
  directoryByToolkit,
  filterToolkitOperations,
  matchesToolkitCatalogSearch,
  toolkitConnectionStatusView,
  toolkitOverviewCounts,
  matchesToolkitStatus,
  type AccountToolkitKey,
  type ToolkitConnectionStatus,
} from "./toolkit-catalog-model";

const testI18n = i18next.createInstance();
void testI18n.init({ initAsync: false, lng: "zh-CN", resources });
const t = testI18n.getFixedT("zh-CN", "accounts");

const directory: ToolkitDirectory = {
  id: "toolkit.x",
  toolkit: "x",
  title: "X",
  summary: "X 平台的公开资料、内容和消息工具。",
  purpose: "读取 X 的公开主页与近期内容，并使用已连接身份处理关系和消息。",
  connection_model: "operator account",
  identifier_semantics: ["公开主页使用 handle"],
  capability_categories: ["Creator 发现", "主页资料"],
  gotchas: ["平台权限在调用时复核"],
  operations: [
    { id: "x.posts.search.advanced", title: "通过帖子发现 X 作者", summary: "通过高级帖子搜索发现公开 X 账号的作者。" },
    { id: "x.profile.get", title: "查看 X 主页", summary: "按 handle 读取一位用户的公开主页。" },
  ],
};

function account(platform: Account["platform"], id: string): Account {
  return {
    id,
    platform,
    provider_account_id: `${id}-provider`,
    username: id,
    profile_name: "",
    fingerprint_seed: "",
    x_chat_passcode_status: "not_applicable",
    updated_at: "1",
  };
}

function credential(
  id: string,
  overrides: Partial<ServiceCredential> = {},
): ServiceCredential {
  return {
    id,
    provider: "youtube",
    label: id,
    metadata: {},
    disabled: false,
    cooldown_until: null,
    last_used_at: null,
    last_error: null,
    created_at: "1",
    updated_at: "1",
    ...overrides,
  };
}

describe("Accounts toolkit catalog model", () => {
  it("counts real platform states", () => {
    const statuses = Object.fromEntries(ACCOUNT_TOOLKIT_SPECS.map((spec) => [spec.toolkit, { kind: "not_connected" }])) as Record<AccountToolkitKey, ToolkitConnectionStatus>;
    statuses.x = { kind: "connected", count: 3 };
    statuses.youtube = { kind: "attention", connectedCount: 1, attentionCount: 2, disabledCount: 1 };
    expect(toolkitOverviewCounts(statuses)).toEqual({ total: 5, connected: 2, attention: 1 });
    expect(matchesToolkitStatus(statuses.youtube, "connected")).toBe(true);
    expect(matchesToolkitStatus(statuses.youtube, "attention")).toBe(true);
  });
  it("derives operator inventory without claiming remote health", () => {
    expect(deriveOperatorToolkitStatus({ records: undefined, loading: true, error: false, platform: "x" })).toEqual({ kind: "loading" });
    expect(deriveOperatorToolkitStatus({ records: [], loading: false, error: true, platform: "x" })).toEqual({ kind: "error" });
    expect(deriveOperatorToolkitStatus({ records: [], loading: false, error: false, platform: "x" })).toEqual({ kind: "not_connected" });
    expect(deriveOperatorToolkitStatus({
      records: [account("x", "one"), account("instagram", "two")],
      loading: false,
      error: false,
      platform: "x",
    })).toEqual({ kind: "connected", count: 1 });
    expect(toolkitConnectionStatusView({ kind: "connected", count: 1 }, t).detail).toContain("调用时复核");
  });

  it("distinguishes usable, attention, disabled, missing, and failed credential states", () => {
    const now = 10_000;
    expect(deriveServiceToolkitStatus({ records: undefined, loading: true, error: false, provider: "youtube", now })).toEqual({ kind: "loading" });
    expect(deriveServiceToolkitStatus({ records: [], loading: false, error: true, provider: "youtube", now })).toEqual({ kind: "error" });
    expect(deriveServiceToolkitStatus({ records: [], loading: false, error: false, provider: "youtube", now })).toEqual({ kind: "not_connected" });
    expect(deriveServiceToolkitStatus({ records: [credential("ok")], loading: false, error: false, provider: "youtube", now })).toEqual({ kind: "connected", count: 1 });
    expect(deriveServiceToolkitStatus({ records: [credential("off", { disabled: true })], loading: false, error: false, provider: "youtube", now })).toEqual({ kind: "disabled", count: 1 });
    expect(deriveServiceToolkitStatus({
      records: [
        credential("ok"),
        credential("failed", { last_error: "provider unavailable" }),
        credential("cooling", { cooldown_until: 20 }),
        credential("off", { disabled: true }),
      ],
      loading: false,
      error: false,
      provider: "youtube",
      now,
    })).toEqual({ kind: "attention", connectedCount: 1, attentionCount: 2, disabledCount: 1 });
    expect(toolkitConnectionStatusView({ kind: "disabled", count: 2 }, t)).toMatchObject({ label: "已停用", badge: "danger" });
    expect(toolkitConnectionStatusView({ kind: "error" }, t)).toMatchObject({ label: "读取失败", badge: "danger" });
  });

  it("searches the live directory by canonical id, title, and summary", () => {
    expect(filterToolkitOperations(directory, "profile.get").map((operation) => operation.id)).toEqual(["x.profile.get"]);
    expect(filterToolkitOperations(directory, "公开 X 账号").map((operation) => operation.id)).toEqual(["x.posts.search.advanced"]);
    expect(filterToolkitOperations(directory, "missing")).toEqual([]);

    const spec = ACCOUNT_TOOLKIT_SPECS[0];
    // A capability category is what the card stands for, so it still matches.
    expect(matchesToolkitCatalogSearch(spec, directory, "主页资料")).toBe(true);
    expect(matchesToolkitCatalogSearch(spec, directory, "Creator 发现")).toBe(true);
    // Operation text belongs to the sheet's own search, not to the card:
    // registered summaries share common words, so folding them in here made a
    // short query match every platform.
    const operationOnly: ToolkitDirectory = {
      ...directory,
      operations: [
        { id: "x.profile.get", title: "查看 X 主页", summary: "Read one live X profile by handle." },
      ],
    };
    expect(filterToolkitOperations(operationOnly, "one").map((operation) => operation.id))
      .toEqual(["x.profile.get"]);
    expect(matchesToolkitCatalogSearch(spec, operationOnly, "one")).toBe(false);
    expect(matchesToolkitCatalogSearch(spec, directory, "x.posts.search.advanced")).toBe(false);
    expect(matchesToolkitCatalogSearch(spec, directory, "youtube")).toBe(false);
    expect(directoryByToolkit([directory])).toEqual({ x: directory });
  });

  it("keeps directory inspection and connection management as sibling controls", () => {
    const spec = ACCOUNT_TOOLKIT_SPECS[0];
    const html = renderToStaticMarkup(createElement(
      I18nextProvider,
      { i18n: testI18n },
      createElement(ToolkitCatalogCard, {
        spec,
        directory,
        directoryLoading: false,
        directoryError: false,
        status: { kind: "connected", count: 1 },
        onOpenDirectory: vi.fn(),
        onManage: vi.fn(),
      }),
    ));
    const buttonBodies = [...html.matchAll(/<button[^>]*>([\s\S]*?)<\/button>/g)];
    expect(buttonBodies).toHaveLength(2);
    expect(html).toContain("aria-label=\"查看 X 工具目录\"");
    expect(buttonBodies[0]?.[1]).toContain("2 个工具");
    expect(buttonBodies[1]?.[1]).toContain("管理账号");
    expect(html).toContain("管理账号");
    expect(buttonBodies.every((match) => !match[1]?.includes("<button"))).toBe(true);
    expect(html).not.toContain("hover-card");
    expect(html).toContain("[.border-t]:pt-2");
    expect(html).not.toContain("[.border-t]:pt-6");
  });

  it("keeps the detail sheet bounded when the canvas narrows", () => {
    expect(TOOLKIT_DETAIL_SHEET_CLASS).toContain("w-[min(560px,calc(100%-1rem))]");
    expect(TOOLKIT_DETAIL_SHEET_CLASS).toContain("max-w-[calc(100%-1rem)]");
    expect(TOOLKIT_DETAIL_SHEET_CLASS).toContain("overflow-hidden");
    expect(TOOLKIT_CARD_FOOTER_CLASS).toContain("[.border-t]:pt-2");
  });

  it("renders 0, 1 and 2 with localized metric plurals for identities and credentials", () => {
    for (const locale of ["en-US", "zh-CN"] as const) {
      const instance = i18next.createInstance();
      void instance.init({ initAsync: false, lng: locale, resources });
      const cases = [
        { toolkit: "x", singular: "connected identity", plural: "connected identities", chinese: "个已连接身份" },
        { toolkit: "youtube", singular: "available credential", plural: "available credentials", chinese: "个可用凭据" },
      ] as const;
      for (const metric of cases) {
        for (const count of [0, 1, 2]) {
          const spec = ACCOUNT_TOOLKIT_SPECS.find((item) => item.toolkit === metric.toolkit)!;
          const status: ToolkitConnectionStatus = count === 0 ? { kind: "not_connected" } : { kind: "connected", count };
          const html = renderToStaticMarkup(createElement(I18nextProvider, { i18n: instance }, createElement(ToolkitCatalogCard, {
            spec, directory: undefined, directoryLoading: false, directoryError: false,
            status, onOpenDirectory: vi.fn(), onManage: vi.fn(),
          })));
          const text = html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ");
          const unit = locale === "zh-CN" ? metric.chinese : count === 1 ? metric.singular : metric.plural;
          expect(text, `${locale} ${metric.toolkit} ${count}`).toContain(`${count} ${unit}`);
        }
      }
      const localized = instance.getFixedT(locale, "accounts");
      for (const count of [0, 1, 2]) {
        expect(localized("catalog.overview.cardAttention", { count })).toBe(locale === "zh-CN"
          ? `${count} 项待处理` : `${count} ${count === 1 ? "needs" : "need"} attention`);
      }
    }
  });
});
