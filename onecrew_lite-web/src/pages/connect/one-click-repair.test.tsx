import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";
import { createInstance } from "i18next";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { OneClickRepair } from "./one-click-repair";

describe("两个页面的修复入口", () => {
  for (const page of ["connections", "search"] as const) for (const language of ["zh-CN", "en-US"]) {
    it(`${page} · ${language} 显示明确的修复按钮且渲染不触发修复`, async () => {
      const i18n = createInstance();
      await i18n.init({ lng: language, fallbackLng: false, resources: { "zh-CN": { shell: zh }, "en-US": { shell: en } } });
      const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const html = renderToStaticMarkup(<QueryClientProvider client={client}><I18nextProvider i18n={i18n}>
        <OneClickRepair page={page} credentialId={null}><span>English</span></OneClickRepair>
      </I18nextProvider></QueryClientProvider>);
      expect(html).toContain(language === "zh-CN" ? "一键修复" : "One-click repair");
      expect(html).toContain('aria-expanded="false"');
      expect(html).toContain('aria-controls="onecrew-repair-panel"');
      expect(client.getQueryCache().getAll().every(query => query.state.fetchStatus === "idle")).toBe(true);
      client.clear();
    });
  }
});
