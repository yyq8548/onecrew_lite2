import { z } from "zod";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import { telegramConnectionStart, telegramConnectionOperation, telegramConnectionOtp, telegramConnectionPassword, telegramConnectionCancel } from "./telegram-connection-contracts.ts";
import { TelegramConnectionRuntime } from "./telegram-connection.ts";

export const TELEGRAM_CONNECTION_OPERATION_IDS = ["telegram.connection.login.start", "telegram.connection.login.status", "telegram.connection.login.otp.submit", "telegram.connection.login.two_factor.submit", "telegram.connection.login.cancel", "telegram.connection.reconnect.prepare", "telegram.connection.reconnect.begin", "telegram.connection.reconnect.status", "telegram.connection.reconnect.finalize", "telegram.connection.reconnect.cancel"] as const;
const flow = z.strictObject({ flow_id: z.string() });
export const telegramConnectionOperationSchemas = {
  "telegram.connection.login.start": telegramConnectionStart,
  "telegram.connection.login.status": telegramConnectionOperation,
  "telegram.connection.login.otp.submit": telegramConnectionOtp,
  "telegram.connection.login.two_factor.submit": telegramConnectionPassword,
  "telegram.connection.login.cancel": telegramConnectionCancel,
  "telegram.connection.reconnect.prepare": z.strictObject({ platform: z.literal("telegram"), intent: z.literal("reconnect"), method: z.literal("existing_account"), operator_account_id: z.string().nullish(), label: z.string().nullish() }),
  "telegram.connection.reconnect.begin": flow.extend({ confirmed: z.boolean(), idempotency_key: z.string() }),
  "telegram.connection.reconnect.status": flow,
  "telegram.connection.reconnect.finalize": flow.extend({ confirmed: z.boolean(), idempotency_key: z.string() }),
  "telegram.connection.reconnect.cancel": flow.extend({ confirmed: z.boolean() }),
} as const;
export function registerTelegramConnectionOperations(registry: OperationRegistry, runtime: TelegramConnectionRuntime): void {
  registry.register("telegram.connection.login.start", telegramConnectionStart, (input, context) => runtime.start(context, input));
  registry.register("telegram.connection.login.status", telegramConnectionOperation, async (input, context) => runtime.directStatus(context, input.operation_id));
  registry.register("telegram.connection.login.otp.submit", telegramConnectionOtp, async (input, context) => { await runtime.submit(context, { flowId: input.operation_id, value: input.otp }); return runtime.directStatus(context, input.operation_id); });
  registry.register("telegram.connection.login.two_factor.submit", telegramConnectionPassword, async (input, context) => { await runtime.submit(context, { flowId: input.operation_id, value: input.password }, true); return runtime.directStatus(context, input.operation_id); });
  registry.register("telegram.connection.login.cancel", telegramConnectionCancel, async (input, context) => { await runtime.cancel(context, { flow_id: input.operation_id, confirmed: input.confirmed }); return runtime.directStatus(context, input.operation_id); });
  registry.register("onecrew_operator_account_connection_configure_telegram_in_app", z.strictObject({ flowId: z.string(), apiId: z.number(), apiHash: z.string(), phone: z.string().nullish(), proxy: z.unknown() }), (input, context) => runtime.configure(context, input));
  registry.register("onecrew_operator_account_connection_submit_telegram_code_in_app", z.strictObject({ flowId: z.string(), code: z.string() }), (input, context) => runtime.submit(context, { flowId: input.flowId, value: input.code }));
  registry.register("onecrew_operator_account_connection_submit_telegram_password_in_app", z.strictObject({ flowId: z.string(), password: z.string() }), (input, context) => runtime.submit(context, { flowId: input.flowId, value: input.password }, true));
  registry.register("onecrew_operator_account_connection_open_telegram_credentials_in_app", z.strictObject({ flowId: z.string(), proxy: z.unknown() }), (input, context) => runtime.openCredentials(context, input));
  registry.register("telegram.connection.reconnect.prepare", telegramConnectionOperationSchemas["telegram.connection.reconnect.prepare"], (input, context) => runtime.prepare(context, input));
  for (const action of ["begin", "status", "finalize", "cancel"] as const) {
    const schema = telegramConnectionOperationSchemas[`telegram.connection.reconnect.${action}`];
    registry.register(`telegram.connection.reconnect.${action}`, schema, async (input, context) => {
      const current = await runtime.status(context, { flow_id: input.flow_id }); if (current.intent !== "reconnect" || current.method !== "existing_account") throw new Error("operation is not a Telegram reconnect");
      return runtime[action](context, input);
    });
  }
}
