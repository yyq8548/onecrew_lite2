import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";

import { mailHistoryIndicator } from "@/components/mail/mail-item-list";

const t = ((key: string, options?: { count?: number }) => (
  key === "list.historyWithSent"
    ? `历史交互 ${options?.count} 封，包含已发送邮件`
    : `历史交互 ${options?.count} 封`
)) as TFunction<"mail">;

describe("mailHistoryIndicator", () => {
  it("hides single-message threads", () => {
    expect(mailHistoryIndicator(1, false, t)).toBeNull();
  });

  it("combines the message count with reply evidence", () => {
    expect(mailHistoryIndicator(5, true, t)).toEqual({
      countLabel: "5",
      accessibleLabel: "历史交互 5 封，包含已发送邮件",
      showReply: true,
    });
  });

  it("caps the visible count without losing the accessible total", () => {
    expect(mailHistoryIndicator(120, false, t)).toEqual({
      countLabel: "99+",
      accessibleLabel: "历史交互 120 封",
      showReply: false,
    });
  });
});
