import { describe, expect, it } from "vitest";
import { gmailAuthorizationComplete, type GmailAuthorization } from "./gmail-connection";

describe("Gmail authorization contract", () => {
  const authorization: GmailAuthorization = { status: "connected", capabilities: { can_send: true, can_read_thread: true }, scopes: ["https://www.googleapis.com/auth/gmail.send", "https://www.googleapis.com/auth/gmail.readonly"] };
  it("requires both granted scopes and corresponding capabilities", () => {
    expect(gmailAuthorizationComplete(authorization)).toBe(true);
    for (const scope of authorization.scopes) expect(gmailAuthorizationComplete({ ...authorization, scopes: [scope] })).toBe(false);
    expect(gmailAuthorizationComplete({ ...authorization, capabilities: { can_send: true, can_read_thread: false } })).toBe(false);
    expect(gmailAuthorizationComplete({ ...authorization, capabilities: { can_send: false, can_read_thread: true } })).toBe(false);
    expect(gmailAuthorizationComplete({ ...authorization, status: "revoked" })).toBe(false);
  });
});
