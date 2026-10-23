import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerCreatorOperations } from "../src/creators/operations.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("真实账号联系路由确认只提交一次，历史清理通知覆盖邮件与本地触点，replay/noop不重复通知", async () => {
  const scratch = await mkdtemp(fileURLToPath(new URL("../../.scratch/creator-contacts-account-", import.meta.url)));
  const directory = await createPrivateDirectory(join(scratch, "private")), lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts);
  try {
    const principal = { user: { id: "contacts", email: "contacts@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
    const context = accounts.activate(principal);
    await databases.createNewAccount(context);
    const registry = new OperationRegistry(accounts), changes: (readonly string[])[] = [];
    registerCreatorOperations(registry, databases.creatorsResolver(), accounts, (_, scopes) => changes.push(scopes));
    const execute = (name: string, input: unknown) => registry.execute(name, input, principal.user.id, context.epoch);
    const created = await execute("onecrew_creator_manual_create", { input: { display_name: "Contacts" } }) as { creator_id: string };
    changes.length = 0;
    const add = { creator_id: created.creator_id, channel: "email", value: "target@example.com" };
    const route = await execute("onecrew_creator_contact_route_add", { input: add }) as { route_id: string };
    await execute("onecrew_creator_contact_route_add", { input: add });
    await execute("onecrew_creator_contact_route_update", { input: { ...add, contact_route_id: route.route_id } });
    assert.deepEqual(changes, [["creators"]]);
    await execute("onecrew_creator_contact_route_update", { input: { ...add, contact_route_id: route.route_id, source_url: "https://example.com/contact" } });
    assert.deepEqual(changes, [["creators"], ["creators"]]);
    const key = createHash("sha256").update(JSON.stringify([principal.user.id, principal.tenantId, principal.baseId])).digest("hex");
    const db = new DatabaseSync(join(directory, `account-${key}`, "creator.sqlite"));
    try {
      db.exec("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; INSERT INTO mailbox_refs VALUES ('mail','gmail','mail','sender@example.com',NULL,'[]',1,NULL,1,1)");
      db.prepare("INSERT INTO email_threads (id,mailbox_ref_id,creator_id,contact_route_id,provider_thread_key,subject,unread_count,active_folder,created_at,updated_at) VALUES ('thread','mail',?,?,'provider','history',0,'sent',1,1)").run(created.creator_id, route.route_id);
      db.exec("INSERT INTO emails (id,thread_id,provider_message_id,direction,subject,delivery_status,occurred_at,origin) VALUES ('email','thread','remote','outbound','history','sent',3,'provider_import'); INSERT INTO email_recipients VALUES ('email','from','sender@example.com',NULL),('email','to','target@example.com',NULL)");
      db.prepare("INSERT INTO email_drafts (id,mailbox_ref_id,to_address,creator_id,contact_route_id,subject,body_text,state,revision,created_at,updated_at) VALUES ('draft','mail','target@example.com',?,?,'draft','draft','draft',1,1,1)").run(created.creator_id, route.route_id);
      db.prepare("INSERT INTO touchpoints (id,creator_id,contact_route_id,kind,occurred_at) VALUES ('touch',?,?,'historical',1)").run(created.creator_id, route.route_id);
      db.exec("COMMIT");
    } finally { db.close(); }
    const preview = await execute("onecrew_creator_contact_route_delete_prepare", { input: { creator_id: created.creator_id, contact_route_id: route.route_id } }) as { status: string; operation_id: string };
    assert.equal(preview.status, "confirmation_required");
    assert.equal(changes.length, 2);
    assert.deepEqual(await execute("onecrew.creator.identity.resolve", { candidate: { platform: "x", external_id: null, target_kind: "user", handle: null, display_name: "Contacts", profile_url: null } }), { status: "new" });
    const confirm = { input: { operation_id: preview.operation_id } };
    const [first, replay] = await Promise.all([execute("onecrew_creator_contact_route_delete_confirm", confirm), execute("onecrew_creator_contact_route_delete_confirm", confirm)]);
    assert.deepEqual(first, replay);
    assert.deepEqual(changes, [["creators"], ["creators"], ["creators", "email_folders", "email_threads", "email_drafts", "outreach"]]);
    const resolved = await databases.creatorsResolver().resolve(context);
    assert.equal(resolved.repository.get(created.creator_id).contact_routes.length, 0);
    const reader = new DatabaseSync(join(directory, `account-${key}`, "creator.sqlite"), { readOnly: true });
    try { assert.equal(reader.prepare("SELECT COUNT(*) AS n FROM emails").get()!.n, 0); assert.deepEqual(reader.prepare("PRAGMA foreign_key_check").all(), []); }
    finally { reader.close(); }
  } finally { databases.close(); await lease.close(); }
});
