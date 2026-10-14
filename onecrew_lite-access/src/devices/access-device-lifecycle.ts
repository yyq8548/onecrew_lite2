import {
  creatorSyncDeviceRevocationRequestSchema,
  normalizeCreatorSyncDeviceRevocationResult,
  type CreatorSyncDeviceRevocationService,
} from "./creator-sync-revocation-contract";
import { getDeviceRevocationTarget, revokeDevice } from "./service";

export async function revokeAccessDeviceLifecycle(
  env: CloudflareBindings,
  input: unknown,
) {
  const parsed = creatorSyncDeviceRevocationRequestSchema.safeParse(input);
  if (!parsed.success) return { revocation: "invalid" as const, device: null };
  const target = await getDeviceRevocationTarget(
    env.DB,
    parsed.data.userId,
    parsed.data.deviceId,
  );
  if (target === null) return { revocation: "not_found" as const, device: null };
  if (target.pairingId !== parsed.data.pairingId) {
    return { revocation: "invalid" as const, device: null };
  }
  if (target.consumed) {
    let synchronized;
    try {
      const service = env.CREATOR_SYNC_DEVICE_REVOCATION as unknown as CreatorSyncDeviceRevocationService;
      synchronized = normalizeCreatorSyncDeviceRevocationResult(
        await service.revokeAccessDevice(parsed.data),
      );
    } catch {
      synchronized = { revocation: "unavailable" as const };
    }
    if (synchronized.revocation !== "revoked") {
      return { revocation: synchronized.revocation, device: null };
    }
  }
  const device = await revokeDevice(
    env.DB,
    parsed.data.userId,
    parsed.data.deviceId,
  );
  return device?.status === "revoked"
    ? { revocation: "revoked" as const, device }
    : { revocation: "unavailable" as const, device: null };
}
