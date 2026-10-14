import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const bindings = env as unknown as CloudflareBindings & { TEST_MIGRATIONS: D1Migration[] };

const retiredTkOneTables = [
  "oauth_authorization_transaction",
  "integration_revocation",
  "desktop_integration_authorization",
  "integration_read_request",
  "third_party_connection",
] as const;

async function seedRetirementOwner(db: D1Database) {
  await db.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES (?,?,?,?,?,?)')
    .bind("tkone-retirement-user", "Retirement", "retirement@example.test", 1, "2026-01-01", "2026-01-01").run();
  await db.prepare('INSERT INTO account(id,issuer,accountId,providerId,userId,createdAt,updatedAt) VALUES (?,?,?,?,?,?,?)')
    .bind("retained-account", "https://accounts.google.com", "google-login", "google", "tkone-retirement-user", "2026-01-01", "2026-01-01").run();
  await db.prepare("INSERT INTO access_device(id,user_id,label,created_at) VALUES ('dev_0123456789abcdefghijklmn','tkone-retirement-user','Retained device',1000)").run();
  await db.prepare("INSERT INTO mail_connection(id,user_id,provider,provider_subject,email,token_ciphertext,token_nonce,token_key_version,scope_json,status,created_at,updated_at) VALUES ('conn_retained','tkone-retirement-user','google','mail-subject','other@example.test','cipher','nonce',1,'[]','connected',1000,1000)").run();
  await db.prepare("INSERT INTO mail_oauth_transaction(state_hash,provider,user_id,pkce_ciphertext,pkce_nonce,pkce_key_version,nonce,created_at,expires_at) VALUES ('mail-state','google','tkone-retirement-user','cipher','nonce',1,'nonce',1000,999999999)").run();
}

describe("TkOne retirement migration", () => {
  it("drops retired tables, preserves other domains, and tolerates an already dropped schema", async () => {
    const db = bindings.DB;
    await applyD1Migrations(db, bindings.TEST_MIGRATIONS.slice(0, 14));
    await seedRetirementOwner(db);
    await db.prepare("INSERT INTO third_party_connection(id,user_id,provider,token_ciphertext,token_nonce,token_key_version,scope_json,status,created_at,updated_at) VALUES ('old-connection','tkone-retirement-user','tiktok-business','cipher','nonce',1,'[]','connected',1000,1000)").run();
    await db.prepare("INSERT INTO desktop_integration_authorization(id,provider,user_id,device_id,pairing_id,installation_id,credential_id,idempotency_key,request_hash,locale,status,connection_id,created_at,updated_at,expires_at,retain_until) VALUES ('old-authorization','tiktok-business','tkone-retirement-user','dev_0123456789abcdefghijklmn','pair','installation','credential','request','hash','en-US','connected','old-connection',1000,1000,999999999,999999999)").run();
    await db.prepare("INSERT INTO oauth_authorization_transaction(state_hash,provider,user_id,session_id,locale,created_at,expires_at,desktop_authorization_id) VALUES ('old-state','tiktok-business','tkone-retirement-user','session','en-US',1000,999999999,'old-authorization')").run();
    await db.prepare("INSERT INTO integration_revocation(id,user_id,device_id,provider,connection_id,idempotency_key,status,created_at,updated_at,retain_until) VALUES ('old-revocation','tkone-retirement-user','dev_0123456789abcdefghijklmn','tiktok-business','old-connection','request','revoked',1000,1000,999999999)").run();
    await db.prepare("INSERT INTO integration_read_request(user_id,device_id,provider,request_id,status,created_at,updated_at,expires_at) VALUES ('tkone-retirement-user','dev_0123456789abcdefghijklmn','tiktok-business','request','succeeded',1000,1000,999999999)").run();

    await applyD1Migrations(db, bindings.TEST_MIGRATIONS.slice(14));
    // Execute the SQL again: a second migration-runner call only checks its journal.
    for (const query of bindings.TEST_MIGRATIONS[14]!.queries) await db.prepare(query).run();
    await applyD1Migrations(db, bindings.TEST_MIGRATIONS.slice(14));
    const tables = await db.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").all<{name:string}>();
    for (const table of retiredTkOneTables) expect(tables.results.map(row => row.name)).not.toContain(table);
    expect((await db.prepare("PRAGMA foreign_key_check").all()).results).toEqual([]);
    const indexes = await db.prepare("SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'").all<{name:string}>();
    expect(indexes.results.some(row => /^(oauth_transaction_|third_party_connection_|desktop_authorization_|integration_revocation_|integration_read_)/.test(row.name))).toBe(false);
    expect(await db.prepare("SELECT email FROM user WHERE id='tkone-retirement-user'").first()).toEqual({email:"retirement@example.test"});
    expect(await db.prepare("SELECT providerId FROM account WHERE id='retained-account'").first()).toEqual({providerId:"google"});
    expect(await db.prepare("SELECT label FROM access_device WHERE id='dev_0123456789abcdefghijklmn'").first()).toEqual({label:"Retained device"});
    expect(await db.prepare("SELECT email FROM mail_connection WHERE id='conn_retained'").first()).toEqual({email:"other@example.test"});
    expect(await db.prepare("SELECT state_hash FROM mail_oauth_transaction WHERE state_hash='mail-state'").first()).toEqual({state_hash:"mail-state"});
  });
});
