import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";

import APP_SOURCE from "@/App.tsx?raw";
import { AppErrorBoundary } from "@/components/app-error-boundary";
import APP_ERROR_SOURCE from "@/components/app-error-boundary.tsx?raw";
import { RuntimeUnavailable } from "@/components/runtime-unavailable";
import RUNTIME_UNAVAILABLE_SOURCE from "@/components/runtime-unavailable.tsx?raw";
import COLUMN_HEADER_SOURCE from "@/components/data-table/column-header.tsx?raw";
import DATA_TABLE_SOURCE from "@/components/data-table/data-table.tsx?raw";
import DATE_FILTER_SOURCE from "@/components/data-table/date-filter.tsx?raw";
import FACETED_FILTER_SOURCE from "@/components/data-table/faceted-filter.tsx?raw";
import PAGINATION_SOURCE from "@/components/data-table/pagination.tsx?raw";
import TOOLBAR_SOURCE from "@/components/data-table/toolbar.tsx?raw";
import VIEW_OPTIONS_SOURCE from "@/components/data-table/view-options.tsx?raw";
import GETTING_STARTED_SOURCE from "@/components/GettingStarted.tsx?raw";
import PROVIDER_ALERT_SOURCE from "@/components/provider-error-alert.tsx?raw";
import DIALOG_SOURCE from "@/components/ui/dialog.tsx?raw";
import REFRESH_BUTTON_SOURCE from "@/components/ui/refresh-button.tsx?raw";
import SHEET_SOURCE from "@/components/ui/sheet.tsx?raw";
import SIDEBAR_PRIMITIVE_SOURCE from "@/components/ui/sidebar.tsx?raw";
import { appI18n } from "@/i18n";
import { resources } from "@/i18n/resources";
import CONNECT_SOURCE from "@/lib/connect.ts?raw";
import { creatorInvoke } from "@/lib/creator";
import CREATOR_SOURCE from "@/lib/creator.ts?raw";
import LOCAL_DATA_SOURCE from "@/lib/local-data.ts?raw";
import {
  failureMessage,
} from "@/lib/local-data";
import { presentProviderError } from "@/lib/provider-error";
import PROVIDER_ERROR_SOURCE from "@/lib/provider-error.ts?raw";
import RUNTIME_QUERY_SOURCE from "@/lib/runtime-query.ts?raw";

function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
}

function leafPaths(value: unknown, prefix = ""): string[] {
  if (typeof value === "string") return [prefix];
  if (typeof value !== "object" || value == null) return [];
  return Object.entries(value).flatMap(([key, child]) =>
    leafPaths(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe("shared shell translations", () => {
  it("keeps the common and shell locale catalogs in strict parity", () => {
    for (const namespace of ["common", "shell"] as const) {
      expect(leafPaths(resources["en-US"][namespace]).sort()).toEqual(
        leafPaths(resources["zh-CN"][namespace]).sort(),
      );
    }
  });

  it("routes fixed shared UI copy through common or shell catalogs", () => {
    const fixedUiSources = [
      APP_SOURCE,
      APP_ERROR_SOURCE,
      RUNTIME_UNAVAILABLE_SOURCE,
      COLUMN_HEADER_SOURCE,
      DATA_TABLE_SOURCE,
      DATE_FILTER_SOURCE,
      FACETED_FILTER_SOURCE,
      PAGINATION_SOURCE,
      TOOLBAR_SOURCE,
      VIEW_OPTIONS_SOURCE,
      GETTING_STARTED_SOURCE,
      PROVIDER_ALERT_SOURCE,
      DIALOG_SOURCE,
      REFRESH_BUTTON_SOURCE,
      SHEET_SOURCE,
      SIDEBAR_PRIMITIVE_SOURCE,
      CONNECT_SOURCE,
      CREATOR_SOURCE,
      LOCAL_DATA_SOURCE,
      PROVIDER_ERROR_SOURCE,
      RUNTIME_QUERY_SOURCE,
    ].map(stripComments);

    for (const source of fixedUiSources) {
      expect(source).not.toMatch(/[\u3400-\u9fff]/);
    }
    expect(APP_SOURCE).toContain('useTranslation("shell")');
    expect(GETTING_STARTED_SOURCE).toContain('useTranslation("shell")');
    expect(DATA_TABLE_SOURCE).toContain('useTranslation("common")');
    expect(REFRESH_BUTTON_SOURCE).toContain("useTranslation('common')");
    expect(APP_SOURCE).not.toContain("setSuccess(result.message)");
    expect(DIALOG_SOURCE).toContain("t('actions.close')");
    expect(SHEET_SOURCE).toContain("t('actions.close')");
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain('t("sidebar.mobileDescription")');
    expect(SIDEBAR_PRIMITIVE_SOURCE).toContain('aria-label={t("sidebar.toggle")}');
  });

  it("localizes helper-owned presentation while preserving runtime detail", async () => {
    const previousLanguage = appI18n.resolvedLanguage ?? "zh-CN";
    if (!appI18n.isInitialized) {
      await appI18n.init({
        fallbackLng: "zh-CN",
        interpolation: { escapeValue: false },
        lng: "en-US",
        resources,
      });
    } else {
      await appI18n.changeLanguage("en-US");
    }

    try {
      expect(failureMessage("busy", false)).toContain("Quit other app instances");

      expect(appI18n.t("service.connected", { ns: "shell" })).toBe("Local service connected");

      const raw = "HTTP 429 from provider account-42";
      const presentation = presentProviderError(raw);
      expect(presentation.summary).toBe("The service did not return a verifiable diagnostic.");
      expect(presentation.details).toEqual([]);

      await expect(creatorInvoke("onecrew_i18n_contract_test")).rejects.toThrow(
        resources["en-US"].common.runtimeUnavailable.description,
      );

      const boundary = new AppErrorBoundary({ children: null });
      boundary.state = { failed: true, message: "RAW_RENDER_ERROR" };
      const errorMarkup = renderToStaticMarkup(boundary.render());
      expect(errorMarkup).toContain("Page failed to load");
      expect(errorMarkup).toContain("RAW_RENDER_ERROR");

      const markup = renderToStaticMarkup(
        createElement(
          I18nextProvider,
          { i18n: appI18n },
          createElement(RuntimeUnavailable),
        ),
      );
      expect(markup).toContain("Local onecrew_lite service unavailable");
      expect(markup).toContain("Open workbench");
    } finally {
      await appI18n.changeLanguage(previousLanguage);
    }
  });
});
