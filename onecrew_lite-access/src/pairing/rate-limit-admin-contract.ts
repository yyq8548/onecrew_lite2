import { z } from "zod";

export const ResetPairingAuthorizationRateLimitRequestSchema = z.object({
  requestId: z.string().uuid(),
  userId: z.string().min(1).max(128),
}).strict();

export const ResetPairingAuthorizationRateLimitResultSchema = z.discriminatedUnion(
  "reset",
  [
    z.object({
      reset: z.literal("completed"),
      clearedAuthorizationBuckets: z.number().int().min(0).max(1),
    }).strict(),
    z.object({ reset: z.literal("invalid") }).strict(),
    z.object({ reset: z.literal("unavailable") }).strict(),
  ],
);

export type ResetPairingAuthorizationRateLimitRequest = z.infer<
  typeof ResetPairingAuthorizationRateLimitRequestSchema
>;
export type ResetPairingAuthorizationRateLimitResult = z.infer<
  typeof ResetPairingAuthorizationRateLimitResultSchema
>;

export interface DesktopPairingRateLimitAdminContract {
  resetAuthorizationRateLimit(
    input: ResetPairingAuthorizationRateLimitRequest,
  ): Promise<ResetPairingAuthorizationRateLimitResult>;
}

export function normalizeResetPairingAuthorizationRateLimitResult(value: unknown) {
  const parsed = ResetPairingAuthorizationRateLimitResultSchema.safeParse(value);
  return parsed.success ? parsed.data : ({ reset: "unavailable" } as const);
}
