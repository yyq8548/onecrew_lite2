import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerCreatorOperations } from "../src/creators/operations.ts";
import { CreatorRepository } from "../src/creators/repository.ts";
import { encodeWorkbook } from "../src/creators/workbook.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { listenLocalServer } from "../src/service/http.ts";

test("real HTTP binary workbook upload prepares an import, requires confirmation and rejects stale ownership", async () => {
  const db = new DatabaseSync(":memory:");
  for (const name of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${name}`, import.meta.url), "utf8"));
  db.exec("PRAGMA user_version=90");
  const repository = new CreatorRepository(db);
  const accounts = new AccountContextService();
  const owner = { user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const context = accounts.activate(owner);
  const operations = new OperationRegistry(accounts);
  registerCreatorOperations(operations, { async resolve() { return { owner: { accountId: "account", tenantId: "tenant", baseId: "base" }, repository }; } }, accounts);
  repository.create({ display_name: "Workbook author" });
  const workbook = await encodeWorkbook(repository.exportSnapshot().bundle);
  const token = randomBytes(32).toString("hex");
  const server = await listenLocalServer({ port: 0, instanceId: randomUUID(), controlToken: token, operations });
  const headers = { authorization: `Bearer ${token}`, "x-onecrew-account-id": "account", "x-onecrew-auth-epoch": context.epoch, "x-onecrew-file-name": "%E4%BD%9C%E8%80%85+export.xlsx", "content-type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" };
  try {
    const upload = () => fetch(`${server.origin}/api/creator-import`, { method: "POST", headers, body: new Uint8Array(workbook) });
    const response = await upload();
    assert.equal(response.status, 200);
    const body = await response.json();
    assert.equal(body.ok, true);
    assert.equal(body.result.status, "confirmation_required");
    assert.equal(body.result.file_name, "作者 export.xlsx");
    assert.equal(db.prepare("SELECT count(*) AS n FROM creator_import_receipts").get()!.n, 0);
    const missing = await fetch(`${server.origin}/api/creator-import`, { method: "POST", headers: { authorization: `Bearer ${token}` }, body: new Uint8Array(workbook) });
    assert.deepEqual(await missing.json(), { ok: false, error: "missing_workbook_file_name" });
    accounts.logout();
    assert.equal((await upload()).status, 409);
  } finally {
    await server.close();
    db.close();
  }
});
