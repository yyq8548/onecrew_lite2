import i18next from "i18next";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { OpenCliWizard } from "./opencli-wizard";

const state = vi.hoisted(() => ({ version: "1.8.6", issue: "version_incompatible" as string | null }));
vi.mock("@/lib/opencli-setup", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/opencli-setup")>(),
  useOpenCliSetup: () => ({
    data: { installed: true, version: state.version, required_version: "1.8.8", command_path: "/old-install/opencli", command_count: 100, daemon_running: false,
      extension_connected: false, profiles: [], search_capabilities: [], addons: [], issue: state.issue },
    isError: false, isFetching: false, refetch: vi.fn(),
  }),
}));

describe("OpenCLI upgrade guidance", () => {
  for (const [locale, catalog, upgrade] of [["zh-CN", zh, "升级 OpenCLI"], ["en-US", en, "Update OpenCLI"]] as const) {
    it(`provides a precise upgrade path without treating an old CLI as ready in ${locale}`, async () => {
      state.version = "1.8.6";
      state.issue = "version_incompatible";
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}><I18nextProvider i18n={i18n}><OpenCliWizard /></I18nextProvider></QueryClientProvider>);
      expect(markup).toContain("1.8.6");
      expect(markup).toContain("1.8.8");
      expect(markup).toContain(upgrade);
      expect(markup).not.toContain('text-[var(--tone-success-fg)]');
      const chooseLabel = catalog.opencli.chooseProfile;
      expect(markup).toMatch(new RegExp(`<button[^>]*disabled=""[^>]*>${chooseLabel}</button>`));
      expect(markup).not.toContain(catalog.opencli.profilesEmpty);
    });
  }
});
