import { z } from "zod";
import {
  getDistributionSettings,
  setDistributionSettings,
} from "./distribution/settings";
import { DistributionError } from "./distribution/contracts";
import {
  applicationDetail,
  listApplications,
  reviewApplication,
  retryEmail,
} from "./distribution/service";
import { flushDistributionMail, logDistributionMailFlushFailure } from "./distribution/outbox";
import { cleanExpiredLegacyTickets } from "./cli-distribution/service";
import { WorkerEntrypoint } from "cloudflare:workers";
import type {
  ConsumePairingGrantRequest,
  ConsumePairingGrantResult,
  DesktopPairingAdmissionContract,
} from "./pairing/desktop-pairing-admission-contract";
import { normalizeConsumePairingGrantResult } from "./pairing/desktop-pairing-admission-contract";
import { safelyConsumePairingGrant } from "./pairing/service";
import {
  type DesktopPairingRateLimitAdminContract,
  normalizeResetPairingAuthorizationRateLimitResult,
  type ResetPairingAuthorizationRateLimitRequest,
  type ResetPairingAuthorizationRateLimitResult,
  ResetPairingAuthorizationRateLimitRequestSchema,
} from "./pairing/rate-limit-admin-contract";
import { resetPairingAuthorizationRateLimit } from "./pairing/service";
import { revokeAccessDeviceLifecycle } from "./devices/access-device-lifecycle";
import {
  AccessUserDirectoryRequestSchema,
  normalizeAccessUserDirectoryResult,
  type AccessUserDirectoryService,
  type AccessUserDirectoryRequest,
  type AccessUserDirectoryResult,
} from "./users/access-user-directory-contract";
import { lookupAccessUsers } from "./users/service";
import type {
  CreatorSyncDeviceRevocationRequest,
  CreatorSyncDeviceRevocationResult,
  CreatorSyncDeviceRevocationService,
} from "./devices/creator-sync-revocation-contract";
import { createApp } from "./app";
import { parseRuntimeConfig } from "./config/env";

export class DesktopPairingAdmission
  extends WorkerEntrypoint<CloudflareBindings>
  implements DesktopPairingAdmissionContract
{
  async consumePairingGrant(
    input: ConsumePairingGrantRequest,
  ): Promise<ConsumePairingGrantResult> {
    try {
      const config = parseRuntimeConfig(this.env);
      return normalizeConsumePairingGrantResult(
        await safelyConsumePairingGrant(config, input),
      );
    } catch {
      return { admission: "unavailable" };
    }
  }
}

export class DesktopPairingRateLimitAdmin
  extends WorkerEntrypoint<CloudflareBindings>
  implements DesktopPairingRateLimitAdminContract
{
  async resetAuthorizationRateLimit(
    input: ResetPairingAuthorizationRateLimitRequest,
  ): Promise<ResetPairingAuthorizationRateLimitResult> {
    const parsed =
      ResetPairingAuthorizationRateLimitRequestSchema.safeParse(input);
    if (!parsed.success) return { reset: "invalid" };
    try {
      const config = parseRuntimeConfig(this.env);
      return normalizeResetPairingAuthorizationRateLimitResult(
        await resetPairingAuthorizationRateLimit(config, parsed.data),
      );
    } catch {
      return { reset: "unavailable" };
    }
  }
}

export class DesktopDeviceLifecycle
  extends WorkerEntrypoint<CloudflareBindings>
  implements CreatorSyncDeviceRevocationService
{
  async revokeAccessDevice(
    input: CreatorSyncDeviceRevocationRequest,
  ): Promise<CreatorSyncDeviceRevocationResult> {
    try {
      const result = await revokeAccessDeviceLifecycle(this.env, input);
      return { revocation: result.revocation };
    } catch {
      return { revocation: "unavailable" };
    }
  }
}

export class AccessUserDirectory
  extends WorkerEntrypoint<CloudflareBindings>
  implements AccessUserDirectoryService
{
  async lookupUsers(
    input: AccessUserDirectoryRequest,
  ): Promise<AccessUserDirectoryResult> {
    const parsed = AccessUserDirectoryRequestSchema.safeParse(input);
    if (!parsed.success) return { users: [] };
    try {
      const config = parseRuntimeConfig(this.env);
      return normalizeAccessUserDirectoryResult(
        await lookupAccessUsers(config, parsed.data.userIds),
      );
    } catch {
      return { users: [] };
    }
  }
}

export class DesktopDistributionAdmin extends WorkerEntrypoint<CloudflareBindings> {
  async getDistributionSettings(input: unknown) {
    return distributionRpc(() => getDistributionSettings(this.env, input));
  }
  async setDistributionSettings(input: unknown) {
    return distributionRpc(() => setDistributionSettings(this.env, input));
  }
  async listApplications(input: unknown) {
    return distributionRpc(() => listApplications(this.env, input));
  }
  async getApplication(input: unknown) {
    return distributionRpc(async () => {
      const parsed = distributionGetInput.parse(input);
      return {
        ok: true,
        application: await applicationDetail(this.env.DB, parsed.applicationId),
      };
    });
  }
  async reviewApplication(input: unknown) {
    const result = await distributionRpc(() =>
      reviewApplication(this.env, input),
    );
    if (result.ok)
      this.ctx.waitUntil(
        flushDistributionMail(this.env).catch(logDistributionMailFlushFailure),
      );
    return result;
  }
  async retryEmail(input: unknown) {
    return distributionRpc(() => retryEmail(this.env, input));
  }
}
const distributionGetInput = z
  .object({ applicationId: z.string().uuid() })
  .strict();
async function distributionRpc<T extends { ok: boolean }>(
  action: () => Promise<T>,
): Promise<T | { ok: false; error: string }> {
  try {
    return await action();
  } catch (error) {
    return {
      ok: false,
      error:
        error instanceof DistributionError
          ? error.code
          : error instanceof z.ZodError
            ? "invalid_request"
            : "unavailable",
    };
  }
}
const app = createApp();
export default Object.assign(app, {
  scheduled: (
    _event: ScheduledController,
    env: CloudflareBindings,
    ctx: ExecutionContext,
  ) => {
    ctx.waitUntil(flushDistributionMail(env).catch(logDistributionMailFlushFailure));
    ctx.waitUntil(cleanExpiredLegacyTickets(env).catch(() => console.error("cli_legacy_ticket_cleanup_failed")));
  },
});
