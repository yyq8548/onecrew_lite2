import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import accessI18n from "@/lib/i18n";

import { competitors, competitorSlugs } from "./competitors";
import { MarketingRoutes } from "./marketing-routes";

const accessStateMocks = vi.hoisted(() => ({
  useAuthCapabilities: vi.fn(() => ({ emailOtpEnabled: false, googleSignInEnabled: false })),
  useSafeSession: vi.fn(() => ({
    refresh: vi.fn(),
    state: { status: "loading" as const },
  })),
}));

vi.mock("@/lib/use-access-state", () => accessStateMocks);

beforeEach(async () => {
  accessStateMocks.useAuthCapabilities.mockClear();
  accessStateMocks.useSafeSession.mockClear();
  await accessI18n.changeLanguage("zh-CN");
});

describe("MarketingRoutes", () => {
  function readSchema() {
    return JSON.parse(document.querySelector('script[type="application/ld+json"]')?.textContent ?? "null");
  }

  it.each([
    ["/product", "从发现达人到获得回复，保持一条连续的工作流。"],
    ["/product/creator-discovery", "找到更合适的 KOL，而不是得到更长的名单。"],
    ["/use-cases", "同一套达人外联流程，适配不同规模的执行团队。"],
    ["/use-cases/agencies", "让每个客户项目，都有清楚的达人关系与下一步。"],
    ["/resources", "把达人外联的方法讲清楚，再决定工具应该做什么。"],
    ["/resources/kol-outreach-guide", "KOL 外联怎么做：从找达人到持续跟进的 6 个步骤"],
    ["/about", "找到对的创作者， 建立长久的合作关系。"],
    ["/pricing", "从你的真实工作流开始确认方案"],
    ["/compare/lev8", "OneCrew 与 Lev8 怎么选"],
    ["/terms", "服务条款"],
    ["/privacy", "隐私政策"],
  ])("renders %s from the public route tree", (path, heading) => {
    render(<MemoryRouter initialEntries={[path]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(document.getElementById("main-content")).toBeInTheDocument();
  });

  it("renders a marketing 404 for an unknown comparison", () => {
    render(<MemoryRouter initialEntries={["/compare/not-a-product"]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: "没有找到这个页面" })).toBeInTheDocument();
    expect(document.querySelector('meta[name="robots"]')).toHaveAttribute("content", "noindex, nofollow");
  });

  it("keeps the comparison route restricted to the approved competitors", () => {
    expect(competitorSlugs).toEqual([
      "tiktok-shop-affiliate",
      "lev8",
      "modash",
      "hypeauditor",
      "upfluence",
      "saral",
      "influencity",
      "aspire",
    ]);
    expect(competitors).toHaveLength(8);
  });

  it.each([
    [
      "zh-CN",
      "OneCrew 与 Modash 怎么选",
      "Modash 是以 Shopify 品牌为重点的一体化网红营销平台；OneCrew 更强调把已有 AI 与达人数据、品牌邮箱和支持的社交渠道连接起来。",
      "产品起点",
    ],
    [
      "en-US",
      "OneCrew vs. Modash",
      "Modash is an all-in-one influencer marketing platform with a strong Shopify focus. OneCrew centers the workflow on connecting your existing AI with creator data, brand email, and supported social channels.",
      "Starting point",
    ],
  ])("renders comparison copy from the %s locale resource", async (language, heading, summary, rowLabel) => {
    await accessI18n.changeLanguage(language);
    render(<MemoryRouter initialEntries={["/compare/modash"]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(screen.getByText(summary)).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: rowLabel })).toBeInTheDocument();
  });

  it("uses the shared i18next language for every page", async () => {
    await accessI18n.changeLanguage("en-US");
    render(<MemoryRouter initialEntries={["/resources"]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: "Understand the outreach method before deciding what the tool should do." })).toBeInTheDocument();
  });

  it.each([
    ["zh-CN", "/about", "找到对的创作者， 建立长久的合作关系。", "基于理解，建立合作关系"],
    ["en-US", "/about", "Find the right creators. Build lasting partnerships.", "Build partnerships on understanding"],
    ["zh-CN", "/pricing", "从你的真实工作流开始确认方案", "不是只看演示，而是验证真实工作"],
    ["en-US", "/pricing", "A plan shaped around the workflow you actually run.", "Enough to test the work, not just watch a demo."],
    ["zh-CN", "/product", "从发现达人到获得回复，保持一条连续的工作流。", "Agent 已完成检查"],
    ["en-US", "/product", "Keep one continuous workflow from creator discovery to reply.", "Agent review complete"],
    ["zh-CN", "/use-cases/agencies", "让每个客户项目，都有清楚的达人关系与下一步。", "为什么现有流程容易失控"],
    ["en-US", "/use-cases/agencies", "Give every client campaign a clear relationship history and next step.", "Why the current workflow loses control"],
  ])("renders extracted %s copy for %s", async (language, path, heading, supportingCopy) => {
    await accessI18n.changeLanguage(language);
    render(<MemoryRouter initialEntries={[path]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
    expect(screen.getByText(supportingCopy)).toBeInTheDocument();
  });

  it.each([
    [
      "zh-CN",
      "OneCrew｜AI 达人发现与外联工作台",
      "OneCrew 帮助出海品牌发现合适的 KOL，集中管理邮件与社交私信，并通过 AI Agent 检查回复、跟进和推进合作。",
    ],
    [
      "en-US",
      "OneCrew — AI Creator Discovery & Influencer Outreach",
      "Find, qualify, contact, and follow up with creators through the AI services you already use.",
    ],
  ])("uses the %s home metadata resource", async (language, title, description) => {
    await accessI18n.changeLanguage(language);
    render(<MemoryRouter initialEntries={["/"]}><MarketingRoutes /></MemoryRouter>);
    await waitFor(() => expect(document.title).toBe(title));
    expect(document.querySelector('meta[name="description"]')).toHaveAttribute("content", description);
  });

  it("updates route metadata and canonical URL", async () => {
    render(<MemoryRouter initialEntries={["/product"]}><MarketingRoutes /></MemoryRouter>);
    await waitFor(() => expect(document.title).toBe("产品总览｜OneCrew KOL 发现与外联工作台"));
    expect(document.querySelector('meta[name="description"]')).toHaveAttribute(
      "content",
      "了解 OneCrew 如何连接达人发现、统一沟通、AI 回复检查、跟进和合作推进。",
    );
    expect(document.querySelector('link[rel="canonical"]')).toHaveAttribute(
      "href",
      "https://onecrewai.io/zh/product",
    );
  });

  it.each([
    ["/about", ["首页", "关于我们"]],
    ["/pricing", ["首页", "定价"]],
    ["/product", ["首页", "产品"]],
    ["/product/ai-agent", ["首页", "产品", "AI Agent"]],
    ["/use-cases", ["首页", "解决方案"]],
    ["/use-cases/agencies", ["首页", "解决方案", "达人营销代理团队"]],
    ["/resources", ["首页", "资源中心"]],
    ["/resources/kol-outreach-guide", ["首页", "资源", "KOL 外联怎么做：从找达人到持续跟进的 6 个步骤"]],
    ["/compare/modash", ["首页", "产品对比", "OneCrew 与 Modash"]],
    ["/terms", ["首页", "服务条款"]],
  ])("keeps the visible breadcrumbs and BreadcrumbList aligned for %s", (path, labels) => {
    render(<MemoryRouter initialEntries={[path]}><MarketingRoutes /></MemoryRouter>);
    const breadcrumb = readSchema()["@graph"].find((item: { "@type": string }) => item["@type"] === "BreadcrumbList");
    expect(breadcrumb.itemListElement.map((item: { name: string }) => item.name)).toEqual(labels);
    expect(breadcrumb.itemListElement.map((item: { position: number }) => item.position)).toEqual(labels.map((_, index) => index + 1));
  });

  it.each([
    ["zh-CN", "作者 OneCrew · 发布于 2026 年 8 月 26 日 · 更新于 2026 年 9 月 5 日"],
    ["en-US", "By OneCrew · Published August 26, 2026 · Updated September 5, 2026"],
  ])("shows the Organization author and article dates in %s", async (language, byline) => {
    await accessI18n.changeLanguage(language);
    render(<MemoryRouter initialEntries={["/resources/kol-outreach-guide"]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByText(byline)).toBeInTheDocument();
    const article = readSchema()["@graph"].find((item: { "@type": string }) => item["@type"] === "Article");
    expect(article).toMatchObject({
      datePublished: "2026-08-26",
      dateModified: "2026-09-05",
      author: { "@type": "Organization", name: "OneCrew" },
      publisher: { "@type": "Organization", name: "OneCrew" },
    });
  });

  it("keeps the comparison's official source and reviewed date visible and structured", () => {
    render(<MemoryRouter initialEntries={["/compare/modash"]}><MarketingRoutes /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 2, name: "Modash 官方平台与帮助中心" })).toBeInTheDocument();
    expect(screen.getByText(/页面核对时间：2026 年 9 月/)).toBeInTheDocument();
    const page = readSchema()["@graph"].find((item: { "@type": string }) => item["@type"] === "WebPage");
    expect(page.dateModified).toBe("2026-09-05");
  });

  it.each([
    [
      "zh-CN",
      "/resources/kol-outreach-guide",
      "Article",
      "https://onecrewai.io/zh/resources/kol-outreach-guide",
      ["https://onecrewai.io/zh", "https://onecrewai.io/zh/resources"],
    ],
    [
      "en-US",
      "/resources/kol-outreach-guide",
      "Article",
      "https://onecrewai.io/resources/kol-outreach-guide",
      ["https://onecrewai.io/", "https://onecrewai.io/resources"],
    ],
    [
      "en-US",
      "/product/ai-agent",
      "WebPage",
      "https://onecrewai.io/product/ai-agent",
      ["https://onecrewai.io/", "https://onecrewai.io/product"],
    ],
  ])("uses locale-aware JSON-LD URLs for %s at %s", async (language, path, pageType, pageUrl, breadcrumbUrls) => {
    await accessI18n.changeLanguage(language);
    render(<MemoryRouter initialEntries={[path]}><MarketingRoutes /></MemoryRouter>);
    const graph = readSchema()["@graph"];
    expect(graph.find((item: { "@type": string }) => item["@type"] === pageType).url).toBe(pageUrl);
    const breadcrumb = graph.find((item: { "@type": string }) => item["@type"] === "BreadcrumbList");
    expect(breadcrumb.itemListElement.flatMap((item: { item?: string }) => item.item ?? [])).toEqual(breadcrumbUrls);
  });
});

describe("App public route boundary", () => {
  it("does not initialize session or auth capability state for marketing pages", () => {
    render(<MemoryRouter initialEntries={["/product/ai-agent"]}><App /></MemoryRouter>);
    expect(screen.getByRole("heading", { level: 1, name: "Agent 负责盯进度，你负责做决定。" })).toBeInTheDocument();
    expect(accessStateMocks.useSafeSession).not.toHaveBeenCalled();
    expect(accessStateMocks.useAuthCapabilities).not.toHaveBeenCalled();
  });

  it("keeps the existing Access boundary for login", () => {
    render(<MemoryRouter initialEntries={["/login"]}><App /></MemoryRouter>);
    expect(screen.getByText("正在检查登录状态…")).toBeInTheDocument();
    expect(accessStateMocks.useSafeSession).toHaveBeenCalledOnce();
    expect(accessStateMocks.useAuthCapabilities).toHaveBeenCalledOnce();
  });

  it("synchronizes the document language through the shared i18next instance", async () => {
    await accessI18n.changeLanguage("en-US");
    render(<MemoryRouter initialEntries={["/product"]}><App /></MemoryRouter>);
    await waitFor(() => expect(document.documentElement).toHaveAttribute("lang", "en"));
  });
});
