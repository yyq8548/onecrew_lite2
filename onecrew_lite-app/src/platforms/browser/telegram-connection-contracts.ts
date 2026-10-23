import { createHash } from "node:crypto";
import { z } from "zod";

const text = z.string().refine(value => value.isWellFormed());
export const telegramConnectionProxy = z.object({ kind: z.enum(["http", "https", "socks5"]), host: text, port: z.number().int().min(0).max(65535), username: text.nullish().default(null), password: text.nullish().default(null) });
export const telegramConnectionStart = z.strictObject({ api_id: z.number().int().min(-2147483648).max(2147483647), api_hash: text, proxy: telegramConnectionProxy, method: z.enum(["qr", "phone"]).default("qr"), phone: text.nullish().default(null), idempotency_key: text });
export type TelegramConnectionStart = z.infer<typeof telegramConnectionStart>;
export function validateTelegramConnectionStart(raw: unknown): TelegramConnectionStart {
  const input = telegramConnectionStart.parse(raw);
  if (!input.idempotency_key.trim() || Buffer.byteLength(input.idempotency_key.trim()) > 128) throw new Error("idempotency_key must contain 1 to 128 characters");
  if (input.api_id <= 0 || !input.api_hash.trim()) throw new Error("Telegram api_id and api_hash are required");
  if (input.proxy.kind !== "socks5") throw new Error("Telegram account connection requires a SOCKS5 proxy");
  if (input.phone === "") input.phone = null;
  if (input.method === "phone" && input.phone === null) throw new Error("Telegram phone login requires a phone number");
  if (input.phone !== null && !/^\+[0-9]{8,15}$/u.test(input.phone)) throw new Error("Telegram phone must use strict E.164 format");
  return input;
}
export function telegramConnectionFingerprint(input: TelegramConnectionStart): string {
  const hash = createHash("sha256");
  const field = (value: Uint8Array) => { const length = Buffer.alloc(8); length.writeBigUInt64BE(BigInt(value.length)); hash.update(length).update(value); };
  const string = (value: string) => field(Buffer.from(value));
  const optional = (value: string | null) => { hash.update(Buffer.of(value === null ? 0 : 1)); if (value !== null) string(value); };
  string("direct-telegram-account-v1"); const api = Buffer.alloc(4); api.writeInt32BE(input.api_id); field(api); string(input.api_hash);
  string(input.proxy.kind); string(input.proxy.host); const port = Buffer.alloc(2); port.writeUInt16BE(input.proxy.port); field(port); optional(input.proxy.username); optional(input.proxy.password);
  string(input.method); optional(input.phone); return hash.digest("hex");
}
export const telegramConnectionOperation = z.strictObject({ operation_id: text });
export const telegramConnectionOtp = telegramConnectionOperation.extend({ otp: text });
export const telegramConnectionPassword = telegramConnectionOperation.extend({ password: text });
export const telegramConnectionCancel = telegramConnectionOperation.extend({ confirmed: z.boolean() });
export type TelegramConnectionPhase = "starting" | "awaiting_telegram_qr_scan" | "awaiting_telegram_otp" | "awaiting_telegram_two_factor" | "completed" | "canceled" | "failed";
export interface TelegramConnectionStatus { operation_id: string; phase: TelegramConnectionPhase; expires_at: number; qr_code?: { expires_at: number }; code_delivery?: TelegramCodeDelivery; operator_account?: unknown; error_code?: string }
export interface TelegramCodeDelivery { method: string; code_length: number | null; destination_hint: string | null; timeout_seconds: number | null; next_method: string | null; action_url: string | null; can_submit_code: boolean }
export function assertTelegramSecretStep(state: { phase: string; method: string; hasClient: boolean; hasInput: boolean; hasCode: boolean; hasPasswordWaiter: boolean }, password: boolean): void {
  if (state.phase !== "awaiting_secret_in_app" || !state.hasClient || !state.hasInput) throw new Error("operator account connection operation is in progress");
  if (password ? state.method === "telegram_qr" ? !state.hasPasswordWaiter : state.hasCode || state.method !== "telegram_phone" : !state.hasCode) throw new Error(password ? "connection is not awaiting Telegram 2FA in the app" : "connection is not awaiting a Telegram OTP in the app");
}
