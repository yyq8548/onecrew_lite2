import { createInstance } from "i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";

import { DisconnectedCard } from "@/App";
import APP_SOURCE from "@/App.tsx?raw";
import { DEFAULT_LOCALE, NAMESPACES, resources, SUPPORTED_LOCALES } from "@/i18n/resources";

/** `App` itself subscribes to the runtime store, which server rendering does
 * not support, so the card is rendered directly. It is what `App` shows while
 * this browser holds no local session. */
function renderApp(locale: (typeof SUPPORTED_LOCALES)[number]) {
  const i18n = createInstance();
  void i18n.init({ lng: locale, resources, ns: NAMESPACES, defaultNS: "shell" });
  return renderToStaticMarkup(
    <I18nextProvider i18n={i18n}>
      <DisconnectedCard />
    </I18nextProvider>,
  );
}

describe("the disconnected card", () => {
  it("leads with the command that starts the service, and keeps retry beside it", () => {
    for (const locale of SUPPORTED_LOCALES) {
      const markup = renderApp(locale);
      const service = resources[locale].shell.service;
      expect(markup).toContain(service.disconnectedTitle);
      expect(markup).toContain("onecrew web --no-browser");
      // Anchored on the element boundary: the description also tells the user
      // they may retry, so a bare substring would match that sentence first.
      const copy = markup.indexOf(`>${service.copyCommand}<`);
      const retry = markup.indexOf(`>${service.retry}<`);
      expect(copy).toBeGreaterThan(-1);
      // The card now means the service is unreachable, so the command that
      // starts it leads and retry follows.
      expect(retry).toBeGreaterThan(copy);
    }
  });

  it("keeps retry visually secondary and reports a clipboard the environment withholds", () => {
    // `renderToStaticMarkup` runs no effects, so the outcome text is asserted
    // through the source: both branches must reach a status region.
    expect(APP_SOURCE).toContain('variant="outline"');
    expect(APP_SOURCE).toContain('role="status"');
    expect(APP_SOURCE).toContain("errors.clipboardUnavailable");
    expect(APP_SOURCE).toContain("service.copied");
  });

  it("offers the copy and outcome strings in every locale", () => {
    for (const locale of SUPPORTED_LOCALES) {
      const service = resources[locale].shell.service;
      expect(service.copyCommand.length).toBeGreaterThan(0);
      expect(service.copied.length).toBeGreaterThan(0);
      expect(resources[locale].common.errors.clipboardUnavailable.length).toBeGreaterThan(0);
    }
  });
});

it("names the default locale so the card copy above is asserted in it", () => {
  expect(SUPPORTED_LOCALES).toContain(DEFAULT_LOCALE);
});
