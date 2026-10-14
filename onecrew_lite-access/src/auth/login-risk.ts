import { normalizeIpAddress } from "../security/ip-address";

export async function requiresEmailOTPForLogin(
  db: D1Database,
  email: string,
  currentIp: string | null | undefined,
) {
  const user = await db.prepare(
    `SELECT "id" FROM "user" WHERE "email" = ?`,
  ).bind(email.toLowerCase()).first<{ id: string }>();
  if (!user) return false;

  const latestSession = await db.prepare(
    `SELECT "ipAddress" AS "ip_address"
     FROM "session"
     WHERE "userId" = ? AND "ipAddress" IS NOT NULL
     ORDER BY "updatedAt" DESC, "createdAt" DESC, "id" DESC
     LIMIT 1`,
  ).bind(user.id).first<{ ip_address: string | null }>();
  const previousIp = normalizeIpAddress(latestSession?.ip_address);
  const nextIp = normalizeIpAddress(currentIp);
  return previousIp !== null && nextIp !== null && previousIp !== nextIp;
}
