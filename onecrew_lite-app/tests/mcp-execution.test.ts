import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { McpSessions } from "../src/service/mcp-sessions.ts";
import { OwnerResults } from "../src/application/owner-results.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerCreatorOperations } from "../src/creators/operations.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { createMcpCoordinator } from "../src/service/mcp-coordinator.ts";
import { wireResultSnapshot } from "@onecrew/runtime-contracts/result-wire";
import { mcpCompletedSections, mcpExactFields, mcpOutputResourceRefs } from "../src/service/mcp-projection.ts";
import { loadOriginalMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { nativePlatformSearchCatalog } from "../src/discovery/native-search-contracts.ts";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { createMcpAdapter } from "../src/service/mcp-adapter.ts";
import { startLocalRuntime } from "../src/service/runtime.ts";
import { configureMcpCoordinatorRoutes } from "../src/service/mcp-coordinator.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";

async function fixture() {
  const scratch = fileURLToPath(new URL("../../.scratch/mcp-execution-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "actual-")), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const databases = new AccountDatabases(lease, accounts), sessions = new McpSessions(accounts), session = sessions.create(account, "full"), operations = new OperationRegistry(accounts);
  const results = new OwnerResults(accounts, sessions, databases.creatorsResolver());
  registerCreatorOperations(operations, databases.creatorsResolver(), accounts);
  await databases.createNewAccount(account);
  const coordinator = await createMcpCoordinator({ registry: operations, sessions, results, authorize: async token => sessions.authorize(token) });
  const repository = (await databases.creatorsResolver().resolve(account)).repository;
  async function call(operation_id: string, arguments_: Record<string, unknown>, call_id: string) {
    const contract = await coordinator.call(session.session_id, "onecrew_tool_get", { operation_ids: [operation_id] });
    const parsed = contract as { operations: { schema_revision: string }[] };
    return { call_id, operation_id, schema_revision: parsed.operations[0]!.schema_revision, authority: { kind: "direct" }, arguments: arguments_ };
  }
  return { account, accounts, databases, sessions, session, operations, results, coordinator, repository, call,
    async close() { results.close(); sessions.close(); databases.close(); await lease.close(); await rm(root, { recursive: true }); } };
}

test("real SQLite MCP batch confirms each Creator owner separately and preserves completed calls on cancellation", async () => {
  const value = await fixture();
  try {
    const calls = [await value.call("onecrew.creator.list", {}, "before"), await value.call("onecrew.creator.batch.create", { idempotency_key: "first", validation: "strict", items: [{ display_name: "First actual Creator" }] }, "first"), await value.call("onecrew.creator.batch.create", { idempotency_key: "second", validation: "strict", items: [{ display_name: "Second actual Creator" }] }, "second"), await value.call("onecrew.creator.list", {}, "after")];
    const executed = await value.coordinator.call(value.session.session_id, "onecrew_tool_execute", { calls, presentation: "requested" }) as { result_ref: string; outcomes: { status: string }[] };
    assert.deepEqual(executed.outcomes.map(call => call.status), ["completed", "input_required", "skipped", "skipped"]);
    assert.equal(value.repository.list({}).items.length, 0);
    const shown = value.results.present(value.session.session_id, executed.result_ref);
    wireResultSnapshot.parse(shown);
    const first = await value.results.state(value.session.session_id, { action: "confirm", binding: shown.binding });
    if (!("snapshot" in first)) throw new Error("Expected next real review");
    wireResultSnapshot.parse(first.snapshot);
    assert.equal(first.snapshot.state.kind, "available");
    assert.equal(first.snapshot.batch.outcomes[1]!.disposition.kind, "succeeded");
    assert.equal(first.snapshot.batch.outcomes[2]!.disposition.kind, "not_run");
    assert.equal(value.repository.list({}).items.length, 1);
    await assert.rejects(value.results.state(value.session.session_id, { action: "confirm", binding: shown.binding }), /revision/);
    const cancelled = await value.results.state(value.session.session_id, { action: "cancel", binding: first.snapshot.binding });
    if (!("snapshot" in cancelled)) throw new Error("Expected cancelled result");
    wireResultSnapshot.parse(cancelled.snapshot);
    assert.equal(cancelled.snapshot.batch.outcomes[1]!.disposition.kind, "succeeded");
    assert.deepEqual(cancelled.snapshot.presentation.sections, first.snapshot.presentation.sections);
    assert.equal(value.repository.list({}).items.length, 1);
  } finally { await value.close(); }
});

test("all eight calls validate before a real write and generic attended deletion consumes one private grant", async () => {
  const value = await fixture();
  try {
    const create = await value.call("onecrew.creator.group.create", { name: "Should not exist" }, "create");
    const invalid = await value.call("onecrew.creator.list", { unexpected: true }, "invalid");
    const reads = await Promise.all(Array.from({ length: 6 }, (_, index) => value.call("onecrew.creator.list", {}, `read-${index}`)));
    await assert.rejects(value.coordinator.call(value.session.session_id, "onecrew_tool_execute", { calls: [create, ...reads, invalid], presentation: "never" }));
    assert.equal(value.repository.groupList({}).items.length, 0);
    const group = value.repository.groupCreate({ name: "Actual deletion" });
    const deletion = await value.call("onecrew.creator.group.delete", { group_id: group.id, expected_revision: group.revision }, "delete");
    const executed = await value.coordinator.call(value.session.session_id, "onecrew_tool_execute", { calls: [deletion], presentation: "never" }) as { result_ref: string };
    assert.equal(value.repository.groupList({}).items.length, 1);
    const shown = value.results.present(value.session.session_id, executed.result_ref);
    assert.ok(shown.presentation.sections.some(section => section.kind === "fields" && section.fields.some(field => field.value === group.id)));
    const confirmed = await value.results.state(value.session.session_id, { action: "confirm", binding: shown.binding });
    if (!("snapshot" in confirmed)) throw new Error("Expected deletion receipt");
    wireResultSnapshot.parse(confirmed.snapshot);
    assert.equal(confirmed.snapshot.state.kind, "resolved");
    assert.equal(value.repository.groupList({}).items.length, 0);
    await assert.rejects(value.results.state(value.session.session_id, { action: "confirm", binding: confirmed.snapshot.binding }), /consumed/);
    await assert.rejects(value.coordinator.call(value.session.session_id, "onecrew_tool_get", { operation_ids: ["email.send_many", "onecrew_gmail_bulk_prepare"] }));
    await assert.rejects(value.coordinator.call(value.session.session_id, "onecrew_connection_list", {}), /inventory/);
  } finally { await value.close(); }
});

test("missing actual profile resolver fails before invocation and cannot create a Creator", async () => {
  const value = await fixture();
  try {
    const call = await value.call("onecrew.creator.candidate.save", { intent: "add_new", target: { platform: "youtube", channel_id: "UCabcdefghijklmnopqrstuv" }, connection: { platform: "youtube", service_credential_ref_id: "11111111-1111-4111-8111-111111111111" }, provenance: { kind: "direct_selection", source_url: "https://youtube.com/@public" }, tags: null, quality: {} }, "save");
    const result = await value.coordinator.call(value.session.session_id, "onecrew_tool_execute", { calls: [call], presentation: "requested" }) as { outcomes: { status: string; failure: { phase: string; effect: string; retry_safe: boolean } }[] };
    assert.equal(result.outcomes[0]!.status, "failed");
    assert.equal(result.outcomes[0]!.failure.phase, "before_invocation");
    assert.equal(result.outcomes[0]!.failure.effect, "not_invoked");
    assert.equal(result.outcomes[0]!.failure.retry_safe, true);
    assert.equal(value.repository.list({}).items.length, 0);
  } finally { await value.close(); }
});

test("actual MCP plan confirmation alone issues exact native search authorization", async () => {
  const value = await fixture();
  try {
    const descriptor = nativePlatformSearchCatalog().find(entry => entry.operation_id === "youtube.channels.search")!;
    const scope = { operation_id: descriptor.operation_id, schema_revision: descriptor.schema_revision, connection: { kind: "service_credential", toolkit: "youtube", credential_id: "actual-selected-key-reference" }, arguments: { query: "mechanical keyboards", count: 5 } };
    assert.throws(() => value.results.authorizeNativeSearch(value.account, scope), /confirmation_required/);
    const call = await value.call("onecrew.search.plan.prepare", { description: "Independent keyboard reviewers", keywords: ["keyboards"], scopes: [scope] }, "plan");
    const executed = await value.coordinator.call(value.session.session_id, "onecrew_tool_execute", { calls: [call], presentation: "never" }) as { result_ref: string };
    const shown = value.results.present(value.session.session_id, executed.result_ref);
    const confirmed = await value.results.state(value.session.session_id, { action: "confirm", binding: shown.binding });
    if (!("snapshot" in confirmed)) throw new Error("Expected real confirmed plan snapshot");
    const claimed = await value.results.state(value.session.session_id, { action: "claim_continuation", input: { binding: confirmed.snapshot.binding, expected_continuation_revision: confirmed.snapshot.continuation.continuation_revision, mode: "pending" } });
    const reference = z.object({ kind: z.literal("claimed_continuation"), claimed: z.object({ request: z.object({ context: z.object({ kind: z.literal("search_plan_confirmed"), confirmed_plan_ref: z.string() }) }) }) }).parse(claimed).claimed.request.context.confirmed_plan_ref;
    const approved = { ...scope, arguments: { ...scope.arguments, confirmed_plan_ref: reference } };
    const bound = value.results.authorizeNativeSearch(value.account, approved);
    bound.authorization.validateRequest(bound.operationType, bound.request);
    assert.throws(() => bound.authorization.validateRequest(Symbol("youtube.channels.search"), bound.request), /scope_mismatch/);
    assert.throws(() => value.results.authorizeNativeSearch(value.account, { ...approved, arguments: { ...approved.arguments, query: "unapproved topic" } }), /scope_mismatch/);
    const page = value.results.authorizeNativeSearch(value.account, { ...approved, arguments: { ...approved.arguments, cursor: "actual-followup-page" } });
    page.authorization.validateRequest(page.operationType, page.request);
    assert.throws(() => bound.authorization.validateRequest(bound.operationType, page.request), /scope_mismatch/);
    assert.throws(() => JSON.stringify(bound.authorization), /must not be serialized/);
  } finally { await value.close(); }
});

test("actual SDK and loopback HTTP carry Creator review, sealed confirmation and result continuation", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/mcp-execution-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "sdk-")), directory = await createPrivateDirectory(join(root, "private"));
  let databases: AccountDatabases | undefined;
  let actualSessions: McpSessions | undefined;
  let u64Creator = "";
  const runtime = await startLocalRuntime(directory, async lease => {
    const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "sdk-owner", email: "sdk@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
    databases = new AccountDatabases(lease, accounts);
    await databases.createNewAccount(account);
    const repository = (await databases.creatorsResolver().resolve(account)).repository;
    u64Creator = repository.create({ display_name: "Actual u64 Creator" }).creator_id;
    repository.update({ creator_id: u64Creator, follower_counts: { x: 18446744073709551615n } });
    const sessions = new McpSessions(accounts), operations = new OperationRegistry(accounts), results = new OwnerResults(accounts, sessions, databases.creatorsResolver());
    actualSessions = sessions;
    registerCreatorOperations(operations, databases.creatorsResolver(), accounts);
    const coordinator = await createMcpCoordinator({ registry: operations, sessions, results, authorize: async token => sessions.authorize(token) });
    return { operations, configureRoutes(app) {
      configureMcpCoordinatorRoutes(app, coordinator);
      app.post("/_local/mcp/session", context => context.json(sessions.create(account, "full")));
      app.post("/_local/mcp/session/close", async context => { const input = await context.req.json() as { session_id: string }; sessions.revoke(input.session_id); return context.json({ closed: true }); });
    }, async close() { results.close(); sessions.close(); databases!.close(); } };
  });
  const adapter = await createMcpAdapter(directory), client = new Client({ name: "actual-sqlite-protocol", version: "1" }), [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const text = (result: Awaited<ReturnType<Client["callTool"]>>) => {
    const content = result.content as { type: string; text: string }[];
    return parseApplicationJson(content[0]!.text);
  };
  try {
    await adapter.connect(serverTransport); await client.connect(clientTransport);
    const contract = text(await client.callTool({ name: "onecrew_tool_get", arguments: { operation_ids: ["onecrew.creator.batch.create"] } })) as { operations: { schema_revision: string }[] };
    const executed = text(await client.callTool({ name: "onecrew_tool_execute", arguments: { calls: [{ call_id: "actual-sdk-create", operation_id: "onecrew.creator.batch.create", schema_revision: contract.operations[0]!.schema_revision, authority: { kind: "direct" }, arguments: { idempotency_key: "actual-sdk-once", validation: "strict", items: [{ display_name: "Actual SDK Creator" }] } }], presentation: "requested" } })) as { result_ref: string };
    const presented = await client.callTool({ name: "onecrew_result_present", arguments: { result_ref: executed.result_ref, locale: "zh-CN" } });
    const snapshot = wireResultSnapshot.parse(parseApplicationJson(String(presented._meta?.["onecrew/result-json"])));
    assert.equal(snapshot.state.kind, "available");
    const confirmed = text(await client.callTool({ name: "onecrew_result_state", arguments: { application_json: stringifyApplicationJson({ action: "confirm", binding: snapshot.binding }) } })) as { snapshot: unknown };
    const completed = wireResultSnapshot.parse(confirmed.snapshot);
    assert.equal(completed.state.kind, "resolved");
    assert.equal(completed.batch.outcomes[0]!.disposition.kind, "succeeded");
    await assert.rejects(client.callTool({ name: "onecrew_result_state", arguments: { application_json: stringifyApplicationJson({ action: "confirm", binding: completed.binding }) } }));
    const claimed = text(await client.callTool({ name: "onecrew_result_state", arguments: { application_json: stringifyApplicationJson({ action: "claim_continuation", input: { binding: completed.binding, expected_continuation_revision: completed.continuation.continuation_revision, mode: "pending" } }) } })) as { kind: string; claimed: { request: { context: { kind: string } } } };
    assert.equal(claimed.kind, "claimed_continuation");
    assert.equal(claimed.claimed.request.context.kind, "execution");
    const getContract = text(await client.callTool({ name: "onecrew_tool_get", arguments: { operation_ids: ["onecrew.creator.get"] } })) as { operations: { schema_revision: string }[] };
    const full = await client.callTool({ name: "onecrew_tool_execute", arguments: { calls: [{ call_id: "u64-read", operation_id: "onecrew.creator.get", schema_revision: getContract.operations[0]!.schema_revision, authority: { kind: "direct" }, arguments: { creator_id: u64Creator } }], presentation: "requested" } });
    const precise = text(full) as { outcomes: { result: { creator: { follower_counts: { x: bigint } } } }[] };
    assert.equal(precise.outcomes[0]!.result.creator.follower_counts.x, 18446744073709551615n);
    assert.equal(full._meta?.["onecrew/lossless-required"], true);
    assert.deepEqual(full.structuredContent, { application_json: (full.content as { text: string }[])[0]!.text });
    assert.equal(actualSessions!.activeSessionCount(), 1);
    const child = new Client({ name: "actual-stdio-disconnect", version: "1" });
    const module = new URL("../src/service/mcp-adapter.ts", import.meta.url).href;
    const transport = new StdioClientTransport({ command: process.execPath, args: ["--input-type=module", "-e", `import {serveMcpStdio} from ${JSON.stringify(module)}; await serveMcpStdio(${JSON.stringify(directory)});`], stderr: "pipe" });
    try {
      await child.connect(transport);
      await child.callTool({ name: "onecrew_tool_get", arguments: { operation_ids: ["onecrew.creator.list"] } });
      assert.equal(actualSessions!.activeSessionCount(), 2);
    } finally { await child.close(); }
    const deadline = performance.now() + 5_000;
    while (actualSessions!.activeSessionCount() !== 1 && performance.now() < deadline) await new Promise(resolve => setTimeout(resolve, 25));
    assert.equal(actualSessions!.activeSessionCount(), 1);
  } finally { await client.close(); await adapter.close(); await runtime.close(); await rm(root, { recursive: true }); }
});

test("exact MCP confirmation rejects hidden/colliding fields and retains u64 and independent resource references", () => {
  assert.deepEqual(mcpExactFields({ followers: 18446744073709551615n, body: "one\ntwo" }, true).fields.map(field => field.value), ["one\ntwo", "18446744073709551615"]);
  assert.throws(() => mcpExactFields({ next_cursor: "one", "next-cursor": "two" }, true), /collision/);
  assert.throws(() => mcpExactFields({ api_key: "private" }, true), /unsafe/);
  const records = Array.from({ length: 60 }, (_, index) => ({ post_id: `post-${index}`, text: "public", language: "en" }));
  assert.ok(mcpExactFields(records, false).truncated > 0);
  assert.equal(mcpOutputResourceRefs(records).length, 60);
});

test("original eight-call prefix budget retains operation receipt and bounds only display", async () => {
  const data = await loadOriginalMcpRegistry(), operation = data.entries.find(entry => entry.kind === "operation" && entry.metadata.id === "onecrew.creator.get");
  if (operation?.kind !== "operation") throw new Error("Original Creator read declaration missing");
  const sections = mcpCompletedSections({ text: "x".repeat(8192) }, operation, 8);
  assert.ok(Buffer.byteLength(stringifyApplicationJson(sections)) <= 8192);
  assert.ok(sections.some(section => section.kind === "items" && section.heading === "Operations"));
  assert.ok(sections.some(section => section.kind === "notice" && section.body.includes("0 of 1")));
  assert.throws(() => mcpCompletedSections({ next_cursor: "one", "next-cursor": "two" }, operation, 8), /collision/);
});
