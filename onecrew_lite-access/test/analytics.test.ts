import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import app from "../src/index";
import { claimSuccessEvents as claimForContext, recordSuccessEvent } from "../src/analytics/service";
import { submitApplication as submitWithContext } from "../src/distribution/service";

const bindings = env as unknown as CloudflareBindings & { TEST_MIGRATIONS: D1Migration[] };
const browserContext = "a".repeat(64);
const claimSuccessEvents = (db: D1Database, userId: string) => claimForContext(db, userId, browserContext);
const submitApplication = (env: CloudflareBindings, userId: string, input: unknown, key: string) => submitWithContext(env, userId, input, key, "zh-CN", browserContext);
const request = (path: string, body: unknown, cookie = "", overrides: Partial<CloudflareBindings> = {}) => app.request(
  new URL(path, bindings.WEB_ORIGIN), {
    method: "POST", headers: { Origin: bindings.WEB_ORIGIN, "Content-Type": "application/json", Cookie: `${cookie}; __Host-onecrew-ga-context=${browserContext}` },
    body: JSON.stringify(body),
  }, { ...bindings, ...overrides },
);
const input = { displayName: "Private name", purpose: "Private application reason", company: "Private company" };

beforeEach(async () => {
  await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS);
  await bindings.DB.prepare('DELETE FROM "rateLimit"').run();
});
afterEach(() => vi.restoreAllMocks());

async function newSession(overrides: Partial<CloudflareBindings> = {}) {
  const response = await request("/api/session/register", { email: `${crypto.randomUUID()}@example.com`, name: "Test", password: "correct-horse-battery-staple" }, "", overrides);
  expect(response.status).toBe(200);
  const body = await response.json<{ user: { id: string } }>();
  const cookie = response.headers.get("Set-Cookie")!.split(";", 1)[0]!;
  await bindings.DB.prepare('UPDATE "user" SET "emailVerified"=1 WHERE id=?').bind(body.user.id).run();
  return { id: body.user.id, cookie };
}

describe("server-confirmed GA4 success receipts", () => {
  it("does not assign receipts to another browser/session, old sessions, or previously unbound retries", async () => {
    const user = await newSession();
    const other = "b".repeat(64);
    expect(await claimForContext(bindings.DB, user.id, other)).toEqual([]);
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual(["sign_up"]);
    await recordSuccessEvent(bindings.DB, user.id, "generate_lead", "expired", browserContext);
    await bindings.DB.prepare("UPDATE analytics_success_event SET created_at=? WHERE receipt_key=?").bind(Date.now() - 31 * 60 * 1000, "generate_lead:expired").run();
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
    await recordSuccessEvent(bindings.DB, user.id, "generate_lead", "unbound");
    await recordSuccessEvent(bindings.DB, user.id, "generate_lead", "unbound", other);
    expect(await claimForContext(bindings.DB, user.id, other)).toEqual([]);
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
  });
  it("records only actual account creation, not duplicate-email synthetic HTTP 200", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response('{"id":"test-mail"}', { status: 200, headers: { "Content-Type": "application/json" } }));
    const overrides = { RESEND_API_KEY: "re_test_analytics", RESEND_FROM_EMAIL: "OneCrew <test@example.com>" };
    const email = `${crypto.randomUUID()}@example.com`;
    const body = { email, name: "Private signup name", password: "correct-horse-battery-staple" };
    const first = await request("/api/session/register", body, "", overrides);
    expect(first.status).toBe(200);
    const duplicate = await request("/api/session/register", body, "", overrides);
    expect(duplicate.status).toBe(200);
    expect(await duplicate.json()).toMatchObject({ ok: true });
    const user = await bindings.DB.prepare('SELECT id FROM "user" WHERE email=?').bind(email).first<{ id: string }>();
    const receipts = await bindings.DB.prepare("SELECT event_name FROM analytics_success_event WHERE user_id=?").bind(user!.id).all();
    expect(receipts.results).toEqual([{ event_name: "sign_up" }]);
    expect((await request("/api/analytics/claim", {})).status).toBe(401);
    const invalid = await request("/api/session/register", { ...body, email: "invalid", password: "short" }, "", overrides);
    expect(invalid.status).toBe(400);
    expect((await bindings.DB.prepare("SELECT count(*) AS n FROM analytics_success_event WHERE user_id=?").bind(user!.id).first<{ n: number }>())?.n).toBe(1);
  });

  it("authenticates claims, emits no personal fields, and survives repeated or concurrent consumers", async () => {
    const user = await newSession();
    await bindings.DB.prepare('UPDATE "user" SET "emailVerified"=0 WHERE id=?').bind(user.id).run();
    expect(await (await request("/api/analytics/claim", {}, user.cookie)).json()).toEqual({ events: [] });
    const identity = await bindings.DB.prepare('SELECT email FROM "user" WHERE id=?').bind(user.id).first<{ email: string }>();
    const login = await request("/api/session/login", { email: identity!.email, password: "correct-horse-battery-staple" });
    expect(login.status).toBe(200);
    expect(await (await request("/api/analytics/claim", {}, user.cookie)).json()).toEqual({ events: [] });
    await bindings.DB.prepare('UPDATE "user" SET "emailVerified"=1 WHERE id=?').bind(user.id).run();
    const responses = await Promise.all([request("/api/analytics/claim", {}, user.cookie), request("/api/analytics/claim", {}, user.cookie)]);
    const bodies = await Promise.all(responses.map(response => response.json<{ events: string[] }>()));
    expect(bodies.flatMap(body => body.events)).toEqual(["sign_up"]);
    expect(await (await request("/api/analytics/claim", {}, user.cookie)).json()).toEqual({ events: [] });
    expect(JSON.stringify(bodies)).not.toMatch(/email|user_id|receipt_key|token|Private/);
    expect((await request("/api/analytics/claim", { events: ["generate_lead"] }, user.cookie)).status).toBe(400);
    const other = await newSession();
    expect(await claimSuccessEvents(bindings.DB, other.id)).toEqual(["sign_up"]);
  });

  it("counts one saved submission per idempotency key, including retries and legitimate reapplication", async () => {
    const user = await newSession();
    await claimSuccessEvents(bindings.DB, user.id);
    await bindings.DB.prepare("UPDATE distribution_settings SET applications_open=1 WHERE id=1").run();
    const key = crypto.randomUUID();
    await submitApplication(bindings, user.id, input, key);
    await submitApplication(bindings, user.id, input, key);
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual(["generate_lead"]);
    await submitApplication(bindings, user.id, input, key);
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
    await expect(submitApplication(bindings, user.id, input, crypto.randomUUID())).rejects.toThrow("application_exists");
    await expect(submitApplication(bindings, user.id, { ...input, purpose: "different" }, key)).rejects.toThrow("idempotency_conflict");
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
    await bindings.DB.prepare("UPDATE distribution_application SET status='rejected' WHERE user_id=?").bind(user.id).run();
    await submitApplication(bindings, user.id, input, crypto.randomUUID());
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual(["generate_lead"]);
  });

  it("does not record rejected or invalid submissions", async () => {
    const user = await newSession();
    await claimSuccessEvents(bindings.DB, user.id);
    await bindings.DB.prepare("UPDATE distribution_settings SET applications_open=0 WHERE id=1").run();
    await expect(submitApplication(bindings, user.id, input, crypto.randomUUID())).rejects.toThrow("applications_closed");
    await expect(submitApplication(bindings, user.id, { ...input, purpose: "" }, crypto.randomUUID())).rejects.toThrow();
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
  });

  it("keeps registration and application success independent of analytics storage failure", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const broken = new Proxy(bindings.DB, {
      get(target, property) {
        if (property === "prepare") return (sql: string) => {
          if (sql.includes("analytics_success_event")) throw new Error("telemetry database unavailable");
          return target.prepare(sql);
        };
        const value = Reflect.get(target, property);
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    const user = await newSession({ DB: broken });
    await bindings.DB.prepare("UPDATE distribution_settings SET applications_open=1 WHERE id=1").run();
    const key = crypto.randomUUID();
    const result = await submitApplication({ ...bindings, DB: broken }, user.id, input, key);
    expect(result.application?.status).toBe("pending");
    await submitWithContext(bindings, user.id, input, key, "zh-CN", "b".repeat(64));
    expect(await claimForContext(bindings.DB, user.id, "b".repeat(64))).toEqual([]);
    expect(await claimSuccessEvents(bindings.DB, user.id)).toEqual([]);
    await expect(recordSuccessEvent(broken, user.id, "sign_up", user.id)).resolves.toBeUndefined();
    expect(await (await request("/api/analytics/claim", {}, user.cookie, { DB: broken })).json()).toEqual({ events: [] });
  });
});
