import { describe, expect, it } from "vitest";
import { gmailAuthorizationComplete } from "./gmail-connection";

describe("Gmail authorization admission", () => {
  const scopes = ["https://www.googleapis.com/auth/gmail.send", "https://www.googleapis.com/auth/gmail.readonly"];

  it("requires both scopes and both capabilities for a connected authorization", () => {
    for (const can_send of [false, true]) {
      for (const can_read_thread of [false, true]) {
        for (const granted of [[], scopes.slice(0, 1), scopes.slice(1), scopes]) {
          expect(gmailAuthorizationComplete({ status: "connected", scopes: granted, capabilities: { can_send, can_read_thread } }))
            .toBe(can_send && can_read_thread && granted.length === 2);
        }
      }
    }
  });

  it("rejects disconnected authorization even with complete permissions", () => {
    expect(gmailAuthorizationComplete({ status: "disconnected", scopes, capabilities: { can_send: true, can_read_thread: true } })).toBe(false);
  });
});
