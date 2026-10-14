import { z } from "zod";
import { PAIRING_ID_PATTERN } from "../pairing/desktop-pairing-admission-contract";
import { DEVICE_ID_PATTERN } from "./ids";

export const creatorSyncDeviceRevocationRequestSchema = z
  .object({
    deviceId: z.string().regex(DEVICE_ID_PATTERN),
    pairingId: z.string().regex(PAIRING_ID_PATTERN),
    userId: z.string().min(1).max(128),
  })
  .strict();

export const creatorSyncDeviceRevocationResultSchema = z
  .object({
    revocation: z.enum(["invalid", "not_found", "revoked", "unavailable"]),
  })
  .strict();

export type CreatorSyncDeviceRevocationRequest = z.infer<
  typeof creatorSyncDeviceRevocationRequestSchema
>;
export type CreatorSyncDeviceRevocationResult = z.infer<
  typeof creatorSyncDeviceRevocationResultSchema
>;

export interface CreatorSyncDeviceRevocationService {
  revokeAccessDevice(
    request: CreatorSyncDeviceRevocationRequest,
  ): Promise<CreatorSyncDeviceRevocationResult>;
}

export function normalizeCreatorSyncDeviceRevocationResult(value: unknown) {
  const parsed = creatorSyncDeviceRevocationResultSchema.safeParse(value);
  return parsed.success ? parsed.data : ({ revocation: "unavailable" } as const);
}
