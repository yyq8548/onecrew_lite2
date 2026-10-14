export type SuccessEvent = "sign_up" | "generate_lead";

export function attributionContext(headers?: Headers): string | null {
  const value = headers?.get("Cookie")?.split(";").map(part => part.trim()).find(part => part.startsWith("__Host-onecrew-ga-context="))?.split("=", 2)[1];
  return value && /^[a-f0-9]{64}$/.test(value) ? value : null;
}

export async function recordSuccessEvent(db: D1Database, userId: string, event: SuccessEvent, key: string, context: string | null = null) {
  try {
    await db.prepare(
      "INSERT INTO analytics_success_event(receipt_key,user_id,event_name,created_at,attribution_context) VALUES(?,?,?,?,?) ON CONFLICT(receipt_key) DO NOTHING",
    ).bind(`${event}:${key}`, userId, event, Date.now(), context).run();
  } catch {
    // Telemetry must not turn an already committed business operation into a failure.
    console.warn("analytics_success_receipt_unavailable");
  }
}

export async function claimSuccessEvents(db: D1Database, userId: string, context: string): Promise<SuccessEvent[]> {
  const now = Date.now();
  const result = await db.prepare(
    `UPDATE analytics_success_event SET claimed_at=?
     WHERE receipt_key IN (
       SELECT receipt_key FROM analytics_success_event
       WHERE user_id=? AND attribution_context=? AND created_at>=? AND claimed_at IS NULL ORDER BY created_at LIMIT 20
     ) RETURNING event_name`,
  ).bind(now, userId, context, now - 30 * 60 * 1000).all<{ event_name: SuccessEvent }>();
  return result.results.map(row => row.event_name);
}
