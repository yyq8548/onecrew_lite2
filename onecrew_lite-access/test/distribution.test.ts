import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import app from "../src/index";
import * as sessionModule from "../src/auth/session";
import * as identityModule from "../src/devices/credential-identity";
import {
  applicationDetail,
  applicationProjection,
  authorizeLink,
  createUserLink,
  entitlement,
  reviewApplication,
  retryEmail,
  submitApplication,
} from "../src/distribution/service";
import { decryptLink, hash } from "../src/distribution/crypto";
import {
  getDistributionSettings,
  setDistributionSettings,
} from "../src/distribution/settings";
import { flushDistributionMail } from "../src/distribution/outbox";
import {
  compareVersions,
  getAsset,
  parseRange,
  publishRelease,
  readRelease,
  streamAsset,
} from "../src/distribution/releases";
import {
  releaseInput,
  safeDownloadReturnTo,
} from "../src/distribution/contracts";
const base = env as unknown as CloudflareBindings & {
  TEST_MIGRATIONS: D1Migration[];
};
const db = base.DB;
const object = new Uint8Array([1, 2, 3, 4, 5]);
const bucket = {
  head: vi.fn(async () => ({
    size: 5,
    etag: "etag",
    httpEtag: '"etag"',
    customMetadata: { sha256: "a".repeat(64) },
  })),
  get: vi.fn(async () => ({ body: new Blob([object]).stream() })),
} as unknown as R2Bucket;
const bindings = {
  ...base,
  RELEASES: bucket,
  RELEASE_LINK_SECRET: "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8",
  RESEND_API_KEY: "test-resend-placeholder",
  RESEND_FROM_EMAIL: "OneCrew <noreply@example.com>",
};
const user = "distribution-user";
let applicationId: string;
const uuid = () => crypto.randomUUID();
const review = (
  decision: "approved" | "rejected" | "revoked",
  revision = 1,
  requestId = uuid(),
) => ({
  applicationId,
  expectedRevision: revision,
  decision,
  ...(decision === "rejected"
    ? { reason: "Please clarify your intended use." }
    : {}),
  requestId,
  actorId: "test-admin",
});
beforeEach(async () => {
  vi.restoreAllMocks();
  await applyD1Migrations(db, base.TEST_MIGRATIONS);
  await db.batch(
    [
      "distribution_mail",
      "distribution_link",
      "distribution_audit",
      "distribution_operation",
      "distribution_asset",
      "distribution_release",
      "distribution_entitlement",
      "distribution_application",
      "distribution_settings_audit",
    ].map((t) => db.prepare(`DELETE FROM ${t}`)),
  );
  await db
    .prepare(
      "UPDATE distribution_settings SET applications_open=1,revision=1,last_request_id=NULL,updated_at=0 WHERE id=1",
    )
    .run();
  await db.prepare('DELETE FROM "user" WHERE id=?').bind(user).run();
  await db
    .prepare(
      'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)',
    )
    .bind(
      user,
      "Tester",
      "distribution@example.com",
      "2026-01-01",
      "2026-01-01",
    )
    .run();
  const projection = await submitApplication(
    bindings,
    user,
    { displayName: "Tester", purpose: "Test download" },
    uuid(),
  );
  applicationId = projection.application!.id;
});
afterEach(() => vi.unstubAllGlobals());
async function approvalLink() {
  await reviewApplication(bindings, review("approved"));
  return (await createUserLink(bindings, user, uuid())).url.slice(3);
}
function release(version = "1.2.3") {
  return {
    version,
    notes: "Notes",
    pubDate: "2026-09-06T00:00:00Z",
    assets: [
      {
        id: uuid(),
        key: `releases/${version}/OneCrew.dmg`,
        filename: "OneCrew.dmg",
        platform: "darwin",
        arch: "aarch64",
        kind: "installer",
        size: 5,
        sha256: "a".repeat(64),
      },
      {
        id: uuid(),
        key: `releases/${version}/OneCrew.app.tar.gz`,
        filename: "OneCrew.app.tar.gz",
        platform: "darwin",
        arch: "aarch64",
        kind: "updater",
        size: 5,
        sha256: "a".repeat(64),
        signature: "signed-fixture",
      },
    ],
  };
}
describe("Access application and private distribution", () => {
  it("does not grant historical accounts; approval config failure leaves pending with no mail or grant", async () => {
    expect(await entitlement(db, user)).toBeNull();
    await expect(
      reviewApplication(
        { ...bindings, RESEND_FROM_EMAIL: "" },
        review("approved"),
      ),
    ).rejects.toMatchObject({ code: "configuration_required" });
    expect((await applicationDetail(db, applicationId)).status).toBe("pending");
    expect(await entitlement(db, user)).toBeNull();
    expect(
      await db.prepare("SELECT count(*) n FROM distribution_mail").first(),
    ).toEqual({ n: 0 });
  });
  it("atomically approves without mail tokens, and creates encrypted links only on account request", async () => {
    const input = review("approved");
    const result = await reviewApplication(bindings, input);
    expect(result.application.entitlement).toEqual({
      status: "active",
      revision: 1,
    });
    expect(result.application.mail).toMatchObject({
      status: "pending",
      attempts: 0,
    });
    await reviewApplication(bindings, input);
    expect((await applicationDetail(db, applicationId)).revision).toBe(2);
    expect(
      await db.prepare("SELECT count(*) n FROM distribution_link").first(),
    ).toEqual({ n: 0 });
    await createUserLink(bindings, user, uuid());
    const row = await db.prepare("SELECT * FROM distribution_link").first<{
      encrypted_token: string;
      token_hash: string;
      created_at: number;
      expires_at: number;
    }>();
    const token = await decryptLink(bindings, row!.encrypted_token);
    expect(token).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(row!.token_hash).toBe(await hash(token));
    expect(JSON.stringify(row)).not.toContain(token);
    expect(row!.expires_at - row!.created_at).toBe(86400000);
    await expect(
      reviewApplication(bindings, {
        ...input,
        decision: "rejected",
        reason: "Changed decision",
      }),
    ).rejects.toMatchObject({ code: "idempotency_conflict" });
  });
  it("requires a public rejection reason and sends a branded rejected notice without a grant", async () => {
    await expect(
      reviewApplication(bindings, { ...review("rejected"), reason: "   " }),
    ).rejects.toThrow();
    expect((await applicationDetail(db, applicationId)).status).toBe("pending");
    const reason = "Please describe your intended workflow <team>.";
    const result = await reviewApplication(bindings, {
      ...review("rejected"),
      reason,
    });
    expect(result.application).toMatchObject({
      status: "rejected",
      reviewReason: reason,
      entitlement: null,
      mail: { kind: "rejected", status: "pending" },
    });
    expect((await applicationProjection(bindings, user)).application).toMatchObject({
      status: "rejected",
      reviewReason: reason,
    });
    expect(
      await db.prepare("SELECT count(*) n FROM distribution_link").first(),
    ).toEqual({ n: 0 });
    const send = vi.fn(async () => Response.json({ id: "rejection-mail" }));
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).toHaveBeenCalledTimes(1);
    const payload = JSON.parse(
      String((send.mock.calls[0] as unknown as [string, RequestInit])[1].body),
    );
    expect(payload.text).toContain(reason);
    expect(payload.html).toContain("&lt;team&gt;");
    expect(payload.html).toContain("/account?apply=1");
    expect(payload.html).not.toMatch(/\/d\/[A-Za-z0-9_-]{43}/);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      kind: "rejected",
      status: "accepted",
    });
  });
  it("aborts a review atomically if the verified recipient changes before commit", async () => {
    const originalBatch = db.batch.bind(db);
    const changedDb = {
      prepare: db.prepare.bind(db),
      exec: db.exec.bind(db),
      batch: async (statements: D1PreparedStatement[]) => {
        await db
          .prepare('UPDATE "user" SET email=? WHERE id=?')
          .bind("changed-before-commit@example.com", user)
          .run();
        return originalBatch(statements);
      },
    } as D1Database;
    await expect(
      reviewApplication({ ...bindings, DB: changedDb }, review("approved")),
    ).rejects.toMatchObject({ code: "revision_conflict" });
    expect((await applicationDetail(db, applicationId)).status).toBe("pending");
    expect(await entitlement(db, user)).toBeNull();
    expect(
      await db.prepare("SELECT count(*) n FROM distribution_mail").first(),
    ).toEqual({ n: 0 });
  });
  it("resubmission preserves audit, persists request locale and suppresses stale rejection mail", async () => {
    await reviewApplication(bindings, {
      ...review("rejected"),
      reason: "More detail needed",
    });
    const firstMail = (await applicationDetail(db, applicationId)).mail!.id;
    vi.spyOn(sessionModule, "loadSafeSession").mockResolvedValue({
      headers: new Headers(),
      session: {
        expiresAt: new Date(Date.now() + 3600000).toISOString(),
        sessionId: "test-session",
        user: {
          id: user,
          email: "distribution@example.com",
          name: "Tester",
          image: null,
        },
      },
    });
    const response = await app.request(
      "http://localhost:8787/api/distribution/application",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Origin: bindings.ACCESS_ORIGIN,
          "Accept-Language": "en-US",
          "Idempotency-Key": uuid(),
        },
        body: JSON.stringify({
          displayName: "Updated tester",
          purpose: "A detailed revised workflow",
        }),
      },
      bindings,
    );
    expect(response.status).toBe(200);
    expect((await applicationProjection(bindings, user)).application).toMatchObject({
      status: "pending",
      revision: 3,
      reviewReason: null,
      displayName: "Updated tester",
    });
    expect((await applicationDetail(db, applicationId)).audit).toContainEqual(
      expect.objectContaining({
        action: "rejected",
        reason: "More detail needed",
        revision: 2,
      }),
    );
    const send = vi.fn(async () => Response.json({ id: "current-mail" }));
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).not.toHaveBeenCalled();
    expect(
      await db
        .prepare(
          "SELECT status,blocked_reason FROM distribution_mail WHERE id=?",
        )
        .bind(firstMail)
        .first(),
    ).toEqual({
      status: "failed",
      blocked_reason: "decision_or_recipient_ineligible",
    });
    await reviewApplication(bindings, {
      ...review("rejected", 3),
      reason: "Please include team size",
    });
    await flushDistributionMail(bindings);
    const payload = JSON.parse(
      String((send.mock.calls[0] as unknown as [string, RequestInit])[1].body),
    );
    expect(payload.subject).toBe("An update on your OneCrew application");
    expect(payload.text).toContain("Please include team size");
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      kind: "rejected",
      status: "accepted",
    });
  });
  it("preserves unknown rejection outcome after resubmission and never resends stale decision", async () => {
    await reviewApplication(bindings, review("rejected"));
    const send = vi.fn(async () => {
      throw new Error("timeout");
    });
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    await submitApplication(
      bindings,
      user,
      { displayName: "Revised", purpose: "Revised purpose" },
      uuid(),
    );
    await db
      .prepare("UPDATE distribution_mail SET last_attempt_at=?")
      .bind(Date.now() - 61000)
      .run();
    await flushDistributionMail(bindings);
    await flushDistributionMail(bindings);
    expect(send).toHaveBeenCalledTimes(1);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      kind: "rejected",
      status: "unknown",
      attempts: 1,
      blockedReason: null,
      canRetry: false,
    });
  });
  it("retries rejected mail without entitlement and retains immutable HTML payload", async () => {
    await reviewApplication(bindings, review("rejected"));
    const send = vi.fn(async () => {
      throw new Error("timeout");
    });
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    const first = (send.mock.calls[0] as unknown as [string, RequestInit])[1];
    await db
      .prepare("UPDATE distribution_mail SET last_attempt_at=?")
      .bind(Date.now() - 61000)
      .run();
    const current = await applicationDetail(db, applicationId);
    const accepted = vi.fn(async () => Response.json({ id: "retry-accepted" }));
    vi.stubGlobal("fetch", accepted);
    await retryEmail({ ...bindings, RESEND_FROM_EMAIL: "Changed <different@example.com>" }, {
      applicationId,
      expectedRevision: 2,
      expectedMailEventId: current.mail!.id,
      requestId: uuid(),
      actorId: "test-admin",
    });
    await flushDistributionMail({
      ...bindings,
      RESEND_FROM_EMAIL: "Changed <different@example.com>",
    });
    expect(
      (accepted.mock.calls[0] as unknown as [string, RequestInit])[1].body,
    ).toBe(first.body);
    expect(await entitlement(db, user)).toBeNull();
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      kind: "rejected",
      status: "accepted",
      attempts: 2,
    });
  });
  it("publishes authoritative application settings with optimistic revision, idempotency and audit", async () => {
    const input = {
      applicationsOpen: false,
      expectedRevision: 1,
      requestId: uuid(),
      actorId: "test-admin",
    };
    const closed = await setDistributionSettings(bindings, input);
    expect(closed.settings).toMatchObject({
      applicationsOpen: false,
      revision: 2,
      releaseReady: false,
      latestVersion: null,
    });
    expect(await setDistributionSettings(bindings, input)).toEqual(closed);
    await expect(
      setDistributionSettings(bindings, { ...input, applicationsOpen: true }),
    ).rejects.toMatchObject({ code: "idempotency_conflict" });
    await expect(
      setDistributionSettings(bindings, { ...input, requestId: uuid() }),
    ).rejects.toMatchObject({ code: "revision_conflict" });
    const open = await setDistributionSettings(bindings, {
      applicationsOpen: true,
      expectedRevision: 2,
      requestId: uuid(),
      actorId: "test-admin",
    });
    expect(open.settings).toMatchObject({
      applicationsOpen: true,
      releaseReady: false,
    });
    expect(
      await db
        .prepare("SELECT count(*) n FROM distribution_settings_audit")
        .first(),
    ).toEqual({ n: 2 });
    await publishRelease(bindings, release(), uuid());
    expect(
      (await getDistributionSettings(bindings, {})).settings,
    ).toMatchObject({ releaseReady: true, latestVersion: "1.2.3" });
    expect(await applicationProjection(bindings, user)).toMatchObject({
      applicationsOpen: true,
      settingRevision: 3,
      latestRelease: { version: "1.2.3" },
    });
  });
  it("rejects a delayed same UUID with different payload after two real D1 settings commits", async () => {
    const sharedId = uuid(),
      thirdId = uuid();
    let reachedBatch!: () => void;
    let releaseBatch!: () => void;
    const reached = new Promise<void>((resolve) => {
      reachedBatch = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      releaseBatch = resolve;
    });
    const delayedDb = {
      prepare: db.prepare.bind(db),
      exec: db.exec.bind(db),
      batch: async (statements: D1PreparedStatement[]) => {
        reachedBatch();
        await gate;
        return db.batch(statements);
      },
    } as D1Database;
    const delayed = setDistributionSettings(
      { ...bindings, DB: delayedDb },
      {
        applicationsOpen: true,
        expectedRevision: 1,
        requestId: sharedId,
        actorId: "actor-B",
      },
    );
    await reached;
    await setDistributionSettings(bindings, {
      applicationsOpen: false,
      expectedRevision: 1,
      requestId: sharedId,
      actorId: "actor-A",
    });
    const third = await setDistributionSettings(bindings, {
      applicationsOpen: true,
      expectedRevision: 2,
      requestId: thirdId,
      actorId: "actor-C",
    });
    releaseBatch();
    await expect(delayed).rejects.toMatchObject({
      code: "idempotency_conflict",
    });
    expect(await getDistributionSettings(bindings, {})).toEqual(third);
    expect(third.settings).toMatchObject({
      applicationsOpen: true,
      revision: 3,
    });
    const audit = await db
      .prepare(
        "SELECT request_id,actor_id,applications_open,revision FROM distribution_settings_audit ORDER BY revision",
      )
      .all();
    expect(audit.results).toEqual([
      {
        request_id: sharedId,
        actor_id: "actor-A",
        applications_open: 0,
        revision: 2,
      },
      {
        request_id: thirdId,
        actor_id: "actor-C",
        applications_open: 1,
        revision: 3,
      },
    ]);
  });
  it("closed intake blocks new and revised submissions while completed request replay still succeeds", async () => {
    await reviewApplication(bindings, review("rejected"));
    const id = uuid(),
      input = { displayName: "Updated", purpose: "New detail" };
    await setDistributionSettings(bindings, {
      applicationsOpen: false,
      expectedRevision: 1,
      requestId: uuid(),
      actorId: "test-admin",
    });
    await expect(
      submitApplication(bindings, user, input, id),
    ).rejects.toMatchObject({ code: "applications_closed" });
    expect((await applicationDetail(db, applicationId)).status).toBe(
      "rejected",
    );
    await setDistributionSettings(bindings, {
      applicationsOpen: true,
      expectedRevision: 2,
      requestId: uuid(),
      actorId: "test-admin",
    });
    await submitApplication(bindings, user, input, id);
    await setDistributionSettings(bindings, {
      applicationsOpen: false,
      expectedRevision: 3,
      requestId: uuid(),
      actorId: "test-admin",
    });
    expect(
      await submitApplication(bindings, user, input, id, "en-US"),
    ).toMatchObject({
      applicationsOpen: false,
      application: { status: "pending", revision: 3 },
    });
    await db
      .prepare(
        'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)',
      )
      .bind(
        "new-closed-user",
        "New",
        "new-closed@example.com",
        "2026-01-01",
        "2026-01-01",
      )
      .run();
    await expect(
      submitApplication(bindings, "new-closed-user", input, uuid()),
    ).rejects.toMatchObject({ code: "applications_closed" });
    expect(
      (await applicationProjection(bindings, "new-closed-user")).application,
    ).toBeNull();
    await reviewApplication(bindings, review("approved", 3));
    const link = await createUserLink(bindings, user, uuid());
    expect(link.url).toMatch(/^\/d\//);
  });
  it("closing intake and submission serialize through the same database predicate", async () => {
    await reviewApplication(bindings, review("rejected"));
    const submissionId = uuid();
    const results = await Promise.allSettled([
      setDistributionSettings(bindings, {
        applicationsOpen: false,
        expectedRevision: 1,
        requestId: uuid(),
        actorId: "test-admin",
      }),
      submitApplication(
        bindings,
        user,
        { displayName: "Race", purpose: "Revised purpose" },
        submissionId,
      ),
    ]);
    expect(results[0]!.status).toBe("fulfilled");
    const current = await applicationDetail(db, applicationId);
    if (results[1]!.status === "rejected")
      expect(current).toMatchObject({ status: "rejected", revision: 2 });
    else expect(current).toMatchObject({ status: "pending", revision: 3 });
    expect(
      (await getDistributionSettings(bindings, {})).settings.applicationsOpen,
    ).toBe(false);
    await expect(
      submitApplication(
        bindings,
        user,
        { displayName: "Later", purpose: "Later attempt" },
        uuid(),
      ),
    ).rejects.toMatchObject({
      code:
        current.status === "pending"
          ? "application_exists"
          : "applications_closed",
    });
  });
  it("serializes competing reviews with revision guards and single mail", async () => {
    const results = await Promise.allSettled([
      reviewApplication(bindings, review("approved")),
      reviewApplication(bindings, review("rejected")),
    ]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    const a = await applicationDetail(db, applicationId);
    expect(a.revision).toBe(2);
    expect(
      await db.prepare("SELECT count(*) n FROM distribution_audit").first(),
    ).toEqual({ n: 2 });
  });
  it("requires same user, does not consume GET/HEAD, checks expiry and immediate revocation", async () => {
    const token = await approvalLink();
    await expect(
      authorizeLink(bindings, "different-user", token),
    ).rejects.toMatchObject({ code: "link_not_found" });
    await authorizeLink(bindings, user, token);
    await authorizeLink(bindings, user, token);
    await reviewApplication(bindings, review("revoked", 2));
    await expect(authorizeLink(bindings, user, token)).rejects.toMatchObject({
      code: "entitlement_revoked",
    });
    await db
      .prepare("UPDATE distribution_link SET expires_at=?")
      .bind(Date.now() - 1)
      .run();
    await expect(authorizeLink(bindings, user, token)).rejects.toMatchObject({
      code: "link_expired",
    });
  });
  it("rejects unverified approvals with no partial effects", async () => {
    await db
      .prepare('UPDATE "user" SET emailVerified=0 WHERE id=?')
      .bind(user)
      .run();
    await expect(
      reviewApplication(bindings, review("approved")),
    ).rejects.toMatchObject({ code: "email_verification_required" });
    expect((await applicationDetail(db, applicationId)).revision).toBe(1);
  });
  it("issues reusable idempotent 24h account links with cooldown and current entitlement", async () => {
    await approvalLink();
    await db
      .prepare("UPDATE distribution_link SET created_at=?")
      .bind(Date.now() - 61000)
      .run();
    const id = uuid();
    const concurrentLinks = await Promise.all([
      createUserLink(bindings, user, id),
      createUserLink(bindings, user, id),
    ]);
    expect(concurrentLinks[0]).toEqual(concurrentLinks[1]);
    const link = concurrentLinks[0]!;
    expect(await createUserLink(bindings, user, id)).toEqual(link);
    await expect(createUserLink(bindings, user, uuid())).rejects.toMatchObject({
      code: "rate_limited",
    });
    expect(safeDownloadReturnTo(link.url)).toBe(link.url);
    for (const path of [
      "//evil.test/d/x",
      "https://evil.test/d/x",
      `${link.url}?x=1`,
      `${link.url}/../account`,
    ])
      expect(safeDownloadReturnTo(path)).toBeNull();
  });
  it("claims durable outbox before send and records accepted without confusing delivered", async () => {
    const token = await approvalLink();
    const send = vi.fn(async (_url: unknown, init?: RequestInit) => {
      expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
        status: "unknown",
        attempts: 1,
      });
      expect(String(init!.body)).not.toContain(`/d/${token}`);
      expect(String(init!.body)).toContain("/account?apply=1");
      expect(JSON.parse(String(init!.body))).toMatchObject({
        html: expect.any(String),
        text: expect.any(String),
        attachments: [
          {
            content: expect.any(String),
            content_id: "onecrew-mark",
            content_type: "image/png",
            filename: "onecrew-ai-mark.png",
          },
        ],
      });
      return Response.json({ id: "provider-mail-id" });
    });
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    await flushDistributionMail(bindings);
    expect(send).toHaveBeenCalledTimes(1);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "accepted",
      attempts: 1,
    });
  });
  it("unknown mail retry reuses exact provider idempotency and payload; no retry beyond23h", async () => {
    await approvalLink();
    const send = vi.fn(async () => {
      throw new Error("network timeout with secret must never be logged");
    });
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    const first = send.mock.calls[0] as unknown as [string, RequestInit];
    await db
      .prepare("UPDATE distribution_mail SET last_attempt_at=?")
      .bind(Date.now() - 61000)
      .run();
    const detail = await applicationDetail(db, applicationId);
    const accepted = vi.fn(async () => Response.json({ id: "mail-id" }));
    vi.stubGlobal("fetch", accepted);
    await retryEmail(bindings, {
      applicationId,
      expectedRevision: 2,
      expectedMailEventId: detail.mail!.id,
      requestId: uuid(),
      actorId: "test-admin",
    });
    await flushDistributionMail(bindings);
    const second = accepted.mock.calls[0] as unknown as [string, RequestInit];
    expect(second[1].body).toBe(first[1].body);
    expect(second[1].headers).toEqual(first[1].headers);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "accepted",
      attempts: 2,
    });
  });
  it("marks overdue pending mail failed without sending and preserves unknown outcomes", async () => {
    await approvalLink();
    await db
      .prepare("UPDATE distribution_mail SET created_at=?")
      .bind(Date.now() - 24 * 3600000)
      .run();
    const send = vi.fn();
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).not.toHaveBeenCalled();
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "failed",
      attempts: 0,
    });
    await db.prepare("UPDATE distribution_mail SET status='unknown'").run();
    await flushDistributionMail(bindings);
    const detail = await applicationDetail(db, applicationId);
    expect(detail.mail).toMatchObject({ status: "unknown" });
    await expect(
      retryEmail(bindings, {
        applicationId,
        expectedRevision: 2,
        expectedMailEventId: detail.mail!.id,
        requestId: uuid(),
        actorId: "test-admin",
      }),
    ).rejects.toMatchObject({ code: "invalid_transition" });
  });
  it("never steals an in-flight mail lease during administrator retry", async () => {
    await approvalLink();
    await db
      .prepare(
        "UPDATE distribution_mail SET status='unknown',last_attempt_at=?,lease_until=?",
      )
      .bind(Date.now() - 61000, Date.now() + 60000)
      .run();
    const detail = await applicationDetail(db, applicationId);
    await expect(
      retryEmail(bindings, {
        applicationId,
        expectedRevision: 2,
        expectedMailEventId: detail.mail!.id,
        requestId: uuid(),
        actorId: "test-admin",
      }),
    ).rejects.toMatchObject({ code: "revision_conflict" });
    expect(detail.mail).toMatchObject({ canRetry: false });
    expect((await applicationDetail(db, applicationId)).revision).toBe(2);
  });
  it.each(["revocation", "email-change"])(
    "preserves timed-out mail as unknown after %s and permanently blocks retries",
    async (change) => {
      await approvalLink();
      const send = vi.fn(async () => {
        throw new Error("simulated timeout");
      });
      vi.stubGlobal("fetch", send);
      await flushDistributionMail(bindings);
      await db
        .prepare("UPDATE distribution_mail SET last_attempt_at=?")
        .bind(Date.now() - 61000)
        .run();
      if (change === "revocation")
        await reviewApplication(bindings, review("revoked", 2));
      else
        await db
          .prepare('UPDATE "user" SET email=? WHERE id=?')
          .bind("changed@example.com", user)
          .run();
      await flushDistributionMail(bindings);
      await db
        .prepare("UPDATE distribution_mail SET last_attempt_at=?")
        .bind(Date.now() - 61000)
        .run();
      await flushDistributionMail(bindings);
      expect(send).toHaveBeenCalledTimes(1);
      expect(
        await db
          .prepare(
            "SELECT status,attempts,blocked_reason,lease_until FROM distribution_mail",
          )
          .first(),
      ).toEqual({
        status: "unknown",
        attempts: 1,
        blocked_reason: null,
        lease_until: null,
      });
      const detail = await applicationDetail(db, applicationId);
      await expect(
        retryEmail(bindings, {
          applicationId,
          expectedRevision: detail.revision,
          expectedMailEventId: detail.mail!.id,
          requestId: uuid(),
          actorId: "test-admin",
        }),
      ).rejects.toMatchObject({ code: "invalid_transition" });
    },
  );
  it("does not send a queued link after entitlement revocation", async () => {
    await approvalLink();
    await reviewApplication(bindings, review("revoked", 2));
    const send = vi.fn();
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).not.toHaveBeenCalled();
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "failed",
    });
  });
  it("requires complete declared target pairs and R2 heads before atomic publish; immutable idempotent readback", async () => {
    const input = release(),
      id = uuid();
    expect(
      releaseInput.safeParse({ ...input, assets: input.assets.slice(0, 1) })
        .success,
    ).toBe(false);
    const result = await publishRelease(bindings, input, id);
    expect(result.release.version).toBe("1.2.3");
    expect(await publishRelease(bindings, input, id)).toEqual(result);
    await expect(publishRelease(bindings, input, uuid())).rejects.toMatchObject(
      { code: "version_exists" },
    );
    const bad = {
      ...bindings,
      RELEASES: { head: async () => null } as unknown as R2Bucket,
    };
    await expect(
      publishRelease(bad, release("1.2.4"), uuid()),
    ).rejects.toMatchObject({ code: "asset_not_ready" });
    await expect(readRelease(bindings, "1.2.4")).rejects.toMatchObject({
      code: "not_found",
    });
  });
  it("publishes a Windows x64 installer without a desktop updater", async () => {
    const input = release();
    const windows = {
      ...input.assets[0]!, id: uuid(),
      key: `releases/${input.version}/OneCrew-Setup-${input.version}-windows-x64.exe`,
      filename: `OneCrew-Setup-${input.version}-windows-x64.exe`,
      platform: "windows", arch: "x86_64", kind: "installer",
    };
    expect(releaseInput.safeParse({ ...input, assets: [windows] }).success).toBe(true);
    expect(releaseInput.safeParse({ ...input, assets: [...input.assets, windows] }).success).toBe(true);
    expect(releaseInput.safeParse({ ...input, assets: [{ ...windows, arch: "aarch64" }] }).success).toBe(false);
    expect(releaseInput.safeParse({ ...input, assets: [{ ...windows, kind: "updater", signature: "signed" }] }).success).toBe(false);
    const published = await publishRelease(bindings, { ...input, assets: [windows] }, uuid());
    expect(published.release.assets).toMatchObject([{ platform: "windows", arch: "x86_64", kind: "installer" }]);
  });
  it("streams full, HEAD and single Range; rejects unsafe ranges and changed objects", async () => {
    const input = release();
    await publishRelease(bindings, input, uuid());
    const asset = (await getAsset(bindings, input.assets[0]!.id))!;
    const url = "http://localhost:8787/file";
    const head = await streamAsset(
      bindings,
      new Request(url, { method: "HEAD" }),
      asset,
    );
    expect(head.status).toBe(200);
    expect(await head.text()).toBe("");
    expect(head.headers.get("Content-Length")).toBe("5");
    const range = await streamAsset(
      bindings,
      new Request(url, { headers: { Range: "bytes=1-3" } }),
      asset,
    );
    expect(range.status).toBe(206);
    expect(range.headers.get("Content-Range")).toBe("bytes 1-3/5");
    const invalid = await streamAsset(
      bindings,
      new Request(url, { headers: { Range: "bytes=8-" } }),
      asset,
    );
    expect(invalid.status).toBe(416);
    expect(invalid.headers.get("Content-Range")).toBe("bytes */5");
    const changed = {
      ...bindings,
      RELEASES: {
        head: async () => ({ size: 5, etag: "changed" }),
      } as unknown as R2Bucket,
    };
    await expect(
      streamAsset(changed, new Request(url), asset),
    ).rejects.toMatchObject({ code: "not_found" });
    expect(parseRange("bytes=-2", 5)).toEqual({ offset: 3, length: 2 });
    expect(() => parseRange("bytes=0-1,3-4", 5)).toThrow();
  });
  it("denies unauthenticated Web and cookie-only Desktop; returns authentication errors not204", async () => {
    for (const path of [
      "/api/distribution/application",
      `/api/distribution/download-links/${"a".repeat(43)}`,
    ]) {
      const response = await app.request(
        "http://localhost:8787" + path,
        {},
        bindings,
      );
      expect(response.status).toBe(401);
    }
    const response = await app.request(
      "http://localhost:8787/api/desktop/distribution/update/darwin/aarch64/1.0.0",
      { headers: { Cookie: "session=pretend" } },
      bindings,
    );
    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ error: "device_unauthorized" });
  });
  it("serves exact authenticated browser DTOs, requires verified current user and same-origin mutations", async () => {
    const token = await approvalLink();
    await publishRelease(bindings, release(), uuid());
    vi.spyOn(sessionModule, "loadSafeSession").mockResolvedValue({
      headers: new Headers(),
      session: {
        expiresAt: new Date(Date.now() + 3600000).toISOString(),
        sessionId: "test-session",
        user: {
          id: user,
          email: "distribution@example.com",
          name: "Tester",
          image: null,
        },
      },
    });
    const response = await app.request(
      `http://localhost:8787/api/distribution/download-links/${token}`,
      {},
      bindings,
    );
    expect(response.status).toBe(200);
    const payload = (await response.json()) as {
      assets: { id: string; downloadUrl: string }[];
      expiresAt: number;
      release: { version: string };
    };
    expect(payload.release.version).toBe("1.2.3");
    expect(payload.assets).toHaveLength(1);
    expect(payload.assets[0]!.downloadUrl).toBe(
      `/api/distribution/download-links/${token}/assets/${payload.assets[0]!.id}`,
    );
    expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
    const denied = await app.request(
      "http://localhost:8787/api/distribution/download-links",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": uuid(),
        },
        body: "{}",
      },
      bindings,
    );
    expect(denied.status).toBe(403);
    await db
      .prepare('UPDATE "user" SET emailVerified=0 WHERE id=?')
      .bind(user)
      .run();
    const unverified = await app.request(
      "http://localhost:8787/api/distribution/application",
      {},
      bindings,
    );
    expect(unverified.status).toBe(403);
    expect(await unverified.json()).toEqual({
      error: "email_verification_required",
    });
  });
  it("emits official200/204 update protocol and denies downloads immediately after revoke", async () => {
    await approvalLink();
    const input = release();
    await publishRelease(bindings, input, uuid());
    await setDistributionSettings(bindings, {
      applicationsOpen: false,
      expectedRevision: 1,
      requestId: uuid(),
      actorId: "test-admin",
    });
    const verifier = vi
      .spyOn(identityModule, "verifyDeviceCredential")
      .mockResolvedValue({
        userId: user,
        deviceId: "device",
        pairingId: "pair",
        installationId: "installation",
        credentialId: "credential",
      });
    const update = await app.request(
      "http://localhost:8787/api/desktop/distribution/update/darwin/aarch64/1.0.0",
      {},
      bindings,
    );
    expect(update.status).toBe(200);
    expect(await update.json()).toEqual({
      version: "1.2.3",
      notes: "Notes",
      pub_date: "2026-09-06T00:00:00Z",
      signature: "signed-fixture",
      url: `http://localhost:8787/api/desktop/distribution/assets/${input.assets[1]!.id}`,
    });
    const latest = await app.request(
      "http://localhost:8787/api/desktop/distribution/update/darwin/aarch64/1.2.3",
      {},
      bindings,
    );
    expect(latest.status).toBe(204);
    await reviewApplication(bindings, review("revoked", 2));
    const denied = await app.request(
      `http://localhost:8787/api/desktop/distribution/assets/${input.assets[1]!.id}`,
      {},
      bindings,
    );
    expect(denied.status).toBe(403);
    expect(await denied.json()).toEqual({ error: "entitlement_revoked" });
    expect(verifier).toHaveBeenCalledTimes(3);
  });
  it("requires independent publisher bearer and never admits ADMIN_TOKEN", async () => {
    const pub = {
      ...bindings,
      RELEASE_PUBLISHER_TOKEN:
        "publisher-test-independent-high-entropy-token-value-1234",
    };
    const denied = await app.request(
      "http://localhost:8787/api/distribution/publisher/releases/1.2.3",
      { headers: { Authorization: "Bearer pretend-admin-token" } },
      pub,
    );
    expect(denied.status).toBe(401);
    const request = uuid(),
      input = release();
    const published = await app.request(
      "http://localhost:8787/api/distribution/publisher/releases",
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${pub.RELEASE_PUBLISHER_TOKEN}`,
          "Idempotency-Key": request,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(input),
      },
      pub,
    );
    expect(published.status).toBe(200);
    const read = await app.request(
      "http://localhost:8787/api/distribution/publisher/releases/1.2.3",
      { headers: { Authorization: `Bearer ${pub.RELEASE_PUBLISHER_TOKEN}` } },
      pub,
    );
    expect(await read.json()).toEqual(await published.json());
  });
  it("orders versions semantically for beta and stable updates", () => {
    expect(compareVersions("1.10.0", "1.9.9")).toBe(1);
    expect(compareVersions("2.0.0", "2.0.0-beta.9")).toBe(1);
    expect(compareVersions("2.0.0-beta.10", "2.0.0-beta.9")).toBe(1);
    expect(compareVersions("2.0.0-alpha", "2.0.0-beta")).toBe(-1);
    expect(compareVersions("2.0.0-beta-test.3", "2.0.0-beta-test.2")).toBe(1);
    expect(compareVersions("2.0.0-beta-test.2", "2.0.0-beta-test.3")).toBe(-1);
    expect(compareVersions("2.0.0-beta-test.10", "2.0.0-beta-test.9")).toBe(1);
    expect(compareVersions("2.0.0-beta-test.2", "2.0.0-beta-test.2")).toBe(0);
    expect(
      compareVersions("2.0.0-beta-test.2", "2.0.0-beta-test-extra.1"),
    ).toBe(-1);
  });
});

describe("distribution mail diagnostics", () => {
  it.each([
    [400, "failed", "http_client_error"],
    [401, "failed", "http_client_error"],
    [403, "failed", "http_client_error"],
    [422, "failed", "http_client_error"],
    [429, "unknown", "rate_limited"],
    [500, "unknown", "http_server_error"],
    [503, "unknown", "http_server_error"],
    [302, "failed", "unexpected_http_status"],
  ])("records safe diagnostics for HTTP %s", async (httpStatus, status, code) => {
    await reviewApplication(bindings, review("approved"));
    vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
      // Native workerd Request validates the actual sender options, unlike an init-only mock.
      const request = new Request(url, init);
      expect(request.redirect).toBe("manual");
      return new Response("secret-provider-body", { status: Number(httpStatus), headers: { Location: "https://must-not-follow.example" } });
    }));
    await flushDistributionMail(bindings);
    const detail = await applicationDetail(db, applicationId);
    expect(detail.mail).toMatchObject({ status, lastHttpStatus: httpStatus, lastErrorCode: code, attempts: 1 });
    expect(JSON.stringify(detail.mail)).not.toContain("secret-provider-body");
  });
  it.each([
    ["timeout", () => { throw new DOMException("secret", "TimeoutError"); }],
    ["transport_error", () => { throw new TypeError("secret"); }],
    ["invalid_provider_response", () => new Response("not-json")],
    ["invalid_provider_response", () => Response.json({ id: 42 })],
    ["invalid_provider_response", () => Response.json(null)],
    ["invalid_provider_response", () => Response.json({ id: "" })],
  ] as const)("classifies %s without leaking raw errors", async (code, response) => {
    await reviewApplication(bindings, review("approved"));
    vi.stubGlobal("fetch", vi.fn(async () => response()));
    await flushDistributionMail(bindings);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "unknown", lastErrorCode: code,
      lastHttpStatus: code === "invalid_provider_response" ? 200 : null,
    });
  });
  it("records persistence failure after provider ACK as unknown", async () => {
    await reviewApplication(bindings, review("approved"));
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({ id: "accepted-id" })));
    const wrappedDb = {
      batch: db.batch.bind(db),
      exec: db.exec.bind(db),
      prepare(sql: string) {
        if (sql.includes("SET status='accepted'")) return {
          bind: () => ({ run: async () => { throw new Error("database-write-failed"); } }),
        };
        return db.prepare(sql);
      },
    } as unknown as D1Database;
    await flushDistributionMail({ ...bindings, DB: wrappedDb });
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "unknown", lastHttpStatus: 200, lastErrorCode: "persistence_after_ack", acceptedAt: null,
    });
  });
  it("allows an exact manual retry above five attempts, never auto-retries and deduplicates the request", async () => {
    await reviewApplication(bindings, review("approved"));
    await db.prepare("UPDATE distribution_mail SET status='unknown',attempts=5,blocked_reason='retry_window_exhausted',last_attempt_at=?")
      .bind(Date.now() - 61000).run();
    const original = await db.prepare("SELECT id,payload_json FROM distribution_mail").first<{id: string;payload_json:string}>();
    const send = vi.fn(async (_url: string, init: RequestInit) => {
      expect(init.body).toBe(original!.payload_json);
      expect((init.headers as Record<string,string>)["Idempotency-Key"]).toBe(`distribution/${original!.id}`);
      return new Response(null, { status: 503 });
    });
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).not.toHaveBeenCalled();
    const detail = await applicationDetail(db, applicationId);
    expect(detail.mail).toMatchObject({ canRetry: true, attempts: 5 });
    const request = { applicationId, expectedRevision: detail.revision, expectedMailEventId: original!.id, requestId: uuid(), actorId: "test-admin" };
    await retryEmail(bindings, request);
    expect(send).toHaveBeenCalledTimes(1);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "unknown", attempts: 6, blockedReason: null, lastHttpStatus: 503, canRetry: false,
    });
    await db.prepare("UPDATE distribution_mail SET last_attempt_at=?").bind(Date.now()-61000).run();
    await retryEmail(bindings, request); // replay after a lost response must never send again
    await flushDistributionMail(bindings);
    expect(send).toHaveBeenCalledTimes(1);
    expect(await db.prepare("SELECT count(*) n FROM distribution_mail").first()).toEqual({ n: 1 });
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({ attempts: 6, canRetry: true });
  });
  it("manual dispatch sends only its event, rejects accepted and coalesces concurrent request replay", async () => {
    await reviewApplication(bindings, review("approved"));
    const detail = await applicationDetail(db, applicationId);
    const unrelatedId = uuid();
    await db.prepare("INSERT INTO distribution_mail(id,application_id,kind,decision_revision,recipient,payload_json,status,created_at) SELECT ?,application_id,kind,decision_revision,recipient,payload_json,'pending',created_at-1000 FROM distribution_mail WHERE id=?")
      .bind(unrelatedId, detail.mail!.id).run();
    await db.prepare("UPDATE distribution_mail SET status='unknown',attempts=5,last_attempt_at=? WHERE id=?").bind(Date.now()-61000, detail.mail!.id).run();
    const send = vi.fn(async () => Response.json({ id: "accepted-once" }));
    vi.stubGlobal("fetch", send);
    const request = { applicationId, expectedRevision: detail.revision, expectedMailEventId: detail.mail!.id, requestId: uuid(), actorId: "test-admin" };
    await Promise.all([retryEmail(bindings, request), retryEmail(bindings, request)]);
    expect(send).toHaveBeenCalledTimes(1);
    expect(await db.prepare("SELECT status,attempts FROM distribution_mail WHERE id=?").bind(unrelatedId).first()).toEqual({ status: "pending", attempts: 0 });
    const accepted = await applicationDetail(db, applicationId);
    expect(accepted.mail).toMatchObject({ status: "accepted", attempts: 6, canRetry: false });
    await expect(retryEmail(bindings, { ...request, requestId: uuid(), expectedRevision: accepted.revision })).rejects.toMatchObject({ code: "invalid_transition" });
    expect(send).toHaveBeenCalledTimes(1);
  });
  it("blocks truly expired unknown events without sending or inventing diagnostics", async () => {
    await reviewApplication(bindings, review("approved"));
    await db.prepare("UPDATE distribution_mail SET status='unknown',attempts=5,created_at=?")
      .bind(Date.now() - 24 * 3600000).run();
    const send = vi.fn();
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    const detail = await applicationDetail(db, applicationId);
    expect(detail.mail).toMatchObject({ status: "unknown", blockedReason: "retry_window_exhausted", canRetry: false, lastHttpStatus: null, lastErrorCode: null });
    await expect(retryEmail(bindings, { applicationId, expectedRevision: detail.revision, expectedMailEventId: detail.mail!.id, requestId: uuid(), actorId: "test-admin" })).rejects.toMatchObject({code:"invalid_transition"});
    expect(send).not.toHaveBeenCalled();
  });
  it.each(["unknown", "failed", "pending"])("never automatically retries an attempted %s event", async status => {
    await reviewApplication(bindings, review("approved"));
    await db.prepare("UPDATE distribution_mail SET status=?,attempts=1,last_attempt_at=?").bind(status, Date.now()-61000).run();
    const send = vi.fn();
    vi.stubGlobal("fetch", send);
    await flushDistributionMail(bindings);
    expect(send).not.toHaveBeenCalled();
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({ status, attempts: 1 });
  });
  it("records accepted without claiming delivery and clears prior diagnostics", async () => {
    await reviewApplication(bindings, review("approved"));
    await db.prepare("UPDATE distribution_mail SET last_http_status=503,last_error_code='http_server_error'").run();
    vi.stubGlobal("fetch", vi.fn(async () => Response.json({ id: "accepted-id" })));
    await flushDistributionMail(bindings);
    expect((await applicationDetail(db, applicationId)).mail).toMatchObject({
      status: "accepted", lastHttpStatus: 200, lastErrorCode: null,
    });
  });
});

describe("mail runtime request compatibility", () => {
  it("constructs the configured request in workerd without network I/O", () => {
    const request = new Request("https://api.resend.com/emails", {
      method: "POST", redirect: "manual", signal: AbortSignal.timeout(15000), body: "{}",
    });
    expect(request.redirect).toBe("manual");
  });
});
