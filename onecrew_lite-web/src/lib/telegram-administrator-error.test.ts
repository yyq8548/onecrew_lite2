import { describe, expect, it } from "vitest";

import {
  telegramAdministratorError,
} from "@/lib/telegram-administrator-error";

describe("Telegram administrator errors", () => {
  it("classifies the stable operator permission error code", () => {
    expect(telegramAdministratorError(
      new Error("telegram permission: telegram_operator_admin_required: CHAT_ADMIN_REQUIRED"),
    )).toEqual({ kind: "operator_admin_required" });
  });

  it("preserves unclassified errors", () => {
    const error = new Error("telegram network unavailable");
    expect(telegramAdministratorError(error)).toEqual({
      kind: "raw",
      message: String(error),
    });
  });
});
