import { createDeviceId } from "../devices/ids";
import {
  PAIRING_GRANT_TTL_MS,
  consumePairingGrantRequestSchema,
  type ConsumePairingGrantResult,
  type DesktopPairingAuthorizeRequest,
} from "./contracts";
import {
  canonicalPairingRequest,
  constantTimeEqual,
  pairingHmac,
  pkceChallenge,
  randomOpaqueId,
  randomPairingCode,
} from "./crypto";
import { verifyApprovalToken } from "./approval-token";
import { normalizeIpAddress } from "../security/ip-address";
import type { EdgeLocation } from "../security/edge-location";
import type {
  ResetPairingAuthorizationRateLimitRequest,
  ResetPairingAuthorizationRateLimitResult,
} from "./rate-limit-admin-contract";

const MAX_CURRENT_DEVICES_PER_USER = 20;
const MAX_CLOSED_DEVICE_HISTORY_PER_USER = 200;
const AUTHORIZE_LIMIT = { requests: 5, windowMs: 5 * 60 * 1000 } as const;
const GRANT_ATTEMPT_LIMIT = 8;

type Runtime = {
  db: D1Database;
  pairingHmacSecret: string;
};

type PairingRow = {
  attempt_count: number;
  code_challenge: string;
  code_hmac: string;
  consumed_at: number | null;
  device_id: string;
  device_revoked_at: number | null;
  expires_at: number;
  pairing_id: string;
  user_id: string;
};

type RateLimitRow = { request_count: number; reset_at: number };

export async function pairingAuthorizationReadiness(
  runtime: Runtime,
  userId: string,
  now = Date.now(),
) {
  const row = await runtime.db.prepare(
    `select "request_count", "reset_at" from "desktop_pairing_rate_limit"
     where "bucket" = ?`,
  ).bind(`authorize:${userId}`).first<RateLimitRow>();
  if (!row || row.reset_at <= now || row.request_count < AUTHORIZE_LIMIT.requests) {
    return { kind: "ready" } as const;
  }
  return {
    kind: "rate_limited" as const,
    retryAfterSeconds: Math.max(1, Math.ceil((row.reset_at - now) / 1000)),
  };
}

export async function resetPairingAuthorizationRateLimit(
  runtime: Runtime,
  input: ResetPairingAuthorizationRateLimitRequest,
): Promise<ResetPairingAuthorizationRateLimitResult> {
  const deleted = await runtime.db.prepare(
    `delete from "desktop_pairing_rate_limit" where "bucket" = ?`,
  ).bind(`authorize:${input.userId}`).run();
  return {
    reset: "completed",
    clearedAuthorizationBuckets: deleted.meta.changes ?? 0,
  };
}

export async function isPairingRequestResolved(
  runtime: Runtime,
  userId: string,
  request: DesktopPairingAuthorizeRequest,
) {
  const requestHmac = await pairingHmac(
    runtime.pairingHmacSecret,
    "request",
    canonicalPairingRequest(userId, request),
  );
  return (
    await runtime.db
      .prepare(
        `select "id" from "desktop_pairing_grant" where "request_hmac" = ?`,
      )
      .bind(requestHmac)
      .first<{ id: string }>()
  ) !== null;
}

async function takeAuthorizationRateLimit(
  db: D1Database,
  userId: string,
  now: number,
) {
  const resetAt = now + AUTHORIZE_LIMIT.windowMs;
  const row = await db
    .prepare(
      `insert into "desktop_pairing_rate_limit" ("bucket", "request_count", "reset_at")
       values (?, 1, ?)
       on conflict ("bucket") do update set
         "request_count" = case
           when "reset_at" <= ? then 1
           else min("request_count" + 1, ?)
         end,
         "reset_at" = case
           when "reset_at" <= ? then ?
           else "reset_at"
         end
       returning "request_count", "reset_at"`,
    )
    .bind(
      `authorize:${userId}`,
      resetAt,
      now,
      AUTHORIZE_LIMIT.requests + 1,
      now,
      resetAt,
    )
    .first<RateLimitRow>();
  await db
    .prepare(
      `delete from "desktop_pairing_rate_limit"
       where "bucket" in (
         select "bucket" from "desktop_pairing_rate_limit"
         where "bucket" <> ? and "reset_at" <= ?
         order by "reset_at" asc
         limit 32
       )`,
    )
    .bind(`authorize:${userId}`, now)
    .run();
  if (row !== null && row.request_count <= AUTHORIZE_LIMIT.requests) {
    return { allowed: true as const };
  }
  return {
    allowed: false as const,
    retryAfterSeconds: row === null
      ? Math.ceil(AUTHORIZE_LIMIT.windowMs / 1000)
      : Math.max(1, Math.ceil((row.reset_at - now) / 1000)),
  };
}

export async function approveDesktopPairing(
  runtime: Runtime,
  approvalToken: unknown,
  session: {
    sessionId: string;
    userId: string;
    ipAddress?: string | null;
    bindingLocation?: EdgeLocation;
  },
  now = Date.now(),
): Promise<
  | { kind: "approved"; callbackLocation: string; expiresAt: string }
  | { kind: "device_limit_reached" }
  | { kind: "expired" }
  | { kind: "invalid" }
  | { kind: "rate_limited"; retryAfterSeconds: number }
  | { kind: "resolved" }
> {
  const verified = await verifyApprovalToken(
    runtime.pairingHmacSecret,
    approvalToken,
    session.userId,
    session.sessionId,
    now,
  );
  if (verified.kind !== "valid") return verified;
  const rateLimit = await takeAuthorizationRateLimit(runtime.db, session.userId, now);
  if (!rateLimit.allowed) {
    return { kind: "rate_limited", retryAfterSeconds: rateLimit.retryAfterSeconds };
  }

  const request = verified.payload.request;
  const pairingIp = normalizeIpAddress(session.ipAddress);
  const pairingLocation = session.bindingLocation ?? {
    country: null,
    region: null,
    city: null,
    colo: null,
  };
  const requestHmac = await pairingHmac(
    runtime.pairingHmacSecret,
    "request",
    canonicalPairingRequest(session.userId, request),
  );
  const resolved = await runtime.db
    .prepare(
      `select "id" from "desktop_pairing_grant" where "request_hmac" = ?`,
    )
    .bind(requestHmac)
    .first<{ id: string }>();
  if (resolved) return { kind: "resolved" };

  await pruneClosedDeviceHistory(runtime.db, session.userId, now);
  const pairingId = randomOpaqueId("pair");
  const deviceId = createDeviceId();
  const code = randomPairingCode();
  const codeHmac = await pairingHmac(runtime.pairingHmacSecret, "code", code);
  const expiresAt = now + PAIRING_GRANT_TTL_MS;

  try {
    const inserted = await runtime.db.batch([
      runtime.db
        .prepare(
          `insert into "access_device"
             ("id", "user_id", "label", "created_at", "pairingIp",
              "pairingCountry", "pairingRegion", "pairingCity", "pairingColo")
           select ?, ?, ?, ?, ?, ?, ?, ?, ?
           where (
             select count(*)
             from "access_device" d
             join "desktop_pairing_grant" p on p."device_id" = d."id"
             where d."user_id" = ?
               and d."revoked_at" is null
               and (p."consumed_at" is not null or p."expires_at" > ?)
           ) < ?
           returning "id"`,
        )
        .bind(
          deviceId,
          session.userId,
          request.label,
          now,
          pairingIp,
          pairingLocation.country,
          pairingLocation.region,
          pairingLocation.city,
          pairingLocation.colo,
          session.userId,
          now,
          MAX_CURRENT_DEVICES_PER_USER,
        ),
      runtime.db
        .prepare(
          `insert into "desktop_pairing_grant"
             ("id", "device_id", "request_hmac", "code_hmac", "code_challenge",
              "platform", "app_version", "created_at", "expires_at")
           select ?, ?, ?, ?, ?, ?, ?, ?, ?
           where exists (
             select 1 from "access_device" where "id" = ? and "user_id" = ?
           )
           returning "id"`,
        )
        .bind(
          pairingId,
          deviceId,
          requestHmac,
          codeHmac,
          request.codeChallenge,
          request.platform,
          request.appVersion,
          now,
          expiresAt,
          deviceId,
          session.userId,
        ),
    ]);
    if (!inserted[0]?.results[0]) return { kind: "device_limit_reached" };
    if (!inserted[1]?.results[0]) throw new Error("pairing_grant_not_inserted");
  } catch (error) {
    const duplicate = await runtime.db
      .prepare(
        `select "id" from "desktop_pairing_grant" where "request_hmac" = ?`,
      )
      .bind(requestHmac)
      .first<{ id: string }>();
    if (duplicate) return { kind: "resolved" };
    throw error;
  }

  const callback = new URL(
    `http://127.0.0.1:${request.callbackPort}/creator-sync/pairing-callback`,
  );
  callback.searchParams.set("code", code);
  callback.searchParams.set("state", request.state);
  return {
    callbackLocation: callback.toString(),
    expiresAt: new Date(expiresAt).toISOString(),
    kind: "approved",
  };
}

async function pruneClosedDeviceHistory(
  db: D1Database,
  userId: string,
  now: number,
) {
  await db
    .prepare(
      `delete from "access_device"
       where "id" in (
         select d."id"
         from "access_device" d
         join "desktop_pairing_grant" p on p."device_id" = d."id"
         where d."user_id" = ?
           and (d."revoked_at" is not null or (p."consumed_at" is null and p."expires_at" <= ?))
         order by d."created_at" desc, d."id" desc
         limit -1 offset ?
       )`,
    )
    .bind(userId, now, MAX_CLOSED_DEVICE_HISTORY_PER_USER)
    .run();
}

async function readPairing(db: D1Database, codeHmac: string) {
  return db
    .prepare(
      `select
         p."id" as "pairing_id",
         p."device_id",
         p."code_hmac",
         p."code_challenge",
         p."expires_at",
         p."consumed_at",
         p."attempt_count",
         d."user_id",
         d."revoked_at" as "device_revoked_at"
       from "desktop_pairing_grant" p
       join "access_device" d on d."id" = p."device_id"
       where p."code_hmac" = ?`,
    )
    .bind(codeHmac)
    .first<PairingRow>();
}

function closedAdmission(row: PairingRow, now: number) {
  if (row.device_revoked_at !== null) return "revoked" as const;
  if (row.consumed_at !== null) return "consumed" as const;
  if (row.expires_at <= now) return "expired" as const;
  return null;
}

export async function consumePairingGrant(
  runtime: Runtime,
  input: unknown,
  now?: number,
): Promise<ConsumePairingGrantResult> {
  const parsed = consumePairingGrantRequestSchema.safeParse(input);
  if (!parsed.success) return { admission: "invalid" };
  const codeHmac = await pairingHmac(
    runtime.pairingHmacSecret,
    "code",
    parsed.data.code,
  );
  let row = await readPairing(runtime.db, codeHmac);
  if (!row) return { admission: "invalid" };

  const challenge = await pkceChallenge(parsed.data.codeVerifier);
  if (!constantTimeEqual(challenge, row.code_challenge)) {
    const attempt = await runtime.db
      .prepare(
        `update "desktop_pairing_grant"
         set "attempt_count" = min("attempt_count" + 1, ?)
         where "id" = ?
         returning "attempt_count"`,
      )
      .bind(GRANT_ATTEMPT_LIMIT + 1, row.pairing_id)
      .first<{ attempt_count: number }>();
    return !attempt || attempt.attempt_count > GRANT_ATTEMPT_LIMIT
      ? { admission: "rate_limited" }
      : { admission: "invalid" };
  }
  const consumeAt = now ?? Date.now();
  const alreadyClosed = closedAdmission(row, consumeAt);
  if (alreadyClosed) return { admission: alreadyClosed };
  if (row.attempt_count >= GRANT_ATTEMPT_LIMIT) {
    return { admission: "rate_limited" };
  }

  const consumed = await runtime.db
    .prepare(
      `update "desktop_pairing_grant"
       set "consumed_at" = ?
       where "id" = ?
         and "code_hmac" = ?
         and "code_challenge" = ?
         and "consumed_at" is null
         and "expires_at" > ?
         and "attempt_count" < ?
         and exists (
           select 1 from "access_device" d
           where d."id" = "desktop_pairing_grant"."device_id"
             and d."revoked_at" is null
         )
       returning "id"`,
    )
    .bind(
      consumeAt,
      row.pairing_id,
      codeHmac,
      challenge,
      consumeAt,
      GRANT_ATTEMPT_LIMIT,
    )
    .first<{ id: string }>();
  if (!consumed) {
    row = (await readPairing(runtime.db, codeHmac)) ?? row;
    const closed = closedAdmission(row, consumeAt);
    if (closed) return { admission: closed };
    return row.attempt_count >= GRANT_ATTEMPT_LIMIT
      ? { admission: "rate_limited" }
      : { admission: "unavailable" };
  }
  return {
    admission: "admitted",
    deviceId: row.device_id,
    pairingId: row.pairing_id,
    userId: row.user_id,
  };
}

export async function safelyConsumePairingGrant(
  runtime: Runtime,
  input: unknown,
) {
  try {
    return await consumePairingGrant(runtime, input);
  } catch {
    return { admission: "unavailable" } as const;
  }
}

export function pairingPlatformLabel(
  platform: DesktopPairingAuthorizeRequest["platform"],
) {
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  return "Linux";
}
