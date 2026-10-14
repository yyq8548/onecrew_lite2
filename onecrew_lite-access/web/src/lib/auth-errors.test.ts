import { describe, expect, it } from "vitest";

import { getAuthErrorMessage, getCallbackErrorMessage } from "./auth-errors";

describe("auth error mapping", () => {
  it("does not expose unknown server messages", () => {
    expect(getAuthErrorMessage({ code: "UNKNOWN_BACKEND_DETAIL" })).toBe("操作未完成，请稍后重试。");
  });

  it("uses a non-enumerating sign-in error", () => {
    expect(getAuthErrorMessage({ code: "USER_NOT_FOUND" })).toBe("邮箱或密码不正确。");
  });

  it("maps rate limiting by status", () => {
    expect(getAuthErrorMessage({ status: 429 })).toBe("尝试次数过多，请稍后再试。");
  });

  it("translates stable error codes without exposing backend detail", () => {
    expect(getAuthErrorMessage({ code: "USER_NOT_FOUND" }, "en")).toBe(
      "The email or password is incorrect.",
    );
  });

  it("maps callback errors and ignores raw descriptions", () => {
    expect(getCallbackErrorMessage("?error=access_denied&error_description=private-provider-detail")).toBe(
      "Google 登录已取消，你可以重试或改用邮箱登录。",
    );
    expect(getCallbackErrorMessage("?error=unknown&error_description=secret")).toBe(
      "第三方登录未完成，请重新尝试。",
    );
  });
});
