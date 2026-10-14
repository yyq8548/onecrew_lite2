import type { DeviceStatus, DeviceSummary } from "./contracts";
import { edgeLocationFromRow } from "../security/edge-location";

export const DEVICE_LIST_LIMIT = 20;

type DeviceRow = {
  app_version: string;
  consumed_at: number | null;
  created_at: number;
  device_id: string;
  device_revoked_at: number | null;
  expires_at: number;
  label: string;
  pairing_ip: string | null;
  pairing_country: string | null;
  pairing_region: string | null;
  pairing_city: string | null;
  pairing_colo: string | null;
  pairing_id: string;
  platform: "linux" | "macos" | "windows";
  user_id: string;
};

function iso(value: number | null) {
  return value === null ? null : new Date(value).toISOString();
}

function statusFor(row: DeviceRow, now: number): DeviceStatus {
  if (row.device_revoked_at !== null) return "revoked";
  if (row.consumed_at !== null) return "active";
  if (row.expires_at <= now) return "expired";
  return "pending";
}

function safeDevice(row: DeviceRow, now: number): DeviceSummary {
  return {
    appVersion: row.app_version,
    createdAt: new Date(row.created_at).toISOString(),
    deviceId: row.device_id,
    label: row.label,
    pairedAt: iso(row.consumed_at),
    pairingExpiresAt: new Date(row.expires_at).toISOString(),
    pairingId: row.pairing_id,
    pairingIp: row.pairing_ip,
    pairingLocation: edgeLocationFromRow(row),
    platform: row.platform,
    revokedAt: iso(row.device_revoked_at),
    status: statusFor(row, now),
  };
}

async function readDeviceRow(db: D1Database, userId: string, deviceId: string) {
  return db
    .prepare(
      `select
         d."id" as "device_id",
         d."user_id",
         d."label",
         d."pairingIp" as "pairing_ip",
         d."pairingCountry" as "pairing_country",
         d."pairingRegion" as "pairing_region",
         d."pairingCity" as "pairing_city",
         d."pairingColo" as "pairing_colo",
         d."created_at",
         d."revoked_at" as "device_revoked_at",
         p."id" as "pairing_id",
         p."platform",
         p."app_version",
         p."expires_at",
         p."consumed_at"
       from "access_device" d
       join "desktop_pairing_grant" p on p."device_id" = d."id"
       where d."id" = ? and d."user_id" = ?`,
    )
    .bind(deviceId, userId)
    .first<DeviceRow>();
}

export async function listDevices(
  db: D1Database,
  userId: string,
  cursorDeviceId: string | null,
  now = Date.now(),
) {
  const cursor = cursorDeviceId
    ? await db
        .prepare(
          `select d."created_at"
           from "access_device" d
           join "desktop_pairing_grant" p on p."device_id" = d."id"
           where d."id" = ? and d."user_id" = ?`,
        )
        .bind(cursorDeviceId, userId)
        .first<{ created_at: number }>()
    : null;
  if (cursorDeviceId && !cursor) return null;

  const rows = await db
    .prepare(
      `select
         d."id" as "device_id",
         d."user_id",
         d."label",
         d."pairingIp" as "pairing_ip",
         d."pairingCountry" as "pairing_country",
         d."pairingRegion" as "pairing_region",
         d."pairingCity" as "pairing_city",
         d."pairingColo" as "pairing_colo",
         d."created_at",
         d."revoked_at" as "device_revoked_at",
         p."id" as "pairing_id",
         p."platform",
         p."app_version",
         p."expires_at",
         p."consumed_at"
       from "access_device" d
       join "desktop_pairing_grant" p on p."device_id" = d."id"
       where d."user_id" = ?
         and (
           ? is null
           or d."created_at" < ?
           or (d."created_at" = ? and d."id" < ?)
         )
       order by d."created_at" desc, d."id" desc
       limit ?`,
    )
    .bind(
      userId,
      cursorDeviceId,
      cursor?.created_at ?? null,
      cursor?.created_at ?? null,
      cursorDeviceId,
      DEVICE_LIST_LIMIT + 1,
    )
    .all<DeviceRow>();
  const hasMore = rows.results.length > DEVICE_LIST_LIMIT;
  const pageRows = rows.results.slice(0, DEVICE_LIST_LIMIT);
  return {
    devices: pageRows.map((row) => safeDevice(row, now)),
    nextCursor: hasMore ? (pageRows.at(-1)?.device_id ?? null) : null,
  };
}

export async function getDeviceStatus(
  db: D1Database,
  userId: string,
  deviceId: string,
  now = Date.now(),
) {
  const row = await readDeviceRow(db, userId, deviceId);
  return row ? safeDevice(row, now) : null;
}

export async function getDeviceRevocationTarget(
  db: D1Database,
  userId: string,
  deviceId: string,
  now = Date.now(),
) {
  const row = await readDeviceRow(db, userId, deviceId);
  return row
    ? {
        consumed: row.consumed_at !== null,
        pairingId: row.pairing_id,
        status: statusFor(row, now),
      }
    : null;
}

export async function revokeDevice(
  db: D1Database,
  userId: string,
  deviceId: string,
  now = Date.now(),
) {
  const existing = await readDeviceRow(db, userId, deviceId);
  if (!existing) return null;
  await db
    .prepare(
      `update "access_device"
       set "revoked_at" = coalesce("revoked_at", ?)
       where "id" = ? and "user_id" = ?`,
    )
    .bind(now, deviceId, userId)
    .run();
  const revoked = await readDeviceRow(db, userId, deviceId);
  return revoked ? safeDevice(revoked, now) : null;
}

export async function deleteDevice(
  db: D1Database,
  userId: string,
  deviceId: string,
  now = Date.now(),
) {
  const existing = await readDeviceRow(db, userId, deviceId);
  if (!existing) return { kind: "not_found" } as const;
  if (!matchesDeletableStatus(existing, now)) {
    return { kind: "must_revoke" } as const;
  }
  const deleted = await db
    .prepare(
      `delete from "access_device"
       where "id" = ? and "user_id" = ?
         and exists (
           select 1 from "desktop_pairing_grant" p
           where p."device_id" = "access_device"."id"
             and (
               "access_device"."revoked_at" is not null
               or (p."consumed_at" is null and p."expires_at" <= ?)
             )
         )
       returning "id"`,
    )
    .bind(deviceId, userId, now)
    .first<{ id: string }>();
  if (!deleted) {
    return (await readDeviceRow(db, userId, deviceId))
      ? ({ kind: "must_revoke" } as const)
      : ({ kind: "not_found" } as const);
  }
  if (await readDeviceRow(db, userId, deviceId)) {
    throw new Error("device_delete_readback_failed");
  }
  return { deviceId: deleted.id, kind: "deleted" } as const;
}

function matchesDeletableStatus(row: DeviceRow, now: number) {
  return row.device_revoked_at !== null
    || (row.consumed_at === null && row.expires_at <= now);
}
