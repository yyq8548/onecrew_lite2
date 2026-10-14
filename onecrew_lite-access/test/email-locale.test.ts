import { describe, expect, it } from "vitest";

import { resolveEmailLocale } from "../src/auth/email-locale";

describe("transactional email locale negotiation", () => {
  it("honors the selected English request language", () => {
    const request = new Request("https://access.example.test/api/session/login-otp/send", {
      headers: { "Accept-Language": "en-US" },
    });

    expect(resolveEmailLocale(request)).toBe("en-US");
  });

  it("matches regional language tags to the supported template", () => {
    const request = new Request("https://access.example.test/api/session/login-otp/send", {
      headers: { "Accept-Language": "en-GB,en;q=0.8" },
    });

    expect(resolveEmailLocale(request)).toBe("en-US");
  });

  it("uses the Chinese product default for unsupported or missing languages", () => {
    expect(resolveEmailLocale()).toBe("zh-CN");
    const request = new Request("https://access.example.test/api/session/login-otp/send", {
      headers: { "Accept-Language": "fr-FR" },
    });

    expect(resolveEmailLocale(request)).toBe("zh-CN");
  });
});
