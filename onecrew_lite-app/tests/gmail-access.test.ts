import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { z } from "zod";
import { ACCESS_MAIL_ORIGIN, AccessMailClient, MailAccessError, MailAccessToken, accessMailStatusError, decodeAccessMailEnvelope, gmailConnectionIdSchema, mailConnectionSchema, validateGoogleConfirmationUrl } from "../src/mail/access-mail-client.ts";
import { validateGmailAccessToken } from "../src/mail/gmail-access.ts";
import { gmailAccessCommandInput } from "../src/mail/gmail-access-operations.ts";
import { GMAIL_READ_SCOPE, GMAIL_SEND_SCOPE, GmailMailboxRepository, asciiLower, gmailMailboxId, gmailProjection } from "../src/mail/gmail-mailbox-repository.ts";
import type { MailConnection } from "../src/mail/access-mail-client.ts";

const conn: MailConnection = { id: "conn_01234567-89ab-cdef-0123-456789abcdef", provider: "google", email: "Sender@Example.com", status: "connected", scopes: [GMAIL_SEND_SCOPE, GMAIL_READ_SCOPE], capabilities: { can_send: true, can_read_thread: true, can_read_attachment: false }, created_at: "2026-10-15T00:00:00Z", updated_at: "2026-10-15T00:00:00Z" };
const user = "access-user";

test("Access v1解析严格协议、版本、连接标识、大小及UTF-8，错误不携带远端正文", async () => {
  const envelope = (data: unknown) => Buffer.from(JSON.stringify({ protocol: "onecrew.mail", version: 1, data }));
  const value = await decodeAccessMailEnvelope(envelope({ connections: [conn] }), z.object({ connections: z.array(mailConnectionSchema).max(100) }));
  assert.equal(value.connections[0]?.id, conn.id);
  for (const bytes of [Buffer.alloc(256 * 1024 + 1), Buffer.from([0xff]), Buffer.from('{"protocol":"wrong","version":1,"data":{}}'), Buffer.from('{"protocol":"onecrew.mail","version":2,"data":{}}')]) await assert.rejects(decodeAccessMailEnvelope(bytes, z.unknown()), error => error instanceof MailAccessError && error.code === "mail_access_response_invalid");
  assert.equal(gmailConnectionIdSchema.safeParse(conn.id).success, true);
  assert.equal(gmailConnectionIdSchema.safeParse("conn_0123456789abcdef").success, false);
  assert.equal(accessMailStatusError(200), null);
  assert.equal(accessMailStatusError(401)?.code, "mail_access_login_required");
  assert.equal(accessMailStatusError(403)?.code, "mail_access_denied");
  assert.equal(accessMailStatusError(302)?.code, "mail_access_request_rejected");
  await assert.rejects(new AccessMailClient().accessToken("unused", "invalid"), /mail_access_configuration_invalid/);
});

test("Google确认URL必须同源且只有唯一provider与有界ticket", () => {
  const valid = `${ACCESS_MAIL_ORIGIN}/integrations/mail/connect?provider=google&ticket=one-use`;
  assert.equal(validateGoogleConfirmationUrl(valid), valid);
  for (const invalid of [valid.replace("access.onecrewai.io", "other.example.com"), valid + "#fragment", valid + "&ticket=duplicate", valid.replace("provider=google", "provider=other"), valid.replace("ticket=one-use", "ticket="), valid.replace("ticket=one-use", `ticket=${"中".repeat(683)}`), valid.replace("https://", "https://user:password@")]) assert.throws(() => validateGoogleConfirmationUrl(invalid), /mail_access_response_invalid/);
  assert.equal(gmailAccessCommandInput.safeParse({}).success, true);
  assert.equal(gmailAccessCommandInput.safeParse({ accountId: "other", bearer: "untrusted" }).success, false);
});

test("邮箱标识保持SHA256前16字节，不改UUID版本位；邮箱只做ASCII大小写处理", () => {
  const hex = createHash("sha256").update(`onecrew-gmail\0${user}\0${conn.id}`).digest("hex").slice(0, 32);
  assert.equal(gmailMailboxId(user, conn.id).replaceAll("-", ""), hex);
  const projection = gmailProjection(user, conn);
  assert.equal(projection.email, "sender@example.com");
  assert.equal(projection.authConnectionId, `gmail:${user}:${conn.id}`);
  assert.equal(projection.secretRecordId, `access:${user}:${conn.id}`);
  assert.equal(asciiLower("ÄA@EXAMPLE.COM"), "Äa@example.com");
  assert.equal(gmailProjection(user, { ...conn, scopes: [GMAIL_SEND_SCOPE] }).available, false);
});

test("token必须scope齐全且超过30秒；私密值禁止JSON及inspect泄漏", () => {
  const now = Date.parse("2026-10-15T00:00:00Z") / 1000;
  const token = new MailAccessToken("private-token", "2026-10-15T00:00:31Z", conn.scopes, "SENDER@example.com");
  assert.equal(validateGmailAccessToken(token, "sender@example.com", now), now + 31);
  assert.throws(() => validateGmailAccessToken(token, "other@example.com", now), /authorization_required/);
  assert.throws(() => validateGmailAccessToken(new MailAccessToken("secret", "2026-10-15T00:00:30Z", conn.scopes, conn.email), conn.email, now), /authorization_required/);
  assert.throws(() => validateGmailAccessToken(new MailAccessToken("secret", "not-a-date", conn.scopes, conn.email), conn.email, now), /token_invalid/);
  assert.throws(() => validateGmailAccessToken(new MailAccessToken("secret", "2026-10-15T00:00:31Z", [GMAIL_SEND_SCOPE], conn.email), conn.email, now), /authorization_required/);
  assert.throws(() => JSON.stringify(token), /must not be serialized/);
  assert.equal(inspect(token).includes("private-token"), false);
});

test("真实原SQL邮箱映射：授权刷新、历史保留、scope撤销及冲突事务回滚", () => {
  const root = new URL("../../", import.meta.url);
  const scratch = new URL(".scratch/gmail-access/", root);
  mkdirSync(scratch, { recursive: true });
  const path = resolve(mkdtempSync(resolve(fileURLToPath(scratch), "case-")), "creator.sqlite3");
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA user_version=90");
    for (const name of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${name}`, import.meta.url), "utf8"));
    const repository = new GmailMailboxRepository(db);
    repository.reconcile(user, [conn]);
    const id = gmailMailboxId(user, conn.id);
    assert.equal(repository.binding(id).available, true);
    assert.equal(repository.binding(id).authAvailable, true);
    assert.equal(repository.binding(id).providerKind, "remote");
    const before = db.prepare("SELECT created_at FROM mailbox_refs WHERE id=?").get(id)!.created_at;
    repository.reconcile(user, []);
    assert.equal(repository.binding(id).available, false);
    assert.equal(db.prepare("SELECT count(*) AS count FROM mailbox_refs").get()!.count, 1);
    repository.reconcile(user, [{ ...conn, scopes: [GMAIL_SEND_SCOPE] }]);
    assert.equal(repository.binding(id).authAvailable, false);
    repository.reconcile(user, [conn]);
    assert.equal(db.prepare("SELECT created_at FROM mailbox_refs WHERE id=?").get(id)!.created_at, before);
    db.prepare("UPDATE email_auth_connections SET secret_record_id='different-secret' WHERE id=?").run(`gmail:${user}:${conn.id}`);
    assert.throws(() => repository.reconcile(user, [conn]), /another secret/);
    assert.equal(repository.binding(id).available, true);
  } finally { db.close(); }
});
