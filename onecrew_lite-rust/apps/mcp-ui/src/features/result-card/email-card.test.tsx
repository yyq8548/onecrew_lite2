import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ResultCardView } from "@mcp-ui/features/result-card/view";
import {
  emailFieldName,
  emailInitial,
  emailParty,
  emailPreview,
} from "@mcp-ui/features/result-card/email-preview";
import { parseResultSnapshot } from "@mcp-ui/lib/result-contract";
import {
  MANUAL_EMAIL_BODY,
  escapedManualEmailSnapshot,
  manualEmailSnapshot,
} from "@mcp-ui/test-support/email-fixture";
import { creatorSavedSnapshot } from "@mcp-ui/test-support/creator-save-fixture";
import { SNAPSHOT, choiceSnapshot } from "@mcp-ui/test-support/fixtures";

const MAIL_SOURCES = import.meta.glob([
  "/src/features/result-card/email-preview.ts",
  "/src/features/result-card/components/email-preview.tsx",
], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
// A stylesheet is read from disk: the bundler rewrites CSS it imports, and this
// assertion is about the source the bundle is built from.
const THEME_CSS = readFileSync(fileURLToPath(new URL("../../styles/theme.css", import.meta.url)), "utf8");

const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };
const render = (snapshot: Parameters<typeof ResultCardView>[0]["snapshot"]) =>
  renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));

describe("prepared message preview", () => {
  it("keeps the fixture a valid typed snapshot so the preview is derived from real wire shape", () => {
    expect(parseResultSnapshot(manualEmailSnapshot())).toEqual(manualEmailSnapshot());
    expect(parseResultSnapshot(escapedManualEmailSnapshot())).toEqual(escapedManualEmailSnapshot());
  });

  it("recognizes a message only when the projection names both a recipient and a body", () => {
    const model = emailPreview(manualEmailSnapshot().presentation);
    expect(model).not.toBeNull();
    expect(model!.from).toEqual({ name: "堰离 (River)", address: "river_fan@digifinex.org" });
    expect(model!.to).toEqual(["test@oncrewai.io"]);
    expect(model!.subject).toBe("Greetings from GoAI");
    expect(model!.body).toBe(MANUAL_EMAIL_BODY);

    // Every other projection keeps the labelled table it already had.
    expect(emailPreview(SNAPSHOT.presentation)).toBeNull();
    expect(emailPreview(creatorSavedSnapshot().presentation)).toBeNull();
    expect(emailPreview(choiceSnapshot().presentation)).toBeNull();

    const withoutBody = manualEmailSnapshot().presentation;
    const fields = withoutBody.sections[0]!;
    if (fields.kind !== "fields") throw new Error("fixture shape");
    expect(emailPreview({ ...withoutBody, sections: [{ ...fields, fields: fields.fields.filter(field => field.label !== "Text") }] })).toBeNull();
    expect(emailPreview({ ...withoutBody, sections: [{ ...fields, fields: fields.fields.filter(field => field.label !== "To") }] })).toBeNull();
  });

  it("splits a party and an array recipient label without losing either half", () => {
    expect(emailParty("Nora Beauty <nora@example.test>")).toEqual({ name: "Nora Beauty", address: "nora@example.test" });
    expect(emailParty("nora@example.test")).toEqual({ name: null, address: "nora@example.test" });
    expect(emailParty("OneCrew Outreach")).toEqual({ name: "OneCrew Outreach", address: null });
    expect(emailFieldName("To[2]")).toBe("to");
    expect(emailInitial({ name: "堰离", address: null })).toBe("堰");
    expect(emailInitial(null)).toBe("?");
  });

  it("reads as a message instead of a field table, and keeps internals in the details layer", () => {
    const html = render(manualEmailSnapshot());
    expect(html).toContain("mail-preview");
    expect(html).toContain("mail-avatar");
    expect(html).toContain("堰离 (River)");
    expect(html).toContain("&lt;river_fan@digifinex.org&gt;");
    expect(html).toContain("收件人：");
    expect(html).toContain("test@oncrewai.io");
    expect(html).toContain("查看详情");
    // The flattened confirmation table is gone from the collapsed card.
    expect(html).not.toContain("Idempotency Key");
    expect(html).not.toContain("Mailbox Ref Id");
    expect(html).not.toContain("Review every submitted value");
    expect(html).not.toContain("greeting-01a0a82b");
  });

  it("renders the body with its own line breaks and never as markup", () => {
    const html = render(manualEmailSnapshot());
    expect(html).toContain("Hi River,\n\nI hope you");
    expect(html).not.toContain("Hi River,\\n\\nI hope");
    expect(html).toContain("mail-body");
    expect(THEME_CSS).toContain(".mail-body{margin:0;padding-top:12px;border-top:1px solid var(--border);font-size:14px;line-height:1.7;white-space:pre-wrap");

    // A body that arrived already JSON-escaped — as the adapter still sends a
    // value no card can display — is shown exactly as received: this App does
    // not unescape one, because that would claim content it was never given.
    const escaped = render(escapedManualEmailSnapshot());
    expect(escaped).toContain("Hi River,\\n\\nI hope you");

    for (const source of Object.values(MAIL_SOURCES)) {
      expect(source).not.toContain("dangerouslySetInnerHTML");
    }
  });

  it("keeps the confirmation semantics of the card it replaces", () => {
    const html = render(manualEmailSnapshot());
    expect(html).toContain("确认");
    expect(html).toContain(">取消</button>");
    expect(html).toContain("修改需求");
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
    // The approval prompt is the confirm button's own meaning. Repeating the
    // server's sentence beside it said the same thing twice, in English, on a
    // card whose every other word was the reader's language.
    expect(html).not.toContain("Send the prepared manual email to the explicitly selected address?");

    const terminal = manualEmailSnapshot();
    terminal.pending_action = null;
    terminal.state = { kind: "resolved", code: "completed", summary: "邮件已发送。" };
    const done = render(terminal);
    expect(done).not.toContain("data-result-actions");
    expect(done).not.toContain("确认发送");
  });

  it("swaps details in for the body instead of growing the card, and ships none while collapsed", () => {
    const component = MAIL_SOURCES["/src/features/result-card/components/email-preview.tsx"]!;
    // An overlay would fight the Host's own modal and gesture handling: this
    // App is a component inside that surface, not the foreground application.
    for (const forbidden of ["createPortal", "document.body", 'role="dialog"', "aria-modal", "<dialog", "showModal", "popover=", "requestFullscreen", "position:fixed"]) {
      expect(component).not.toContain(forbidden);
    }
    expect(THEME_CSS).not.toContain(".mail-detail-dialog");
    expect(THEME_CSS).not.toContain(".mail-detail-scrim");
    expect(THEME_CSS).toContain(".mail-details{display:grid;");
    // Details reach the reader through the shared detail view, which replaces
    // the body. Opening them in place added height, and this Host keeps its own
    // frame height, so that height pushed the message and the action bar out.
    expect(component).toContain("openView");

    // Collapsed is the rendered default, and it carries no internals at all —
    // not hidden in the markup, simply not mounted.
    const html = render(manualEmailSnapshot());
    expect(html).not.toContain("mail-details");
    expect(html).not.toContain("Idempotency Key");
    // The trigger is a way to a view that replaces the body, not a disclosure
    // that unfolds beneath the addressing, so it reports no expanded state.
    const trigger = html.match(/<button[^>]*mail-detail-trigger[^>]*>/)?.[0];
    expect(trigger).toBeDefined();
    expect(trigger).not.toContain("aria-expanded");
    // The decision stays reachable while the details are one step away.
    expect(html.match(/data-result-actions="true"/g)).toHaveLength(1);
  });

  it("shows the whole message rather than paging a letter like a list", () => {
    const component = MAIL_SOURCES["/src/features/result-card/components/email-preview.tsx"]!;
    for (const pager of ["textPages", "usePagedRegion", "listPage", "SearchPlanPages"]) {
      expect(component).not.toContain(pager);
    }
    // The body arrives in one piece: no page counter, no next/previous control.
    const html = render(manualEmailSnapshot());
    expect(html).toContain(MANUAL_EMAIL_BODY.slice(-"GoAI Partnerships".length));
    expect(html).not.toContain("下一页");
    expect(html).not.toContain("上一页");
  });
});
