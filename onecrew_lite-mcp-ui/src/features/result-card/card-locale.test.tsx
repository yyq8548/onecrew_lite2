import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { MANUAL_EMAIL_BODY, manualEmailSnapshot } from "@mcp-ui/test-support/email-fixture";

const handlers = { onConfirm: vi.fn(), onSelect: vi.fn(), onCancel: vi.fn(), onRequestEdit: vi.fn() };
const render = (snapshot: Parameters<typeof ResultCardView>[0]["snapshot"]) =>
  renderToStaticMarkup(createElement(ResultCardView, { snapshot, busy: false, ...handlers }));

/** The card carries the locale the Agent chose for the conversation, and the
 * whole card must follow it. The confirmation a user reads first is projected
 * before that locale exists — the operation title comes from the capability
 * catalog, which has one fixed English string per operation — so the App is
 * what makes the card speak one language instead of two. */
describe("a confirmation card in the conversation's language", () => {
  it("leaves no English chrome on a Chinese card", () => {
    const html = render(manualEmailSnapshot());
    expect(html).toContain("发送邮件");
    expect(html).not.toContain("Send manual email");
    expect(html).not.toContain(">确认<");
    expect(html).not.toContain(">Confirm<");
  });

  it("keeps an English conversation in English", () => {
    // Chinese is not the default: the Agent picks zh-CN for Chinese
    // conversations and en-US for the rest, and both must be honoured.
    const html = render({ ...manualEmailSnapshot(), locale: "en-US" });
    expect(html).toContain("Send email");
    expect(html).not.toContain(">Confirm<");
    expect(html).not.toContain("发送邮件");
    expect(html).not.toContain("确认");
  });

  it("translates the argument labels the projection derived, never their values", () => {
    const snapshot = manualEmailSnapshot();
    const html = render(snapshot);
    // The body and the address are the operation's own data. Translating
    // either would change what the user is approving. (The body is compared on
    // fragments free of HTML-escaped characters.)
    expect(html).toContain("Best regards,");
    expect(html).toContain(MANUAL_EMAIL_BODY.slice(0, 9));
    expect(html).toContain("test@oncrewai.io");
    // The internals stay out of the collapsed card entirely — neither their
    // English labels nor their values ship until the details are expanded.
    expect(html).not.toContain("Idempotency Key");
    expect(html).not.toContain("greeting-01a0a82b");
  });

  it("does not restore an owner action label on historical sending cards", () => {
    // Outreach sends a specific confirm label. Only the generic English one is
    // known copy; anything an owner wrote deliberately must survive untouched.
    const snapshot = manualEmailSnapshot();
    if (snapshot.pending_action?.kind !== "confirm") throw new Error("fixture shape");
    snapshot.pending_action.label = "确认执行";
    expect(render(snapshot)).not.toContain("确认执行");
  });

  it("names the operation only while it is still asking for approval", () => {
    const terminal = manualEmailSnapshot();
    terminal.pending_action = null;
    terminal.state = { kind: "resolved", code: "completed", summary: "邮件已发送。" };
    // A finished card reports its outcome; it is no longer "send email".
    expect(render(terminal)).not.toContain("发送邮件");
  });
});
