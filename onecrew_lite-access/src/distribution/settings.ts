import { cliReleaseProjection } from "../cli-distribution/service";
import { z } from "zod";
import type { Bindings } from "../types/bindings";
import { DistributionError } from "./contracts";
import { hash } from "./crypto";
import { latestRelease } from "./releases";
export async function readDistributionSettings(db: D1Database) {
  const row = await db
    .prepare(
      "SELECT applications_open,revision FROM distribution_settings WHERE id=1",
    )
    .first<{ applications_open: number; revision: number }>();
  if (!row) throw new DistributionError("configuration_required", 503);
  return {
    applicationsOpen: row.applications_open === 1,
    revision: row.revision,
  };
}
export async function getDistributionSettings(env: Bindings, input: unknown) {
  z.object({}).strict().parse(input);
  const settings = await readDistributionSettings(env.DB);
  const latest = await latestRelease(env);
  return {
    ok: true,
    settings: {
      ...settings,
      cliRelease: await cliReleaseProjection(env),
      releaseReady: latest !== null,
      latestVersion: latest?.version ?? null,
    },
  };
}
const updateInput = z
  .object({
    applicationsOpen: z.boolean(),
    expectedRevision: z.number().int().positive(),
    requestId: z.string().uuid(),
    actorId: z.string().min(1).max(200),
  })
  .strict();
export async function setDistributionSettings(env: Bindings, input: unknown) {
  const data = updateInput.parse(input),
    db = env.DB,
    fingerprint = await hash(JSON.stringify(data));
  const prior = await db
    .prepare(
      "SELECT fingerprint FROM distribution_settings_audit WHERE request_id=?",
    )
    .bind(data.requestId)
    .first<{ fingerprint: string }>();
  if (prior) {
    if (prior.fingerprint !== fingerprint)
      throw new DistributionError("idempotency_conflict", 409);
    return getDistributionSettings(env, {});
  }
  const now = Date.now();
  try {
    await db.batch([
      db
        .prepare(
          "UPDATE distribution_settings SET applications_open=?,revision=revision+1,updated_at=?,last_request_id=? WHERE id=1 AND revision=?",
        )
        .bind(
          data.applicationsOpen ? 1 : 0,
          now,
          data.requestId,
          data.expectedRevision,
        ),
      db
        .prepare(
          "INSERT INTO distribution_settings_audit SELECT ?,?,?,applications_open,revision,? FROM distribution_settings WHERE id=1 AND last_request_id=?",
        )
        .bind(data.requestId, fingerprint, data.actorId, now, data.requestId),
    ]);
  } catch {
    const replay = await db
      .prepare(
        "SELECT fingerprint FROM distribution_settings_audit WHERE request_id=?",
      )
      .bind(data.requestId)
      .first<{ fingerprint: string }>();
    if (!replay) throw new DistributionError("unavailable", 503);
    if (replay.fingerprint !== fingerprint)
      throw new DistributionError("idempotency_conflict", 409);
  }
  const completed = await db
    .prepare(
      "SELECT fingerprint FROM distribution_settings_audit WHERE request_id=?",
    )
    .bind(data.requestId)
    .first<{ fingerprint: string }>();
  if (!completed) throw new DistributionError("revision_conflict", 409);
  if (completed.fingerprint !== fingerprint)
    throw new DistributionError("idempotency_conflict", 409);
  return getDistributionSettings(env, {});
}
