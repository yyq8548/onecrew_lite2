import { createInstance } from "i18next";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";

import APP_SOURCE from "@/App.tsx?raw";
import APP_SIDEBAR_SOURCE from "@/components/AppSidebar.tsx?raw";
import { DEFAULT_LOCALE, NAMESPACES, resources, SUPPORTED_LOCALES } from "@/i18n/resources";
import { SERVICE_STATUS_QUERY_KEY } from "@/lib/connect";
import type { LocalServiceStatus } from "@/lib/generated/creator-sync-contracts";
import { validateLocalServiceStatus } from "@/lib/generated/creator-sync-contracts/validators/LocalServiceStatus.js";
import { Connect } from "@/pages/Connect";
import MAIL_ITEM_LIST_SOURCE from "@/components/mail/mail-item-list.tsx?raw";
import MAIL_SOURCE from "@/pages/Mail.tsx?raw";
import LEADS_SOURCE from "@/pages/Leads.tsx?raw";
import TABLE_TOOLBAR_SOURCE from "@/components/data-table/toolbar.tsx?raw";

const PLUGIN_VERSION = `0.6.0+codex.${"0123456789abcdef".repeat(4)}`;

const STATUS: LocalServiceStatus = {
  locale: "zh-CN",
  binary_sha256: "b".repeat(64),
  cli_update: {
    checked_at: null,
    current_version: "0.6.0",
    diagnostic_code: null,
    latest_version: null,
    source: null,
    state: "not_checked",
  },
  data: null,
  login: null,
  login_error: null,
  workspace_activity: null,
  workspace_activity_error: null,
  workspace_history: [],
  mcp_clients: 0,
  next_actions: [],
  pid: 1,
  plugin: {
    diagnostic_code: null,
    enabled: true,
    installed_version: PLUGIN_VERSION,
    last_verified_at: null,
    state: "installed",
    verification: "cached",
  },
  running: true,
  startup: { phase: "ready" },
  transport: "local_http",
  version: "0.6.0",
};

it("accepts service-owned task history without weakening status validation", () => {
  expect(validateLocalServiceStatus(STATUS)).toBe(true);
  expect(validateLocalServiceStatus({ ...STATUS, workspace_history: [null] })).toBe(false);
  const { workspace_history: _, ...missingHistory } = STATUS;
  expect(validateLocalServiceStatus(missingHistory)).toBe(false);
});

async function renderConnect(locale: (typeof SUPPORTED_LOCALES)[number]): Promise<string> {
  const i18n = createInstance();
  await i18n.init({
    defaultNS: "shell",
    fallbackLng: DEFAULT_LOCALE,
    interpolation: { escapeValue: false },
    lng: locale,
    ns: NAMESPACES,
    resources,
  });
  const queryClient = new QueryClient();
  queryClient.setQueryData(SERVICE_STATUS_QUERY_KEY, STATUS);
  return renderToStaticMarkup(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>
        <Connect />
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

function classesOf(markup: string, tag: "dt" | "dd"): string[] {
  return [...markup.matchAll(new RegExp(`<${tag} class="([^"]*)"`, "g"))].map((match) => match[1]);
}

describe("narrow-window shell", () => {
  it("offers a header sidebar trigger below md and closes the mobile sheet on navigation", () => {
    const header = APP_SOURCE.slice(APP_SOURCE.indexOf("<header"), APP_SOURCE.indexOf("</header>"));
    expect(header).toContain('<SidebarTrigger className="-ml-1 md:hidden" />');
    expect(APP_SIDEBAR_SOURCE).toMatch(/setOpenMobile\(false\);\s*onNavigate\(id\);/);
  });

  it("switches Mail to one pane from its own measured width, not the window's", () => {
    expect(MAIL_SOURCE).not.toContain("useSidebar");
    expect(MAIL_SOURCE).toContain('import { useSize } from "radix-ui/internal";');
    expect(MAIL_SOURCE).toContain("const MAIL_SPLIT_MIN_WIDTH = 800;");
    expect(MAIL_SOURCE).toContain("const mailWidth = useSize(mailRoot)?.width;");
    expect(MAIL_SOURCE).toContain("const singlePane = mailWidth !== undefined && mailWidth < MAIL_SPLIT_MIN_WIDTH;");
    expect(MAIL_SOURCE).toContain('<div ref={setMailRoot} className="flex h-full min-h-0 flex-col');
  });

  it("keeps both one-pane Mail views mounted and hands focus between them", () => {
    expect(MAIL_SOURCE).toContain('<div ref={listRef} className={cn("absolute inset-0", selectedThreadId && "invisible")}>{listPane}</div>');
    expect(MAIL_SOURCE).toContain('<div className={cn("absolute inset-0", !selectedThreadId && "invisible")}>{readerPane}</div>');
    expect(MAIL_SOURCE).toMatch(/flushSync\(\(\) => setSelectedThreadId\(threadId\)\);\s*readerBackRef\.current\?\.focus\(\);/);
    expect(MAIL_SOURCE).toMatch(/\.find\(\(row\) => row\.dataset\.threadId === openedId\)\s*\?\.focus\(\);/);
    expect(MAIL_ITEM_LIST_SOURCE).toContain("data-thread-id={thread.id}");
    // Loading and error states keep the back control a loaded thread has.
    expect(MAIL_SOURCE.match(/\{readerBackRow\}/g)).toHaveLength(2);
    expect(MAIL_SOURCE).toContain("backButtonRef={attachReaderBack}");
  });

  it("opens the Mail folder Sheet from its trigger and forgets it when leaving one pane", () => {
    expect(MAIL_SOURCE).toMatch(/<SheetTrigger asChild>\s*<Button[^>]*aria-label=\{t\("folders\.navigationLabel"\)\}>/);
    expect(MAIL_SOURCE).toMatch(/<Sheet open=\{foldersOpen\} onOpenChange=\{setFoldersOpen\}>[\s\S]*<SheetTitle>\{t\("folders\.navigationLabel"\)\}<\/SheetTitle>\s*<SheetDescription>\{t\("folders\.sheetDescription"\)\}<\/SheetDescription>/);
    expect(MAIL_SOURCE).toContain("if (!singlePane) setFoldersOpen(false);");
    expect(MAIL_SOURCE).toMatch(/const changeFolder = \(value: EmailFolder\) => \{\s*setFoldersOpen\(false\);/);
  });

  it("declares the split Mail panes' percentage floors and asks the panel whether it is collapsed", () => {
    expect(MAIL_SOURCE).toContain('<ResizablePanel defaultSize="32%" minSize="30%" maxSize="45%" className="min-w-0">');
    expect(MAIL_SOURCE).toContain('<ResizablePanel minSize="30%" className="min-w-0">');
    expect(MAIL_SOURCE).not.toMatch(/minSize="(?:280|360)px"/);
    expect(MAIL_SOURCE).toContain("setIsCollapsed(navPanelRef.current.isCollapsed())");
    expect(MAIL_SOURCE).not.toContain("inPixels");
  });


  it("wraps every table toolbar instead of letting its buttons run off the page", () => {
    // Both halves held their min-content width, so a narrow page pushed the
    // labelled actions past the right edge and clipped them.
    expect(TABLE_TOOLBAR_SOURCE).toContain('<div className="flex flex-wrap items-center justify-between gap-2">');
    expect(TABLE_TOOLBAR_SOURCE).toContain('<div className="flex flex-wrap items-center gap-2">');
    expect(TABLE_TOOLBAR_SOURCE).toContain('<div className="flex min-w-0 flex-1 flex-col-reverse');
    // The search box may not hold a fixed width at the narrowest sizes.
    expect(TABLE_TOOLBAR_SOURCE).not.toMatch(/className="h-8 w-\[\d+px\]/);
    expect(LEADS_SOURCE).not.toMatch(/className="h-8 w-\[\d+px\]/);
    expect(LEADS_SOURCE).not.toContain('<div className="flex items-center gap-2">\n              <CreateCreatorGroupDialog');
  });


  it("truncates the Mail list header title and count instead of wrapping them", () => {
    const header = MAIL_SOURCE.slice(MAIL_SOURCE.indexOf("const listPane = ("), MAIL_SOURCE.indexOf("<SelectTrigger"));
    expect(header).toContain('<div className="min-w-0 flex-1">');
    expect(header).toContain('<div className="truncate font-semibold">{emailFolderLabel(folder, t)}</div>');
    expect(header).toContain('<div className="truncate text-xs text-muted-foreground">');
    expect(MAIL_SOURCE).toContain('<SelectTrigger size="sm" className="w-[132px] shrink-0"');
  });

  for (const locale of SUPPORTED_LOCALES) {
    it(`wraps a full 64-hex plugin version inside its row and keeps labels on one line in ${locale}`, async () => {
      const markup = await renderConnect(locale);
      expect(markup).toContain(`>${PLUGIN_VERSION}</dd>`);
      expect(markup).toContain(`>${resources[locale].shell.service.versionLabel}</dt>`);

      const labels = classesOf(markup, "dt");
      const values = classesOf(markup, "dd");
      expect(labels).toHaveLength(7);
      expect(values).toHaveLength(7);
      for (const label of labels) expect(label.split(" ")).toEqual(expect.arrayContaining(["shrink-0", "whitespace-nowrap"]));
      for (const value of values) expect(value.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "text-right", "wrap-anywhere"]));
      expect(values.join(" ")).not.toContain("truncate");
    });
  }
});
