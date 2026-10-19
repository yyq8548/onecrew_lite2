import { describe, expect, it } from "vitest";

import type { OperatorConnectionStatus } from "@/lib/operator-account";

import { telegramLoginView } from "./telegram-flow-state";

function status(
  phase: OperatorConnectionStatus["phase"],
  handoff?: OperatorConnectionStatus["handoff"],
): OperatorConnectionStatus {
  return {
    flow_id: "flow",
    platform: "telegram",
    intent: "connect",
    method: "telegram_phone",
    phase,
    handoff,
    allowed_actions: [],
  };
}

describe("telegram login view", () => {
  it.each([
    ["scan_telegram_qr_in_desktop", "qr"],
    ["enter_telegram_otp_in_desktop", "code"],
    ["enter_telegram_two_factor_in_desktop", "password"],
  ] as const)("maps %s directly to the rendered %s input", (kind, stage) => {
    expect(
      telegramLoginView(
        status("awaiting_secret_in_app", { kind, message: "continue in desktop" }),
      ),
    ).toEqual({ step: 1, stage });
  });

  it("keeps an unconfigured flow on the credential step", () => {
    expect(telegramLoginView(status("awaiting_secret_in_app"))).toEqual({
      step: 0,
      stage: "creds",
    });
  });
});
