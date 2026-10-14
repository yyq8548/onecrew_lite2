import { z } from "zod";
import { DEVICE_ID_PATTERN } from "../devices/ids";

export const PAIRING_ID_PATTERN = /^pair_[A-Za-z0-9_-]{24}$/;
export const PAIRING_CODE_PATTERN =
  /^reacher_pairing_v1\.[A-Za-z0-9_-]{43}$/;

export const pairingCodeSchema = z.string().regex(PAIRING_CODE_PATTERN);
export const pairingCodeVerifierSchema = z
  .string()
  .min(43)
  .max(128)
  .regex(/^[A-Za-z0-9._~-]+$/);

export const consumePairingGrantRequestSchema = z
  .object({
    code: pairingCodeSchema,
    codeVerifier: pairingCodeVerifierSchema,
  })
  .strict();

const admittedPairingSchema = z
  .object({
    admission: z.literal("admitted"),
    deviceId: z.string().regex(DEVICE_ID_PATTERN),
    pairingId: z.string().regex(PAIRING_ID_PATTERN),
    userId: z.string().min(1).max(128),
  })
  .strict();

const rejectedPairingSchema = z
  .object({
    admission: z.enum([
      "consumed",
      "expired",
      "invalid",
      "rate_limited",
      "revoked",
      "unavailable",
    ]),
  })
  .strict();

export const consumePairingGrantResultSchema = z.union([
  admittedPairingSchema,
  rejectedPairingSchema,
]);

export type ConsumePairingGrantRequest = z.infer<
  typeof consumePairingGrantRequestSchema
>;
export type ConsumePairingGrantResult = z.infer<
  typeof consumePairingGrantResultSchema
>;

export interface DesktopPairingAdmissionContract {
  consumePairingGrant(
    input: ConsumePairingGrantRequest,
  ): Promise<ConsumePairingGrantResult>;
}

export function normalizeConsumePairingGrantResult(value: unknown) {
  const parsed = consumePairingGrantResultSchema.safeParse(value);
  return parsed.success ? parsed.data : ({ admission: "unavailable" } as const);
}
