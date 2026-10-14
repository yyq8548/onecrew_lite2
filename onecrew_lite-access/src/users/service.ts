import type { RuntimeConfig } from "../config/env";
import type { AccessUserDirectoryResult } from "./access-user-directory-contract";

type AccessUserRow = {
  user_id: string;
  email: string;
  binding_ip: string | null;
  binding_country: string | null;
  binding_region: string | null;
  binding_city: string | null;
  binding_colo: string | null;
};

export async function lookupAccessUsers(
  config: RuntimeConfig,
  userIds: string[],
): Promise<AccessUserDirectoryResult> {
  const placeholders = userIds.map(() => "?").join(", ");
  const rows = await config.db.prepare(
    `SELECT "user"."id" AS user_id,
            "user"."email",
            "binding_device"."pairingIp" AS binding_ip,
            "binding_device"."pairingCountry" AS binding_country,
            "binding_device"."pairingRegion" AS binding_region,
            "binding_device"."pairingCity" AS binding_city,
            "binding_device"."pairingColo" AS binding_colo
     FROM "user"
     LEFT JOIN "access_device" AS "binding_device"
       ON "binding_device"."id" = (
         SELECT "candidate"."id"
         FROM "access_device" AS "candidate"
         WHERE "candidate"."user_id" = "user"."id"
           AND "candidate"."pairingIp" IS NOT NULL
         ORDER BY "candidate"."created_at" DESC, "candidate"."id" DESC
         LIMIT 1
       )
     WHERE "user"."id" IN (${placeholders})`,
  ).bind(...userIds).all<AccessUserRow>();
  return {
    users: rows.results.map((row) => ({
      userId: row.user_id,
      email: row.email,
      bindingIp: row.binding_ip,
      bindingLocation: {
        country: row.binding_country,
        region: row.binding_region,
        city: row.binding_city,
        colo: row.binding_colo,
      },
    })),
  };
}
