import type { OperatorConnectionStatus } from "@/lib/operator-account";

export type TelegramLoginStage = "creds" | "qr" | "code" | "password";

export function telegramLoginView(status: OperatorConnectionStatus): {
  step: 0 | 1;
  stage: TelegramLoginStage;
} {
  switch (status.handoff?.kind) {
    case "scan_telegram_qr_in_desktop":
      return { step: 1, stage: "qr" };
    case "enter_telegram_otp_in_desktop":
      return { step: 1, stage: "code" };
    case "enter_telegram_two_factor_in_desktop":
      return { step: 1, stage: "password" };
    default:
      return {
        step: status.phase === "awaiting_secret_in_app" ? 0 : 1,
        stage: "creds",
      };
  }
}
