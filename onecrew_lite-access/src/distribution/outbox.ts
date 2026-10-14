import type { Bindings } from "../types/bindings";
import { parseRuntimeConfig } from "../config/env";
import type { DistributionMailErrorCode } from "./contracts";
import { distributionMailEligible } from "./mail-policy";
// State is committed before network I/O. A crash therefore remains unknown, never falsely sent.
// Resend idempotency is reused only inside its 24-hour retention window (23h local bound).
export async function flushDistributionMail(env: Bindings) {
  const config = parseRuntimeConfig(env);
  if (!config.resend) {
    console.error("distribution_mail_flush", { code: "mail_not_configured" });
    return;
  }
  const db = env.DB,
    now = Date.now();
  await db
    .prepare(
      "UPDATE distribution_mail SET status=CASE WHEN attempts>0 THEN 'unknown' ELSE 'failed' END,lease_until=NULL,blocked_reason='retry_window_exhausted' WHERE blocked_reason IS NULL AND status IN ('pending','unknown') AND created_at<=? AND (lease_until IS NULL OR lease_until<?)",
    )
    .bind(now - 23 * 3600000, now)
    .run();
  const rows = await db
    .prepare(
      "SELECT id FROM distribution_mail WHERE blocked_reason IS NULL AND status='pending' AND attempts=0 AND created_at>? AND (lease_until IS NULL OR lease_until<?) AND (last_attempt_at IS NULL OR last_attempt_at<?) ORDER BY created_at LIMIT 10",
    )
    .bind(now - 23 * 3600000, now, now - 60000)
    .all<{ id: string }>();
  for (const row of rows.results) await sendOne(env, row.id);
}
// Called only after retryEmail has transactionally authorized this exact request/event.
export async function sendRequestedDistributionMail(env: Bindings, id: string, requestId: string) {
  await sendOne(env, id, requestId);
}
async function sendOne(env: Bindings, id: string, requestId: string | null = null) {
  const config = parseRuntimeConfig(env);
  if (!config.resend) return;
  const db = env.DB,
    now = Date.now();
  const claim = await db
    .prepare(
      "UPDATE distribution_mail SET lease_until=?,manual_request_id=NULL WHERE id=? AND blocked_reason IS NULL AND ((? IS NULL AND status='pending' AND attempts=0 AND manual_request_id IS NULL) OR (? IS NOT NULL AND manual_request_id=? AND status IN ('failed','unknown'))) AND created_at>? AND (lease_until IS NULL OR lease_until<?) AND (last_attempt_at IS NULL OR last_attempt_at<?) RETURNING application_id,recipient,kind,decision_revision,payload_json,status,attempts",
    )
    .bind(now + 120000, id, requestId, requestId, requestId, now - 23 * 3600000, now, now - 60000)
    .first<{
      application_id: string;
      recipient: string;
      kind: "approved" | "rejected";
      decision_revision: number;
      payload_json: string;
      status: string;
      attempts: number;
    }>();
  if (!claim) return;
  if (!(await distributionMailEligible(db, claim))) {
    await db
      .prepare(
        "UPDATE distribution_mail SET status=?,lease_until=NULL,blocked_reason='decision_or_recipient_ineligible' WHERE id=?",
      )
      .bind(claim.attempts > 0 ? "unknown" : "failed", id)
      .run();
    return;
  }
  await db
    .prepare(
      "UPDATE distribution_mail SET status='unknown',attempts=attempts+1,last_attempt_at=?,last_http_status=NULL,last_error_code=NULL WHERE id=?",
    )
    .bind(now, id)
    .run();
  let httpStatus: number | null = null;
  let errorCode: DistributionMailErrorCode = "transport_error";
  let status: "unknown" | "failed" = "unknown";
  try {
    const response = await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.resend.apiKey}`,
        "Content-Type": "application/json",
        "Idempotency-Key": `distribution/${id}`,
      },
      body: claim.payload_json,
      redirect: "manual",
      signal: AbortSignal.timeout(15000),
    });
    httpStatus = response.status;
    if (response.ok) {
      errorCode = "invalid_provider_response";
      const body = (await response.json()) as { id?: unknown } | null;
      if (!body || typeof body.id !== "string" || !body.id.trim())
        throw new Error("invalid_provider_response");
      errorCode = "persistence_after_ack";
      await db
        .prepare(
          "UPDATE distribution_mail SET status='accepted',accepted_at=?,provider_id=?,lease_until=NULL,last_http_status=?,last_error_code=NULL WHERE id=?",
        )
        .bind(Date.now(), body.id, httpStatus, id)
        .run();
      return;
    }
    if (response.status === 429) errorCode = "rate_limited";
    else if (response.status >= 500) errorCode = "http_server_error";
    else if (response.status >= 400) {
      errorCode = "http_client_error";
      status = "failed";
    } else {
      errorCode = "unexpected_http_status";
      status = "failed";
    }
    // Never persist or log a provider body; cancellation cannot erase HTTP evidence.
    await response.body?.cancel().catch(() => undefined);
  } catch (error) {
    if (httpStatus === null && error instanceof Error &&
        (error.name === "TimeoutError" || error.name === "AbortError"))
      errorCode = "timeout";
  }
  await db
    .prepare(
      "UPDATE distribution_mail SET status=?,lease_until=NULL,last_http_status=?,last_error_code=?,blocked_reason=CASE WHEN ?='unknown' AND created_at<=? THEN 'retry_window_exhausted' ELSE blocked_reason END WHERE id=?",
    )
    .bind(status, httpStatus, errorCode, status, Date.now() - 23 * 3600000, id)
    .run();
}
// Only a closed, static code reaches logs: exceptions may contain provider secrets.
export function logDistributionMailFlushFailure() {
  console.error("distribution_mail_flush", { code: "flush_failed" });
}
