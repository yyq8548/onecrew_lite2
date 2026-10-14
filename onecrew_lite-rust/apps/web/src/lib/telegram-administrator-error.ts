const TELEGRAM_OPERATOR_ADMIN_REQUIRED = "telegram_operator_admin_required";

export type TelegramAdministratorError =
  | { kind: "operator_admin_required" }
  | { kind: "raw"; message: string };

export function telegramAdministratorError(error: unknown): TelegramAdministratorError {
  const message = String(error);
  return message.includes(TELEGRAM_OPERATOR_ADMIN_REQUIRED)
    ? { kind: "operator_admin_required" }
    : { kind: "raw", message };
}
