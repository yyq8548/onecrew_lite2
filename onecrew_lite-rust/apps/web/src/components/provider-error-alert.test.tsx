import { beforeEach, describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { appI18n } from "@/i18n";
import { resources } from "@/i18n/resources";
import type { DmFailure } from "@/lib/provider-error";
import { ProviderErrorAlert } from "./provider-error-alert";

const failure: DmFailure = { stage: "message_readback", diagnostic: {
  platform: "telegram", protocol: "telegram.mtproto", origin: "provider",
  code: "UNRECOGNIZED_RPC", http_status: null, rpc_code: 500,
  message: "The provider readback is unavailable.",
} };

beforeEach(async () => {
  if (!appI18n.isInitialized) await appI18n.init({ lng: "zh-CN", fallbackLng: "zh-CN", resources });
  else await appI18n.changeLanguage("zh-CN");
});

describe("provider diagnostic display", () => {
  it("renders native evidence under the caller's scope without offering an inferred retry", () => {
    const markup = renderToStaticMarkup(<I18nextProvider i18n={appI18n}>
      <ProviderErrorAlert failure={failure} scope="已受理；回读尚未完成" variant="warning" />
    </I18nextProvider>);
    for (const text of ["已受理；回读尚未完成", "实际协议", "telegram.mtproto", "message_readback", "UNRECOGNIZED_RPC", "500"]) {
      expect(markup).toContain(text);
    }
    expect(markup).not.toContain("<button");
    expect(markup).not.toContain("未发送");
  });

  it("localizes labels while preserving protocol, stage and diagnostic content", async () => {
    await appI18n.changeLanguage("en-US");
    const markup = renderToStaticMarkup(<I18nextProvider i18n={appI18n}>
      <ProviderErrorAlert error={{ code: "read_failed", message: failure.diagnostic.message, failure }} scope="Thread read" />
    </I18nextProvider>);
    for (const text of ["Actual protocol", "Native error code", "telegram.mtproto", "message_readback", failure.diagnostic.message]) {
      expect(markup).toContain(text);
    }
    expect(markup).not.toContain("实际协议");
  });

  it("does not publish raw unstructured causes in the detail disclosure", () => {
    const markup = renderToStaticMarkup(<I18nextProvider i18n={appI18n}>
      <ProviderErrorAlert error={new Error("login token=SENTINEL_PRIVATE_VALUE")} scope="Permission check" />
    </I18nextProvider>);
    expect(markup).not.toContain("SENTINEL_PRIVATE_VALUE");
    expect(markup).not.toContain("<details");
    expect(markup).toContain("服务未返回可验证的诊断信息");
  });
});
