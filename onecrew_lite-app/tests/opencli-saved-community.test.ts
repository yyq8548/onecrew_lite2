import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { readSavedTelegramCommunity } from "../src/platforms/candidates/saved-telegram-community.ts";

test("schema90保存Telegram来源只读取当前Creator、账号和有效社区解析", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/opencli-community-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await initializeNewCreatorDatabase(path); const db = new DatabaseSync(path);
  const creator = randomUUID(), route = randomUUID(), account = randomUUID(), locator = randomUUID(), target = randomUUID();
  try {
    db.exec("PRAGMA foreign_keys=ON");
    db.prepare("INSERT INTO creators(id,display_name,created_at,updated_at) VALUES (?,'社区来源',1,1)").run(creator);
    db.prepare("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES (?,'telegram','profile','操作账号',1,1)").run(account);
    db.prepare("INSERT INTO contact_locators VALUES (?,'telegram','handle','@community','@community',1,1)").run(locator);
    db.prepare("INSERT INTO creator_contact_routes VALUES (?,?,?,'exact',1,1)").run(route, creator, locator);
    db.prepare("INSERT INTO resolved_targets VALUES (?,?,'telegram','supergroup','12345',NULL,'{}',1,1)").run(target, account);
    db.prepare("INSERT INTO locator_resolutions VALUES ('resolution',?,?,?,'active',1,NULL)").run(locator, account, target);
    const result = readSavedTelegramCommunity(db, creator, route, account);
    assert.equal(result.externalId, "12345"); assert.equal(result.kind, "supergroup");
    assert.equal(result.source.kind, "creator_contact_route");
    assert.throws(() => readSavedTelegramCommunity(db, randomUUID(), route, account));
    assert.throws(() => readSavedTelegramCommunity(db, creator, route, randomUUID()));
    db.prepare("UPDATE locator_resolutions SET state='invalidated'").run();
    assert.throws(() => readSavedTelegramCommunity(db, creator, route, account));
    db.prepare("UPDATE locator_resolutions SET state='active'").run();
    db.prepare("UPDATE creators SET archived_at=2").run();
    assert.throws(() => readSavedTelegramCommunity(db, creator, route, account));
  } finally { db.close(); }
});
