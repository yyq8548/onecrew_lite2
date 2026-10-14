import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ResultCardView } from "./view";
import { resultStatusPresentation } from "./presentation";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";
import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";

function failed(locale: "zh-CN" | "en-US", code = "presentation_unavailable"): ResultSnapshot {
  return {
    ...structuredClone(SNAPSHOT), locale, pending_action: null,
    state: { kind: "failed", code, summary: "Owner's safe diagnostic", retryable: false },
    presentation: { ...SNAPSHOT.presentation, title: "Presentation failed", summary: null, sections: [] },
  };
}

describe("presentation limits are not business failures", () => {
  it.each([
    ["zh-CN", "结果未知，展示受限", "尚未确认"],
    ["en-US", "Outcome unknown, presentation limited", "unconfirmed"],
  ] as const)("keeps uncertainty explicit in the %s display-limited view", (locale, title, detail) => {
    const snapshot: ResultSnapshot = { ...failed(locale), state: {
      kind: "unknown", code: "presentation_unavailable", summary: "Some outcomes remain unconfirmed",
    } };
    const status = resultStatusPresentation(snapshot);
    expect(status.terminal?.label).toBe(title);
    expect(status.terminal?.detail).toContain(detail);
    expect(status.terminal?.detail).not.toContain("操作已执行完成");
    expect(status.badge.variant).toBe("outline");
  });
  it.each([
    ["zh-CN", "展示受限", "不要重新执行"],
    ["en-US", "Presentation limited", "Do not repeat"],
  ] as const)("renders the distinction on the real %s card", (locale, title, recovery) => {
    const snapshot = failed(locale);
    const status = resultStatusPresentation(snapshot);
    expect(status.terminal?.label).toBe(title);
    expect(status.badge.variant).toBe("outline");
    const html = renderToStaticMarkup(createElement(ResultCardView, {
      snapshot, busy: false,
      onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn(),
    }));
    expect(html).toContain(title);
    expect(html).toContain(recovery);
    expect(html).not.toContain(locale === "zh-CN" ? "处理失败" : "Operation failed");
  });

  it("keeps actual business failures distinct with the owner's explanation", () => {
    const status = resultStatusPresentation(failed("zh-CN", "provider_operation_failed"));
    expect(status.terminal?.label).toBe("处理失败");
    expect(status.terminal?.detail).toBe("Owner's safe diagnostic");
    expect(status.badge.variant).toBe("destructive");
  });
});
