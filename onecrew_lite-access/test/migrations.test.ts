import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { describe, expect, it } from "vitest";

type TestBindings = CloudflareBindings & { TEST_MIGRATIONS: D1Migration[] };
const bindings = env as unknown as TestBindings;

describe("append-only desktop pairing migration", () => {
  it("upgrades an existing 0002 database without rewriting legacy rows", async () => {
    await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS.slice(0, 2));
    await bindings.DB.batch([
      bindings.DB.prepare(
        `insert into "user"
           ("id", "name", "email", "emailVerified", "createdAt", "updatedAt")
         values ('user-migration', 'Migration', 'migration@example.com', 1, ?, ?)`,
      ).bind(new Date(0).toISOString(), new Date(0).toISOString()),
      bindings.DB.prepare(
        `insert into "access_device" ("id", "user_id", "label", "created_at")
         values ('dev_000000000000000000000000', 'user-migration', 'Legacy', 1)`,
      ),
      bindings.DB.prepare(
        `insert into "device_activation"
           ("id", "device_id", "key_verifier", "created_at", "expires_at")
         values ('act_000000000000000000000000', 'dev_000000000000000000000000', ?, 1, 2)`,
      ).bind("A".repeat(43)),
    ]);

    await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS.slice(2));
    expect(
      await bindings.DB.prepare(
        'select count(*) as "count" from "device_activation"',
      ).first<{ count: number }>(),
    ).toEqual({ count: 1 });
    const tables = await bindings.DB.prepare(
      `select "name" from sqlite_master
       where "type" = 'table' and "name" like 'desktop_pairing_%'
       order by "name"`,
    ).all<{ name: string }>();
    expect(tables.results).toEqual([
      { name: "desktop_pairing_grant" },
      { name: "desktop_pairing_rate_limit" },
    ]);
  });

  it("applies the current migrations on a fresh database with no raw secret columns", async () => {
    await applyD1Migrations(bindings.DB, bindings.TEST_MIGRATIONS);
    const migrations = await bindings.DB.prepare(
      'select "name" from "d1_migrations" order by "id"',
    ).all<{ name: string }>();
    expect(migrations.results.map((row) => row.name)).toEqual([
      "0001_better_auth.sql",
      "0002_activation_devices.sql",
      "0003_desktop_pairing_grants.sql",
      "0004_registration_ip.sql",
      "0005_device_pairing_ip.sql",
      "0006_login_risk_session_idx.sql",
      "0007_third_party_oauth.sql",
      "0008_desktop_integrations.sql",
      "0009_desktop_distribution.sql",
      "0010_cli_distribution.sql",
      "0011_mail_diagnostics.sql",
      "0012_analytics_success_events.sql",
      "0013_analytics_attribution_context.sql",
      "0014_mail_oauth.sql",
      "0015_retire_tkone_integrations.sql",
    ]);
    expect(await bindings.DB.prepare('SELECT applications_open,revision FROM distribution_settings WHERE id=1').first()).toEqual({applications_open:0,revision:1});
    const columns = await bindings.DB.prepare(
      'pragma table_info("desktop_pairing_grant")',
    ).all<{ name: string }>();
    const names = columns.results.map((row) => row.name);
    for (const forbidden of [
      "approval_token",
      "callback_url",
      "code",
      "code_verifier",
      "ip",
      "session_token",
      "state",
    ]) {
      expect(names).not.toContain(forbidden);
    }
  });
});
