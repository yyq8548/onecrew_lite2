import { describe, expect, it } from "vitest";
import { safeAccountReturnTo, safeAuthReturnTo } from "../src/auth/return-to";

describe("account continuation allowlist", () => {
  it.each(["overview", "access", "profile", "security", "email", "devices"])("preserves the %s panel and language", panel => {
    const path = `/account?panel=${panel}&lng=en-US`;
    expect(safeAuthReturnTo(path)).toBe(path);
  });
  it.each(["https://evil.test/account", "//evil.test/account", "/account/other", "/account?panel=unknown", "/account?panel=email&panel=security", "/account?redirect=https://evil.test", "/account?lng=xx", "/account?apply=0", "/account?panel=email#x", "/account?", "/account?panel=devices%26redirect%3Devil"])("rejects %s", path => {
    expect(safeAccountReturnTo(path)).toBeNull();
    expect(safeAuthReturnTo(path)).toBeNull();
  });
  it("retains application and opaque download continuations", () => {
    expect(safeAuthReturnTo("/account?apply=1")).toBe("/account?apply=1");
    expect(safeAuthReturnTo(`/d/${"a".repeat(43)}`)).toBe(`/d/${"a".repeat(43)}`);
    expect(safeAuthReturnTo(`/d/${"a".repeat(43)}?panel=email`)).toBeNull();
  });
});
