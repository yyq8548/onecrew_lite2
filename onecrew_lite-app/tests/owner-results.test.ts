import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { McpSessions } from "../src/service/mcp-sessions.ts";
import { OwnerResults, ownerOperationRevisions } from "../src/application/owner-results.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { wireResultSnapshot } from "@onecrew/runtime-contracts/result-wire";
import { nativePlatformSearchCatalog } from "../src/discovery/native-search-contracts.ts";

test("MCP account session prepares and confirms actual Creator SQLite once with strict result wire readback", { timeout: 60_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/owner-result-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "owner-")), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const databases = new AccountDatabases(lease, accounts), sessions = new McpSessions(accounts), session = sessions.create(account, "full"), other = sessions.create(account, "full");
  let changes = 0;
  const results = new OwnerResults(accounts, sessions, databases.creatorsResolver(), undefined, current => { assert.equal(current, account); changes++; });
  const call = { call_id: "batch", operation_id: "onecrew.creator.batch.create" as const, schema_revision: ownerOperationRevisions["onecrew.creator.batch.create"], authority: { kind: "direct" }, arguments: { idempotency_key: "actual-reviewed-batch", validation: "strict", items: [{ display_name: "Actual reviewed Creator" }] } };
  try {
    await databases.createNewAccount(account);
    const repository = (await databases.creatorsResolver().resolve(account)).repository;
    await assert.rejects(results.prepare(session.session_id, { ...call, schema_revision: "client-selected" }), /schema_revision_mismatch/);
    const prepared = await results.prepare(session.session_id, call);
    assert.equal(repository.list({}).items.length, 0);
    assert.throws(() => results.present(other.session_id, prepared.result_ref), /origin_mismatch/);
    const shown = results.present(session.session_id, prepared.result_ref, "zh-CN");
    wireResultSnapshot.parse(shown);
    await assert.rejects(results.state(other.session_id, { action: "confirm", binding: shown.binding }), /origin_mismatch/);
    const completed = await results.state(session.session_id, { action: "confirm", binding: shown.binding });
    if (!("snapshot" in completed)) throw new Error("Expected actual completed result");
    wireResultSnapshot.parse(completed.snapshot);
    assert.equal(completed.snapshot.state.kind, "resolved");
    assert.equal(repository.list({}).items.length, 1);
    assert.equal(changes, 1);
    await assert.rejects(results.state(session.session_id, { action: "confirm", binding: completed.snapshot.binding }), /consumed/);
    const replay = await results.prepare(session.session_id, { ...call, call_id: "replayed" }), replayShown = results.present(session.session_id, replay.result_ref);
    await results.state(session.session_id, { action: "confirm", binding: replayShown.binding });
    assert.equal(repository.list({}).items.length, 1);
    assert.equal(changes, 1);
    const continuation = await results.state(session.session_id, { action: "claim_continuation", input: { binding: completed.snapshot.binding, expected_continuation_revision: completed.snapshot.continuation.continuation_revision, mode: "pending" } });
    if (!("claimed" in continuation)) throw new Error("Expected actual continuation");
    const delivered = await results.state(session.session_id, { action: "complete_continuation", input: { binding: completed.snapshot.binding, expected_continuation_revision: continuation.claimed.continuation_revision, lease_id: continuation.claimed.lease_id, outcome: "delivered" } });
    assert.ok("completed" in delivered);
    const mail = sessions.create(account, "mail");
    await assert.rejects(results.prepare(mail.session_id, call), /surface_denied/);
    accounts.logout();
    await assert.rejects(results.state(session.session_id, { action: "read_current", binding: completed.snapshot.binding }), /session_invalid/);
  } finally { results.close(); sessions.close(); databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});

test("trusted session SearchPlan and Choice outputs satisfy the real browser wire schema", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/owner-result-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "without-creator-db-")), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" }), sessions = new McpSessions(accounts), session = sessions.create(account, "full");
  const databases = new AccountDatabases(lease, accounts), results = new OwnerResults(accounts, sessions, databases.creatorsResolver());
  try {
    const question = await results.prepare(session.session_id, { call_id: "choice", operation_id: "onecrew.ask_user_question", schema_revision: ownerOperationRevisions["onecrew.ask_user_question"], authority: { kind: "direct" }, arguments: { interaction_id: "preference", title: "Choice", questions: [{ id: "q", header: "Preference", prompt: "Choose one", options: [{ id: "a", label: "First" }, { id: "b", label: "Second" }] }] } });
    const shown = results.present(session.session_id, question.result_ref);
    wireResultSnapshot.parse(shown);
    const selected = await results.state(session.session_id, { action: "select", binding: shown.binding, answers: [{ question_id: "q", option_id: "b" }] });
    if (!("snapshot" in selected)) throw new Error("Expected Choice snapshot");
    wireResultSnapshot.parse(selected.snapshot);
    const search = nativePlatformSearchCatalog().find(value => value.operation_id === "youtube.channels.search")!;
    const plan = await results.prepare(session.session_id, { call_id: "plan", operation_id: "onecrew.search.plan.prepare", schema_revision: ownerOperationRevisions["onecrew.search.plan.prepare"], authority: { kind: "direct" }, arguments: { description: "Keyboard reviewers", keywords: ["keyboards"], scopes: [{ operation_id: search.operation_id, schema_revision: search.schema_revision, connection: { kind: "service_credential", toolkit: "youtube", credential_id: "selected-key" }, arguments: { query: "keyboards" } }] } });
    const planShown = results.present(session.session_id, plan.result_ref);
    wireResultSnapshot.parse(planShown);
    const confirmed = await results.state(session.session_id, { action: "confirm", binding: planShown.binding });
    if (!("snapshot" in confirmed)) throw new Error("Expected confirmed SearchPlan snapshot");
    wireResultSnapshot.parse(confirmed.snapshot);
    assert.equal(confirmed.snapshot.state.kind, "resolved");
  } finally { results.close(); sessions.close(); databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});
