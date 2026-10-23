import i18next from "i18next";
import type { ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import enUSInbox from "@/i18n/locales/en-US/inbox";
import zhCNInbox from "@/i18n/locales/zh-CN/inbox";
import type { OwnerReviewSnapshot } from "@/lib/owner-review";
import { TelegramSaveReview } from "./telegram-save-review";

// The production Dialog portals to document.body, which is intentionally not
// available in this node-only suite. Render the real review body in a simple
// shell so this test still exercises the candidate component and its locale.
vi.mock("@/components/ui/dialog", () => {
  const Shell = ({ children }: { children: ReactNode }) => <div>{children}</div>;
  return { Dialog: Shell, DialogContent: Shell, DialogDescription: Shell, DialogFooter: Shell, DialogHeader: Shell, DialogTitle: Shell };
});

function snapshot(): OwnerReviewSnapshot {
  return {
    binding: { result_ref: "result-1", view: { identity: "view", kind: "confirmation" }, fingerprint: "fingerprint", revision: 1, origin: "test" },
    completed_sections_count: 0,
    state: { kind: "available" },
    pending_action: { kind: "creator_save_batch", review: { rows: [{ row_id: "row-1", preview: {
      display_name: "Admin", platform: "telegram", handle: "admin", profile_url: null, avatar_url: null,
      identity_status: "new", identity_creator_ids: [], notes: null, score: 87,
      assessment: "保留逐人评价原文，其中包含历史数字 87。", follower_counts: {}, expires_at: 4102444800,
      routes: [{ route_id: "route-1", channel: "telegram", kind: "handle", display_value: "@admin", normalized_value: "admin", sources: [], required: true, collision: { status: "none", creator_ids: [] } }],
    } }] } },
    presentation: { title: "Review", summary: null, sections: [] },
  };
}

describe("Telegram candidate score presentation", () => {
  for (const [locale, catalog] of [["zh-CN", zhCNInbox], ["en-US", enUSInbox]] as const) {
    it(`renders a grade while retaining the individual assessment in ${locale}`, async () => {
      const i18n = i18next.createInstance();
      await i18n.init({ lng: locale, ns: ["inbox"], defaultNS: "inbox", resources: { [locale]: { inbox: catalog } }, interpolation: { escapeValue: false } });
      const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><TelegramSaveReview snapshot={snapshot()} onSnapshot={() => {}} onClose={() => {}} onSaved={() => {}} /></I18nextProvider>);

      expect(html).toContain(i18n.t("outreach.telegram.reviewScore", { grade: `87 · ${i18n.t("outreach.telegram.reviewGradeGood")}` }));
      expect(html).toContain("保留逐人评价原文，其中包含历史数字 87。");
      expect(html).not.toContain(i18n.t("outreach.telegram.reviewScore", { score: 87 }));
    });
  }
});
