import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import { CodexConsentStep, OpenCliAddonSetup, OpenCliPlatformCapabilities, OpenCliWizard } from "./opencli-wizard";
import { SEARCH_PLATFORM_GROUPS } from "./discovery-pilot";
import { platformWebsite } from "./platform-websites";

describe("OpenCLI platform connection inventory", () => {
  it("does not report a missing browser identity before the initial status read completes", async () => {
    const i18n = i18next.createInstance();
    await i18n.init({ lng: "zh-CN", resources: { "zh-CN": { shell: zh } } });
    const markup = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}>
      <I18nextProvider i18n={i18n}><OpenCliWizard /></I18nextProvider>
    </QueryClientProvider>);
    expect(markup).toContain(zh.service.checking);
    expect(markup).not.toContain(zh.opencli.profilesEmpty);
  });
  it("provides an HTTPS entry for every supported search platform", () => {
    for (const platform of SEARCH_PLATFORM_GROUPS.flatMap(group => group.platforms)) {
      expect(new URL(platformWebsite(platform)!).protocol).toBe("https:");
    }
    expect(platformWebsite("unknown-platform")).toBeUndefined();
    expect(platformWebsite("xiaohongshu")).toBe("https://www.xiaohongshu.com/");
    expect(platformWebsite("rednote")).toBeUndefined();
    const platforms = SEARCH_PLATFORM_GROUPS.flatMap(group => group.platforms);
    expect(platforms).toContain("xiaohongshu");
    expect(platforms).not.toContain("rednote");
  });
  for (const [locale, catalog] of [["zh-CN", zh], ["en-US", en]] as const) {
    it(`keeps Codex usage acknowledgment separate from model access in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><CodexConsentStep /></I18nextProvider>);
      expect(markup).toContain(catalog.opencli.codexConsentTitle);
      expect(markup).toContain(catalog.opencli.codexConsentCheckbox);
      expect(markup).toContain(catalog.opencli.codexConsentBoundary);
      expect(markup).toContain('role="checkbox"');
      expect(markup).toContain('disabled=""');
      expect(markup).not.toContain(catalog.opencli.codexConsentRecorded);
      expect(markup).toContain(catalog.opencli.codexPluginState.unknown);
      expect(markup).toContain(catalog.opencli.codexPluginOpenGuide);
      const fresh = renderToStaticMarkup(<I18nextProvider i18n={i18n}><CodexConsentStep plugin={{ state: "installed", enabled: true, verification: "fresh", installed_version: "1", last_verified_at: 1, diagnostic_code: null }} onCheckPlugin={() => {}} /></I18nextProvider>);
      expect(fresh).toContain(catalog.opencli.codexPluginState.enabledFresh);
      expect(fresh).toContain(catalog.opencli.codexPluginRecheck);
      const cached = renderToStaticMarkup(<I18nextProvider i18n={i18n}><CodexConsentStep plugin={{ state: "installed", enabled: true, verification: "cached", installed_version: "1", last_verified_at: 1, diagnostic_code: null }} /></I18nextProvider>);
      expect(cached).toContain(catalog.opencli.codexPluginState.enabledCached);
      const checking = renderToStaticMarkup(<I18nextProvider i18n={i18n}><CodexConsentStep pluginCheckPending /></I18nextProvider>);
      expect(checking).toContain(catalog.opencli.codexPluginChecking);
      const old = renderToStaticMarkup(<I18nextProvider i18n={i18n}><CodexConsentStep plugin={{ state: "unavailable", enabled: true, verification: "fresh", installed_version: "old", last_verified_at: 1, diagnostic_code: "codex_plugin_update_required" }} /></I18nextProvider>);
      expect(old).toContain(catalog.opencli.codexPluginUpdateRequired);
      expect(old).toContain(catalog.opencli.codexPluginUpgradeHelp);
      expect(old).not.toContain(catalog.opencli.codexPluginState.enabledFresh);
    });
    it(`renders explicit installation and fail-closed states in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      let calls = 0;
      const render = (state: "missing" | "ready" | "conflict" | "unavailable", pending = false, failed = false) => renderToStaticMarkup(
        <I18nextProvider i18n={i18n}><OpenCliAddonSetup report={{ version: "1.8.8", addons: [{ id: "onecrew-site-auth", state: "ready" }, { id: "onecrew-xiaohongshu", state: "ready" }, { id: "onecrew-youtube", state: "ready" }, { id: "onecrew-tiktok", state: "ready" }, { id: "onecrew-bilibili", state: "ready" }, { id: "onecrew-tiktok-profile", state: "ready" }, { id: "onecrew-bilibili-profile", state: "ready" }, { id: "onecrew-bilibili-video", state: "ready" }, { id: "onecrew-twitter-identity", state: "ready" }, { id: "onecrew-twitter-post-identity", state: "ready" }, { id: "onecrew-twitter-post-content", state: "ready" }, { id: "onecrew-browser-search", state }] }} pending={pending} failed={failed} onInstall={() => { calls += 1; }} /></I18nextProvider>,
      );
      expect(render("missing")).toContain(catalog.opencli.installAddons);
      expect(render("missing")).toContain(catalog.opencli.addonsDetails);
      expect(render("missing")).toContain('aria-expanded="false"');
      expect(render("missing")).not.toContain("onecrew-bilibili");
      expect(render("missing")).toContain(catalog.opencli.addonsScope.replaceAll("'", "&#x27;"));
      expect(render("missing")).not.toContain('disabled=""');
      expect(render("missing", true)).toContain('disabled=""');
      expect(render("ready")).toContain(catalog.opencli.addonsInstalled);
      expect(render("ready")).not.toContain(catalog.opencli.installAddons);
      expect(render("unavailable")).toContain('disabled=""');
      expect(render("conflict")).not.toContain('disabled=""');
      expect(render("conflict")).toContain(catalog.opencli.repairAddons);
      expect(render("missing", false, true)).toContain(catalog.opencli.addonsFailed);
      expect(calls).toBe(0); // Rendering/checking never installs.
    });
    it(`distinguishes detected, missing, and public commands in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const render = (capabilities: Parameters<typeof OpenCliPlatformCapabilities>[0]["capabilities"]) => renderToStaticMarkup(
        <QueryClientProvider client={new QueryClient()}><I18nextProvider i18n={i18n}><OpenCliPlatformCapabilities capabilities={capabilities} /></I18nextProvider></QueryClientProvider>,
      );
      const markup = render([
        { platform: "tiktok", command: "onecrew-tiktok/search", browser_required: true, registered: true },
        { platform: "instagram", command: "instagram/search", browser_required: true, registered: false },
        { platform: "substack", command: "substack/search", browser_required: false, registered: true },
      ]);
      expect(markup).not.toContain(catalog.opencli.localCheckColumn);
      expect(markup).toContain(catalog.opencli.authPublicRead);
      expect(markup).toContain(catalog.opencli.commandMissing);
      expect(markup).not.toContain(catalog.opencli.fieldCheckColumn);
      expect(markup).not.toContain(catalog.opencli.accessColumn);
      expect(markup).toContain(catalog.opencli.platformsLoginHelp.replaceAll("'", "&#x27;"));
      expect(markup).toContain(catalog.opencli.authNotChecked);
      expect(markup).toContain(catalog.opencli.authPublicRead);
      expect(markup).not.toContain("onecrew-tiktok/search");
      expect(markup).not.toContain("tiktok/onecrew-tiktok/search");
      expect(markup).toContain(catalog.opencli.authCheckAction);
      expect(markup).toContain(catalog.opencli.authCheckAll);
      expect(markup).not.toContain('href="https://www.tiktok.com/"');
      expect(markup).not.toContain('href="https://www.instagram.com/"');
      expect(markup).toContain(catalog.opencli.platformLogin);
      expect(markup).toContain(catalog.opencli.platformOpen);
      expect(markup).toContain('disabled=""'); // A site read needs a selected connected profile and a click.
      expect(render([])).toContain(catalog.opencli.platformsUnavailable);
    });
    it(`keeps a stopped field test distinct from current command registration in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["shell"], defaultNS: "shell", resources: { [locale]: { shell: catalog } }, interpolation: { escapeValue: false } });
      const markup = renderToStaticMarkup(<QueryClientProvider client={new QueryClient()}><I18nextProvider i18n={i18n}><OpenCliPlatformCapabilities capabilities={[
        { platform: "linkedin-salesnav", command: "linkedin/salesnav-search", browser_required: true, registered: true },
        { platform: "bilibili", command: "bilibili/search", browser_required: true, registered: true },
        { platform: "google-images", command: "google/images", browser_required: true, registered: true },
        { platform: "future-entry", command: "future/search", browser_required: true, registered: true },
      ]} /></I18nextProvider></QueryClientProvider>);
      expect(markup).toContain(catalog.opencli.searchAccess.extra_entitlement);
      expect(markup).toContain(catalog.opencli.authNotChecked);
      expect(markup).toContain(catalog.opencli.authPublicBrowser);
      expect(markup).toContain(catalog.opencli.searchAccess.conditional);
      expect(markup).not.toContain(catalog.opencli.historicalAccess.unknown);
      expect(markup).not.toContain(catalog.opencli.evidenceNotTested);
      expect(markup).not.toContain(catalog.opencli.commandDetected);
    });
  }
});
