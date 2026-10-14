import { cliReleaseProjection } from "../cli-distribution/service";
import { recordSuccessEvent } from "../analytics/service";
import { z } from "zod";
import type { Bindings } from "../types/bindings";
import { parseRuntimeConfig } from "../config/env";
import {
  applicationInput,
  DistributionError,
  reviewInput,
  retryInput,
  type ApplicationRow,
} from "./contracts";
import { decryptLink, hash, makeLink } from "./crypto";
import { renderDistributionEmail } from "./email-template";
import { ONECREW_AI_MARK_PNG_BASE64 } from "../auth/email-assets";
import type { EmailLocale } from "../auth/email-types";
import { distributionMailEligible } from "./mail-policy";
import { sendRequestedDistributionMail } from "./outbox";
import { readDistributionSettings } from "./settings";
import { latestRelease } from "./releases";
const DAY = 86_400_000;
export async function entitlement(db: D1Database, userId: string) {
  return db
    .prepare(
      "SELECT status,revision FROM distribution_entitlement WHERE user_id=?",
    )
    .bind(userId)
    .first<{ status: "active" | "revoked"; revision: number }>();
}
export async function applicationProjection(env: Bindings, userId: string) {
  const db = env.DB;
  const app = await db
    .prepare("SELECT * FROM distribution_application WHERE user_id=?")
    .bind(userId)
    .first<ApplicationRow>();
  const email = await db
    .prepare('SELECT email FROM "user" WHERE id=?')
    .bind(userId)
    .first<{ email: string }>();
  const settings = await readDistributionSettings(db);
  return {
    applicationsOpen: settings.applicationsOpen,
    settingRevision: settings.revision,
    latestRelease: await latestRelease({ DB: db }),
    cliRelease: await cliReleaseProjection(env),
    application: app ? project(app) : null,
    entitlement: await entitlement(db, userId),
    email: email?.email ?? "",
  };
}
function project(a: ApplicationRow) {
  return {
    id: a.id,
    displayName: a.display_name,
    purpose: a.purpose,
    company: a.company,
    status: a.status,
    reviewReason: a.status === "rejected" ? a.review_reason : null,
    revision: a.revision,
    createdAt: a.created_at,
    updatedAt: a.updated_at,
  };
}
export async function applicationDetail(db: D1Database, id: string) {
  const a = await db
    .prepare("SELECT * FROM distribution_application WHERE id=?")
    .bind(id)
    .first<ApplicationRow>();
  if (!a) throw new DistributionError("not_found", 404);
  const { email } = (await db
    .prepare('SELECT email FROM "user" WHERE id=?')
    .bind(a.user_id)
    .first<{ email: string }>()) ?? { email: "" };
  const mail = await db
    .prepare(
      "SELECT id,kind,status,attempts,blocked_reason AS blockedReason,created_at AS createdAt,last_attempt_at AS lastAttemptAt,accepted_at AS acceptedAt,last_http_status AS lastHttpStatus,last_error_code AS lastErrorCode FROM distribution_mail WHERE application_id=? ORDER BY decision_revision DESC,created_at DESC,id DESC LIMIT 1",
    )
    .bind(id)
    .first<{ id: string; [key: string]: unknown }>();
  const audit = await db
    .prepare(
      "SELECT action,actor_id AS actorId,reason,created_at AS createdAt,revision FROM distribution_audit WHERE application_id=? ORDER BY revision DESC,created_at DESC,id DESC LIMIT 100",
    )
    .bind(id)
    .all();
  return {
    ...project(a),
    userId: a.user_id,
    email,
    entitlement: await entitlement(db, a.user_id),
    mail: mail ? {
      ...mail,
      canRetry: Boolean(await db.prepare(
        `SELECT 1 FROM distribution_mail m
         JOIN distribution_application a ON a.id=m.application_id
         JOIN "user" u ON u.id=a.user_id
         LEFT JOIN distribution_entitlement e ON e.user_id=a.user_id
         WHERE m.id=? AND m.status IN ('failed','unknown')
         AND (m.blocked_reason IS NULL OR m.blocked_reason='retry_window_exhausted')
         AND m.created_at>? AND (m.last_attempt_at IS NULL OR m.last_attempt_at<?)
         AND (m.lease_until IS NULL OR m.lease_until<?)
         AND a.status=m.kind AND a.decision_revision=m.decision_revision
         AND u.email=m.recipient AND u."emailVerified"=1
         AND (m.kind='rejected' OR e.status='active')`
      ).bind(mail.id, Date.now() - 23 * 3600000, Date.now() - 60000, Date.now()).first()),
    } : null,
    audit: audit.results,
  };
}
async function previous(db: D1Database, id: string, fingerprint: string) {
  const op = await db
    .prepare(
      "SELECT fingerprint,application_id FROM distribution_operation WHERE request_id=?",
    )
    .bind(id)
    .first<{ fingerprint: string; application_id: string }>();
  if (op && op.fingerprint !== fingerprint)
    throw new DistributionError("idempotency_conflict", 409);
  return op;
}
export async function submitApplication(
  env: Bindings,
  userId: string,
  input: unknown,
  requestId: string,
  locale: EmailLocale = "zh-CN",
  attribution: string | null = null,
) {
  const data = applicationInput.parse(input);
  const db = env.DB;
  const fingerprint = await hash(
    JSON.stringify({ action: "apply", userId, ...data }),
  );
  if (await previous(db, requestId, fingerprint)) {
    // A replay cannot supply the original attribution if its receipt write failed.
    return applicationProjection(env, userId);
  }
  const now = Date.now(),
    id = crypto.randomUUID();
  const existing = await db
    .prepare("SELECT * FROM distribution_application WHERE user_id=?")
    .bind(userId)
    .first<ApplicationRow>();
  if (existing && existing.status !== "rejected")
    throw new DistributionError("application_exists", 409);
  const appId = existing?.id ?? id;
  const first = existing
    ? db
        .prepare(
          "UPDATE distribution_application SET display_name=?,purpose=?,company=?,locale=?,review_reason=NULL,decision_revision=NULL,status='pending',revision=revision+1,updated_at=?,last_request_id=? WHERE id=? AND revision=? AND status='rejected' AND EXISTS(SELECT 1 FROM distribution_settings WHERE id=1 AND applications_open=1)",
        )
        .bind(
          data.displayName,
          data.purpose,
          data.company || null,
          locale,
          now,
          requestId,
          appId,
          existing.revision,
        )
    : db
        .prepare(
          "INSERT INTO distribution_application(id,user_id,display_name,purpose,company,locale,status,revision,created_at,updated_at,last_request_id) SELECT ?,?,?,?,?,?,'pending',1,?,?,? WHERE EXISTS(SELECT 1 FROM distribution_settings WHERE id=1 AND applications_open=1)",
        )
        .bind(
          id,
          userId,
          data.displayName,
          data.purpose,
          data.company || null,
          locale,
          now,
          now,
          requestId,
        );
  let committedHere = false;
  try {
    await db.batch([
      first,
      db
        .prepare(
          "INSERT INTO distribution_operation SELECT ?,?,id,? FROM distribution_application WHERE id=? AND last_request_id=?",
        )
        .bind(requestId, fingerprint, now, appId, requestId),
      db
        .prepare(
          "INSERT INTO distribution_audit SELECT ?,id,'submitted',?,NULL,revision,? FROM distribution_application WHERE id=? AND last_request_id=?",
        )
        .bind(crypto.randomUUID(), userId, now, appId, requestId),
    ]);
    committedHere = true;
  } catch {
    if (!(await previous(db, requestId, fingerprint)))
      throw new DistributionError("application_exists", 409);
  }
  if (!(await previous(db, requestId, fingerprint))) {
    const open = await db
      .prepare("SELECT applications_open FROM distribution_settings WHERE id=1")
      .first<{ applications_open: number }>();
    if (!open?.applications_open)
      throw new DistributionError("applications_closed", 403);
    throw new DistributionError("revision_conflict", 409);
  }
  if (committedHere) await recordSuccessEvent(db, userId, "generate_lead", requestId, attribution);
  return applicationProjection(env, userId);
}
export async function listApplications(env: Bindings, input: unknown) {
  const data = z
    .object({
      status: z.enum(["pending", "approved", "rejected"]).optional(),
      limit: z.number().int().min(1).max(100).optional(),
      cursor: z.string().uuid().optional(),
    })
    .strict()
    .parse(input);
  const db = env.DB;
  const result = await db
    .prepare(
      "SELECT id FROM distribution_application WHERE (? IS NULL OR status=?) AND (? IS NULL OR id>?) ORDER BY id LIMIT ?",
    )
    .bind(
      data.status ?? null,
      data.status ?? null,
      data.cursor ?? null,
      data.cursor ?? null,
      (data.limit ?? 30) + 1,
    )
    .all<{ id: string }>();
  const rows = result.results.slice(0, data.limit ?? 30);
  const count = await db
    .prepare(
      "SELECT count(*) AS count FROM distribution_application WHERE status='pending'",
    )
    .first<{ count: number }>();
  return {
    ok: true,
    applications: await Promise.all(
      rows.map((a) => applicationDetail(db, a.id)),
    ),
    nextCursor: result.results.length > rows.length ? rows.at(-1)!.id : null,
    pendingCount: count?.count ?? 0,
  };
}
export async function reviewApplication(env: Bindings, input: unknown) {
  const data = reviewInput.parse(input),
    db = env.DB;
  const fingerprint = await hash(JSON.stringify({ action: "review", ...data }));
  if (await previous(db, data.requestId, fingerprint))
    return {
      ok: true,
      application: await applicationDetail(db, data.applicationId),
    };
  const app = await db
    .prepare("SELECT * FROM distribution_application WHERE id=?")
    .bind(data.applicationId)
    .first<ApplicationRow>();
  if (!app) throw new DistributionError("not_found", 404);
  if (app.revision !== data.expectedRevision)
    throw new DistributionError("revision_conflict", 409);
  const grant = await entitlement(db, app.user_id);
  if (
    data.decision === "revoked"
      ? grant?.status !== "active"
      : app.status !== "pending"
  )
    throw new DistributionError("invalid_transition", 409);
  const now = Date.now();
  let mailPayload: string | undefined;
  let mailRecipient: string | null = null;
  if (data.decision !== "revoked") {
    let config;
    try {
      config = parseRuntimeConfig(env);
    } catch {
      throw new DistributionError("configuration_required", 503);
    }
    if (!config.resend)
      throw new DistributionError("configuration_required", 503);
    const verified = await db
      .prepare(
        'SELECT email,"emailVerified" AS verified FROM "user" WHERE id=?',
      )
      .bind(app.user_id)
      .first<{ email: string; verified: number }>();
    if (!verified?.verified)
      throw new DistributionError("email_verification_required", 403);
    mailRecipient = verified.email;
    const content = renderDistributionEmail({
      decision: data.decision,
      locale: app.locale,
      displayName: app.display_name,
      ...(data.decision === "rejected" ? { reviewReason: data.reason! } : {}),
      accountUrl: `${config.authOrigin}/account?apply=1`,
    });
    mailPayload = JSON.stringify({
      from: config.resend.from,
      to: [verified.email],
      ...content,
      attachments: [
        {
          content: ONECREW_AI_MARK_PNG_BASE64,
          content_id: "onecrew-mark",
          content_type: "image/png",
          filename: "onecrew-ai-mark.png",
        },
      ],
    });
  }
  const statements = [
    db
      .prepare(
        'UPDATE distribution_application SET status=?,review_reason=?,decision_revision=revision+1,revision=revision+1,updated_at=?,last_request_id=? WHERE id=? AND revision=? AND (? IS NULL OR EXISTS(SELECT 1 FROM "user" u WHERE u.id=distribution_application.user_id AND u.email=? AND u."emailVerified"=1))',
      )
      .bind(
        data.decision === "revoked" ? app.status : data.decision,
        data.decision === "rejected" ? data.reason! : null,
        now,
        data.requestId,
        app.id,
        data.expectedRevision,
        mailRecipient,
        mailRecipient,
      ),
    db
      .prepare(
        "INSERT INTO distribution_operation SELECT ?,?,id,? FROM distribution_application WHERE id=? AND last_request_id=?",
      )
      .bind(data.requestId, fingerprint, now, app.id, data.requestId),
    db
      .prepare(
        "INSERT INTO distribution_audit SELECT ?,id,?,?,?,revision,? FROM distribution_application WHERE id=? AND last_request_id=?",
      )
      .bind(
        crypto.randomUUID(),
        data.decision,
        data.actorId,
        data.reason ?? null,
        now,
        app.id,
        data.requestId,
      ),
  ];
  if (data.decision !== "rejected")
    statements.push(
      db
        .prepare(
          "INSERT INTO distribution_entitlement(user_id,status,revision,updated_at) SELECT user_id,?,1,? FROM distribution_application WHERE id=? AND last_request_id=? ON CONFLICT(user_id) DO UPDATE SET status=excluded.status,revision=distribution_entitlement.revision+1,updated_at=excluded.updated_at",
        )
        .bind(
          data.decision === "approved" ? "active" : "revoked",
          now,
          app.id,
          data.requestId,
        ),
    );
  if (mailPayload)
    statements.push(
      db
        .prepare(
          "INSERT INTO distribution_mail(id,application_id,kind,decision_revision,recipient,payload_json,status,created_at) SELECT ?,a.id,?,a.decision_revision,?,?,'pending',? FROM distribution_application a WHERE a.id=? AND a.last_request_id=?",
        )
        .bind(
          crypto.randomUUID(),
          data.decision,
          mailRecipient,
          mailPayload,
          now,
          app.id,
          data.requestId,
        ),
    );
  try {
    await db.batch(statements);
  } catch {
    if (!(await previous(db, data.requestId, fingerprint)))
      throw new DistributionError("unavailable", 503);
  }
  if (!(await previous(db, data.requestId, fingerprint)))
    throw new DistributionError("revision_conflict", 409);
  return { ok: true, application: await applicationDetail(db, app.id) };
}
export async function retryEmail(env: Bindings, input: unknown) {
  const data = retryInput.parse(input),
    db = env.DB,
    fingerprint = await hash(JSON.stringify({ action: "retry", ...data }));
  if (await previous(db, data.requestId, fingerprint))
    return {
      ok: true,
      application: await applicationDetail(db, data.applicationId),
    };
  if (!parseRuntimeConfig(env).resend)
    throw new DistributionError("configuration_required", 503);
  const app = await db
    .prepare("SELECT * FROM distribution_application WHERE id=?")
    .bind(data.applicationId)
    .first<ApplicationRow>();
  if (!app) throw new DistributionError("not_found", 404);
  if (app.revision !== data.expectedRevision)
    throw new DistributionError("revision_conflict", 409);
  const mail = await db
    .prepare(
      "SELECT * FROM distribution_mail WHERE application_id=? ORDER BY decision_revision DESC,created_at DESC,id DESC LIMIT 1",
    )
    .bind(app.id)
    .first<{
      id: string;
      status: string;
      attempts: number;
      created_at: number;
      last_attempt_at: number | null;
      blocked_reason: string | null;
      application_id: string;
      kind: "approved" | "rejected";
      decision_revision: number;
      recipient: string;
    }>();
  const now = Date.now();
  if (
    !mail ||
    mail.id !== data.expectedMailEventId ||
    !["failed", "unknown"].includes(mail.status) ||
    (mail.blocked_reason !== null && mail.blocked_reason !== "retry_window_exhausted") ||
    now - mail.created_at >= 23 * 3600000 ||
    now - (mail.last_attempt_at ?? 0) < 60000 ||
    !(await distributionMailEligible(db, mail))
  )
    throw new DistributionError("invalid_transition", 409);
  try {
    await db.batch([
      db
        .prepare(
          "UPDATE distribution_application SET revision=revision+1,updated_at=?,last_request_id=? WHERE id=? AND revision=? AND EXISTS(SELECT 1 FROM distribution_mail WHERE id=? AND (blocked_reason IS NULL OR blocked_reason='retry_window_exhausted') AND status IN ('failed','unknown') AND created_at>? AND (last_attempt_at IS NULL OR last_attempt_at<?) AND (lease_until IS NULL OR lease_until<?))",
        )
        .bind(
          now,
          data.requestId,
          app.id,
          data.expectedRevision,
          mail.id,
          now - 23 * 3600000,
          now - 60000,
          now,
        ),
      db
        .prepare(
          "INSERT INTO distribution_operation SELECT ?,?,id,? FROM distribution_application WHERE id=? AND last_request_id=?",
        )
        .bind(data.requestId, fingerprint, now, app.id, data.requestId),
      db
        .prepare(
          "UPDATE distribution_mail SET blocked_reason=NULL,manual_request_id=?,lease_until=NULL WHERE id=? AND EXISTS(SELECT 1 FROM distribution_application WHERE id=? AND last_request_id=?)",
        )
        .bind(data.requestId, mail.id, app.id, data.requestId),
      db
        .prepare(
          "INSERT INTO distribution_audit SELECT ?,id,'email_retry',?,NULL,revision,? FROM distribution_application WHERE id=? AND last_request_id=?",
        )
        .bind(crypto.randomUUID(), data.actorId, now, app.id, data.requestId),
    ]);
  } catch {
    if (!(await previous(db, data.requestId, fingerprint)))
      throw new DistributionError("unavailable", 503);
  }
  if (!(await previous(db, data.requestId, fingerprint)))
    throw new DistributionError("revision_conflict", 409);
  await sendRequestedDistributionMail(env, mail.id, data.requestId);
  return { ok: true, application: await applicationDetail(db, app.id) };
}
export async function createUserLink(
  env: Bindings,
  userId: string,
  requestId: string,
) {
  const db = env.DB;
  const grant = await entitlement(db, userId);
  if (grant?.status !== "active")
    throw new DistributionError("entitlement_revoked", 403);
  const prior = await db
    .prepare(
      "SELECT user_id,encrypted_token,expires_at FROM distribution_link WHERE request_id=?",
    )
    .bind(requestId)
    .first<{ user_id: string; encrypted_token: string; expires_at: number }>();
  if (prior) {
    if (prior.user_id !== userId)
      throw new DistributionError("idempotency_conflict", 409);
    return {
      url: `/d/${await decryptLink(env, prior.encrypted_token)}`,
      expiresAt: prior.expires_at,
    };
  }
  const now = Date.now(),
    material = await makeLink(env);
  let inserted: D1Result;
  try {
    inserted = await db
      .prepare(
        "INSERT INTO distribution_link SELECT ?,user_id,?,?,?,?,revision FROM distribution_entitlement WHERE user_id=? AND status='active' AND revision=? AND NOT EXISTS(SELECT 1 FROM distribution_link WHERE user_id=? AND created_at>?)",
      )
      .bind(
        material.tokenHash,
        material.encryptedToken,
        requestId,
        now,
        now + DAY,
        userId,
        grant.revision,
        userId,
        now - 60000,
      )
      .run();
  } catch {
    const concurrent = await db
      .prepare(
        "SELECT user_id,encrypted_token,expires_at FROM distribution_link WHERE request_id=?",
      )
      .bind(requestId)
      .first<{
        user_id: string;
        encrypted_token: string;
        expires_at: number;
      }>();
    if (!concurrent) throw new DistributionError("unavailable", 503);
    if (concurrent.user_id !== userId)
      throw new DistributionError("idempotency_conflict", 409);
    return {
      url: `/d/${await decryptLink(env, concurrent.encrypted_token)}`,
      expiresAt: concurrent.expires_at,
    };
  }
  if (!inserted.meta.changes) {
    const concurrent = await db
      .prepare(
        "SELECT user_id,encrypted_token,expires_at FROM distribution_link WHERE request_id=?",
      )
      .bind(requestId)
      .first<{
        user_id: string;
        encrypted_token: string;
        expires_at: number;
      }>();
    if (concurrent?.user_id === userId)
      return {
        url: `/d/${await decryptLink(env, concurrent.encrypted_token)}`,
        expiresAt: concurrent.expires_at,
      };
    throw new DistributionError("rate_limited", 429);
  }
  return { url: `/d/${material.token}`, expiresAt: now + DAY };
}
export async function authorizeLink(
  env: Bindings,
  userId: string,
  token: string,
) {
  const link = await env.DB.prepare(
    "SELECT user_id,expires_at,entitlement_revision FROM distribution_link WHERE token_hash=?",
  )
    .bind(await hash(token))
    .first<{
      user_id: string;
      expires_at: number;
      entitlement_revision: number;
    }>();
  if (!link || link.user_id !== userId)
    throw new DistributionError("link_not_found", 404);
  if (link.expires_at <= Date.now())
    throw new DistributionError("link_expired", 410);
  const grant = await entitlement(env.DB, userId);
  if (
    grant?.status !== "active" ||
    grant.revision !== link.entitlement_revision
  )
    throw new DistributionError("entitlement_revoked", 403);
  return link;
}
