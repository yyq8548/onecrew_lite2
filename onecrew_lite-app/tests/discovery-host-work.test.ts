import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as wait } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { test, type TestContext } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { HostOperationRuns, HostSemanticClient } from "../src/discovery/host-work.ts";
import { discoveryReviewFinding } from "../src/discovery/initial-match.ts";
import type { SemanticStructuredRequest } from "../src/discovery/semantic-client.ts";
import { resolveDiscoveryScoreDirectory } from "../src/discovery/saved-scores.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { loadCurrentMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { WorkspaceCardRequests } from "../src/service/workspace-card-requests.ts";

async function hostWorkStoreFixture(t: TestContext) {
  const scratch = fileURLToPath(new URL("../../.scratch/host-work-store-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "store-")), directory = await createPrivateDirectory(join(root, "private"));
  const lock = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lock, accounts);
  t.after(async () => { accounts.logout(); databases.close(); await lock.close(); await rm(root, { recursive: true }); });
  const context = accounts.activate({ user: { id: "host-store-owner", email: "host-store@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  await databases.createNewAccount(context);
  const owned = await resolveDiscoveryScoreDirectory(databases, context), decode = z.array(discoveryReviewFinding);
  const recorded = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  const reviews = (() => {
    try {
      return z.array(z.object({ discovery_campaign_id: z.string(), criteria_revision: z.number(), search_hit_id: z.string(), material_json: z.string(), findings_json: z.string() })).parse(recorded.prepare("SELECT discovery_campaign_id,criteria_revision,search_hit_id,material_json,findings_json FROM discovery_evidence_reviews ORDER BY created_at LIMIT 2").all()).map(row => ({ ...row, findings: decode.parse(parseApplicationJson(Buffer.from(row.findings_json))) }));
    } finally { recorded.close(); }
  })();
  assert.equal(reviews.length, 2);
  const requests = reviews.map(row => ({ prompt: row.material_json, scoringContext: "Validate transport using this recorded historical assessment; no new platform reads.", scope: { kind: "score" as const, campaign_id: row.discovery_campaign_id, revision: row.criteria_revision, search_hit_id: row.search_hit_id }, schema: z.toJSONSchema(decode, { io: "input" }), decode, planning: false, signal: context.signal, assertCurrent: async () => { accounts.assertCurrent(context); owned.assertOwned(); } }));
  const queueDirectory = join(owned.directory, "discovery-host-work");
  return { accounts, databases, context, owned, reviews, requests, queueDirectory, taskPath: join(queueDirectory, "tasks.sqlite") };
}

async function submitRecordedHostTask(client: HostSemanticClient, request: SemanticStructuredRequest<unknown>, result: unknown) {
  const completion = client.structured(request).then(value => ({ state: "completed", value }), error => ({ state: "failed", error }));
  const batch = await client.claim({ wait_ms: 2000 }, () => ({ state: "planning", execution: null }));
  assert.equal(batch.tasks.length, 1);
  const task = batch.tasks[0]!, submission = { task_id: task.task_id, lease: task.lease, request_id: `recorded-${task.task_id}`, result };
  const receipt = await client.submit(submission);
  assert.equal(receipt.state, "submitted");
  assert.deepEqual(await completion, { state: "completed", value: result });
  return { task, submission, receipt };
}

function storedHostTasks(path: string): unknown[] {
  const db = new DatabaseSync(path, { readOnly: true });
  try { return db.prepare("SELECT body FROM host_work_tasks ORDER BY id").all().map(row => parseApplicationJson(Buffer.from(z.string().parse(row.body)))); }
  finally { db.close(); }
}

test("三个真实已保存评估一次领取且规则只传一次，单项立即返回", { skip: !process.env.ONECREW_QUERY_FEEDBACK_DATABASE }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/host-batch-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const lock = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lock, accounts);
  const context = accounts.activate({ user: { id: "host-batch-owner", email: "host@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const recorded = new DatabaseSync(process.env.ONECREW_QUERY_FEEDBACK_DATABASE!, { readOnly: true }), abort = new AbortController();
  try {
    await databases.createNewAccount(context);
    const owned = await resolveDiscoveryScoreDirectory(databases, context), client = new HostSemanticClient(owned, context.signal), decode = z.array(discoveryReviewFinding);
    const rows = recorded.prepare("SELECT discovery_campaign_id,criteria_revision,search_hit_id,material_json,findings_json FROM discovery_evidence_reviews WHERE length(CAST(material_json AS BLOB))<7000 ORDER BY created_at DESC LIMIT 3").all();
    assert.equal(rows.length, 3);
    const requests = rows.map(row => ({ prompt: z.string().parse(row.material_json), scoringContext: "Replay recorded assessments using the same saved evaluation rules.", scope: { kind: "score" as const, campaign_id: z.string().parse(row.discovery_campaign_id), revision: Number(row.criteria_revision), search_hit_id: z.string().parse(row.search_hit_id) }, schema: z.toJSONSchema(decode, { io: "input" }), decode, planning: false, signal: abort.signal, assertCurrent: async () => { accounts.assertCurrent(context); owned.assertOwned(); } }));
    const first = client.structured(requests[0]!);
    for (let count = 0; client.waiting() < 1 && count < 100; count++) await wait(10);
    const single = await client.claim({ limit: 3, wait_ms: 15000 });
    assert.equal(single.tasks.length, 1, "不能等待凑批");
    const known = single.tasks.map(task => task.rule_id);
    await client.submit({ task_id: single.tasks[0]!.task_id, lease: single.tasks[0]!.lease, request_id: "single-recording", result: parseApplicationJson(Buffer.from(z.string().parse(rows[0]!.findings_json))) });
    await first;
    const pending = requests.map(request => client.structured({ ...request, prompt: request.prompt + "\nBatch replay." }));
    for (let count = 0; client.waiting() < 3 && count < 100; count++) await wait(10);
    const batch = await client.claim({ limit: 3, known_rule_ids: known });
    assert.equal(batch.tasks.length, 3);
    assert.deepEqual(batch.rules, {}, "已知规则不重复传送");
    assert.ok(batch.tasks.every(task => known.includes(task.rule_id)));
    await Promise.all(batch.tasks.map(task => {
      const row = rows.find(row => row.search_hit_id === task.scope.search_hit_id)!;
      return client.submit({ task_id: task.task_id, lease: task.lease, request_id: `batch-${task.task_id}`, result: parseApplicationJson(Buffer.from(z.string().parse(row.findings_json))) });
    }));
    await Promise.all(pending);
    assert.equal(client.waiting(), 0);
  } finally { abort.abort(); recorded.close(); databases.close(); await lock.close(); }
});

test("空队列有界等待使用真实计时器正常返回继续领取", async t => {
  const { owned, context, reviews } = await hostWorkStoreFixture(t);
  const client = new HostSemanticClient(owned, context.signal);
  const result = await client.claim({ campaign_id: reviews[0]!.discovery_campaign_id, wait_ms: 25 }, () => ({ state: "planning", execution: null }));
  assert.deepEqual(result.tasks, []);
  assert.equal(result.waiting, 0);
  assert.equal(result.next_action, "continue_claim");
});

test("真实历史审核经宿主领取、校验、幂等回执和重启缓存，不调用模型进程", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-host-work-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const lock = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lock, accounts);
  const context = accounts.activate({ user: { id: "host-transport-owner", email: "host@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url)), { readOnly: true });
  const controller = new AbortController(), decode = z.array(discoveryReviewFinding);
  try {
    await databases.createNewAccount(context); const owned = await resolveDiscoveryScoreDirectory(databases, context);
    const notices = new Set<string>(), cards = new WorkspaceCardRequests(), view = randomUUID(), account = context.principal.user.id;
    cards.register(view, account, null);
    const client = new HostSemanticClient(owned, context.signal, (notice, active) => {
      assert.ok(client.waiting() > 0, "通知发出前，模型任务必须已经可以领取");
      const retire = cards.hostWork(account, notice, active);
      notices.add(notice.id); return cancelled => { notices.delete(notice.id); retire(cancelled); };
    });
    const outputs = new FrozenMcpOutputValidator(await loadCurrentMcpRegistry(), "full");
    const idle = await client.claim({});
    assert.equal(idle.tasks.length, 0);
    assert.equal(idle.driver.state, "idle"); assert.equal(idle.next_action, "stop");
    await assert.rejects(client.claim({ wait_ms: 20001 }), /20000/);
    outputs.validate("onecrew_tool_execute", { batch_id: "host-idle-contract", outcomes: [{ status: "completed", call_id: "idle", operation_id: "onecrew.discovery.host_work.claim", result: idle }], result_ref: null, presentation_failure: null });
    const archived = db.prepare("SELECT discovery_campaign_id,criteria_revision,search_hit_id,material_json,findings_json FROM discovery_evidence_reviews ORDER BY created_at LIMIT 2").all();
    assert.equal(archived.length, 2);
    const rows = archived.map(row => ({ material_json: row.material_json, discovery_campaign_id: row.discovery_campaign_id, criteria_revision: row.criteria_revision, search_hit_id: row.search_hit_id, findings: parseApplicationJson(Buffer.from(z.string().parse(row.findings_json))) }));
    const requests = rows.map(row => ({ prompt: z.string().parse(row.material_json), scoringContext: "Validate transport using this recorded historical assessment; no new platform reads.", scope: { kind: "score" as const, campaign_id: z.string().parse(row.discovery_campaign_id), revision: Number(row.criteria_revision), search_hit_id: z.string().parse(row.search_hit_id) }, schema: z.toJSONSchema(decode, { io: "input" }), decode, planning: false, signal: controller.signal, assertCurrent: async () => { accounts.assertCurrent(context); owned.assertOwned(); } }));
    const operations = new HostOperationRuns();
    const operation = operations.start("transport-test", "recorded-reviews", async () => Promise.all(requests.map(request => client.structured(request))));
    const firstBatch = await client.claim({ wait_ms: 2000 }, () => ({ state: operations.pending("transport-test") ? "planning" : "idle", execution: null }));
    assert.ok(firstBatch.tasks.length > 0, "连续模型驱动在后台产生材料后直接领取，不依赖状态卡通知");
    assert.equal(firstBatch.next_action, "evaluate_and_submit");
    for (let attempt = 0; attempt < 100 && client.waiting() < 2; attempt++) await wait(10);
    assert.equal(notices.size, 2, "网页等待的模型任务必须通知当前对话，不能只排队");
    const captured: Awaited<ReturnType<HostSemanticClient["claim"]>>["tasks"] = [...firstBatch.tasks];
    for (let attempt = 0; attempt < 100 && captured.length < 2; attempt++) { captured.push(...(await client.claim({})).tasks); if (captured.length < 2) await wait(10); }
    assert.equal(captured.length, 2); assert.equal((await client.claim({})).tasks.length, 0);
    cards.started(account, requests[0]!.scope.campaign_id);
    for (let poll = 0; poll < 5; poll++) assert.deepEqual(cards.request({ view_id: view }, account, requests[0]!.scope.campaign_id, true), { kind: "none" }, "真实租约已由模型领取，评分和自动状态消息均不得重复入队");
    assert.equal(cards.request({ view_id: view, manual: true }, account, requests[0]!.scope.campaign_id, true).kind, "message");
    assert.equal((await client.claim({ campaign_id: "another-campaign" })).waiting, 0, "不能把其他任务的待办计入本轮");
    assert.equal((await client.claim({ wait_ms: 2000 })).next_action, "finish_claimed_work", "已有领取先提交，不能重复领取或空等");
    const invalid = await client.submit({ task_id: captured[0]!.task_id, lease: captured[0]!.lease, request_id: "invalid-shape", result: {} });
    assert.equal(invalid.state, "invalid");
    for (const task of captured) {
      const index = requests.findIndex(request => request.scope.search_hit_id === task.scope.search_hit_id);
      assert.ok(index >= 0);
      const submission = { task_id: task.task_id, lease: task.lease, request_id: `recorded-${index}`, result: rows[index]!.findings };
      const receipt = await client.submit(submission); assert.equal(receipt.state, "submitted");
      assert.deepEqual(await client.submit(submission), receipt);
      await assert.rejects(client.submit({ ...submission, request_id: "conflicting-intent" }), /receipt_conflict/);
      assert.deepEqual(cards.request({ view_id: view }, account, task.scope.campaign_id, true), { kind: "none" }, "提交与下一次领取之间不重复唤醒");
    }
    for (let attempt = 0; attempt < 100 && operations.pending("transport-test"); attempt++) await wait(10);
    const results = operations.get("transport-test", operation.operation_id).value as unknown[];
    assert.deepEqual(results, rows.map(row => decode.parse(row.findings)));
    assert.equal(notices.size, 0, "已提交任务不再发送唤醒通知");
    const restarted = new HostSemanticClient(owned, context.signal);
    assert.deepEqual(await restarted.structured(requests[0]!), results[0]); assert.equal((await restarted.claim({})).tasks.length, 0);
    const abort = new AbortController();
    const interrupted = client.structured({ ...requests[0]!, prompt: `${requests[0]!.prompt}\nPending replay cancellation.`, signal: abort.signal });
    const outcome = interrupted.then(() => null, error => error as unknown);
    for (let attempt = 0; attempt < 100 && !client.waiting(); attempt++) await wait(10);
    abort.abort(new Error("user_paused")); assert.match(String(await outcome), /user_paused/); assert.equal(client.waiting(), 0);
    assert.equal(notices.size, 0, "已取消任务必须撤销待发送通知");
    assert.equal((await client.claim({})).tasks.length, 0);
    const resumed = client.structured({ ...requests[0]!, prompt: `${requests[0]!.prompt}\nPending replay cancellation.` });
    for (let attempt = 0; attempt < 100 && !client.waiting(); attempt++) await wait(10);
    assert.deepEqual(cards.request({ view_id: view }, account, requests[0]!.scope.campaign_id, true), { kind: "none" });
    assert.equal(cards.request({ view_id: view }, account, requests[0]!.scope.campaign_id, true).kind, "message", "暂停取消后的同一任务可重新唤醒一次");
    assert.deepEqual(cards.request({ view_id: view }, account, requests[0]!.scope.campaign_id, true), { kind: "none" });
    const resumedTask = (await client.claim({})).tasks[0]!;
    await client.submit({ task_id: resumedTask.task_id, lease: resumedTask.lease, request_id: "resumed-recorded", result: rows[0]!.findings });
    assert.deepEqual(await resumed, decode.parse(rows[0]!.findings));
    const next = accounts.activate({ ...context.principal, user: { id: "different-host-owner", email: "different@example.com" } });
    await databases.createNewAccount(next);
    const other = new HostSemanticClient(await resolveDiscoveryScoreDirectory(databases, next), next.signal);
    await assert.rejects(other.get({ task_id: captured[0]!.task_id }), /not_found/);
    await assert.rejects(client.get({ task_id: captured[0]!.task_id }), /account changed/);
  } finally { controller.abort(); db.close(); databases.close(); await lock.close(); }
});

test("短模型入口保留真实异步结果，跨账号不能读取操作结果", async () => {
  const operations = new HostOperationRuns();
  const original = fileURLToPath(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url));
  const launched = operations.start("owner", original, async () => {
    const db = new DatabaseSync(original, { readOnly: true });
    try { return db.prepare("SELECT count(*) AS count FROM discovery_evidence_reviews").get(); } finally { db.close(); }
  });
  assert.equal(launched.state, "pending");
  for (let attempt = 0; attempt < 100 && operations.get("owner", launched.operation_id).state === "pending"; attempt++) await wait(10);
  assert.equal(operations.get("owner", launched.operation_id).state, "completed");
  assert.throws(() => operations.get("another-owner", launched.operation_id), /not_found/);
});

test("真实宿主任务从旧tasks.json完整导入，暂停任务可恢复且重启回执保持幂等", async t => {
  const fixture = await hostWorkStoreFixture(t), lifetime = new AbortController();
  const client = new HostSemanticClient(fixture.owned, AbortSignal.any([fixture.context.signal, lifetime.signal]));
  const completed = await submitRecordedHostTask(client, fixture.requests[0]!, fixture.reviews[0]!.findings);
  const interrupted = new AbortController(), interruption = new Error("host_store_restart");
  const suspended = client.structured({ ...fixture.requests[1]!, signal: interrupted.signal }).then(value => ({ value }), error => ({ error }));
  const batch = await client.claim({ wait_ms: 2000 }, () => ({ state: "planning", execution: null }));
  assert.equal(batch.tasks.length, 1);
  const suspendedTask = batch.tasks[0]!;
  interrupted.abort(interruption);
  assert.deepEqual(await suspended, { error: interruption });
  const completedState = await client.get({ task_id: completed.task.task_id, include_material: true });
  const suspendedState = await client.get({ task_id: suspendedTask.task_id, include_material: true });
  lifetime.abort();
  const tasks = storedHostTasks(fixture.taskPath);
  assert.equal(tasks.length, 2);
  const legacyPath = join(fixture.queueDirectory, "tasks.json"), legacy = Buffer.from(stringifyApplicationJson({ version: 1, owner: fixture.owned.owner, tasks }));
  await writeFile(legacyPath, legacy, { mode: 0o600, flag: "wx" });
  await rm(fixture.taskPath);

  const migrated = new HostSemanticClient(fixture.owned, fixture.context.signal);
  assert.deepEqual(await migrated.get({ task_id: completed.task.task_id, include_material: true }), completedState);
  assert.deepEqual(await migrated.get({ task_id: suspendedTask.task_id, include_material: true }), suspendedState);
  assert.deepEqual(storedHostTasks(fixture.taskPath), tasks);
  const database = new DatabaseSync(fixture.taskPath, { readOnly: true });
  try { assert.equal(database.prepare("PRAGMA user_version").get()?.user_version, 1); } finally { database.close(); }
  assert.deepEqual(await migrated.submit(completed.submission), completed.receipt);
  await assert.rejects(migrated.submit({ ...completed.submission, request_id: "conflicting-restart" }), /host_work_receipt_conflict/);
  assert.deepEqual(await migrated.structured(fixture.requests[0]!), fixture.reviews[0]!.findings);
  const resumed = await submitRecordedHostTask(migrated, fixture.requests[1]!, fixture.reviews[1]!.findings);
  assert.equal(resumed.task.task_id, suspendedTask.task_id);

  const restarted = new HostSemanticClient(fixture.owned, fixture.context.signal);
  assert.deepEqual(await restarted.submit(completed.submission), completed.receipt);
  assert.deepEqual(await restarted.submit(resumed.submission), resumed.receipt);
  assert.deepEqual(await restarted.structured(fixture.requests[1]!), fixture.reviews[1]!.findings);
  assert.equal((await restarted.claim({ wait_ms: 0 })).tasks.length, 0);
  assert.equal(storedHostTasks(fixture.taskPath).length, 2);
  assert.deepEqual(await readFile(legacyPath), legacy);
});

test("真实账号目录拒绝其他owner的SQLite任务和旧JSON导入", async t => {
  const fixture = await hostWorkStoreFixture(t), lifetime = new AbortController();
  const client = new HostSemanticClient(fixture.owned, AbortSignal.any([fixture.context.signal, lifetime.signal]));
  const completed = await submitRecordedHostTask(client, fixture.requests[0]!, fixture.reviews[0]!.findings);
  lifetime.abort();
  const tasks = storedHostTasks(fixture.taskPath), legacy = Buffer.from(stringifyApplicationJson({ version: 1, owner: fixture.owned.owner, tasks }));
  const next = fixture.accounts.activate({ ...fixture.context.principal, user: { id: "host-store-other-owner", email: "host-store-other@example.com" } });
  await fixture.databases.createNewAccount(next);
  const owned = await resolveDiscoveryScoreDirectory(fixture.databases, next), queueDirectory = await createPrivateDirectory(join(owned.directory, "discovery-host-work")), taskPath = join(queueDirectory, "tasks.sqlite");
  await copyFile(fixture.taskPath, taskPath);
  const other = new HostSemanticClient(owned, next.signal);
  await assert.rejects(other.get({ task_id: completed.task.task_id }), /host_work_owner_mismatch/);
  assert.deepEqual(storedHostTasks(taskPath), tasks);
  await rm(taskPath);
  const legacyPath = join(queueDirectory, "tasks.json");
  await writeFile(legacyPath, legacy, { mode: 0o600, flag: "wx" });
  const importing = new HostSemanticClient(owned, next.signal);
  await assert.rejects(importing.get({ task_id: completed.task.task_id }), /host_work_owner_mismatch/);
  const database = new DatabaseSync(taskPath, { readOnly: true });
  try {
    assert.equal(database.prepare("PRAGMA user_version").get()?.user_version, 0);
    assert.equal(database.prepare("SELECT name FROM sqlite_schema WHERE name='host_work_tasks'").get(), undefined);
  } finally { database.close(); }
  assert.deepEqual(await readFile(legacyPath), legacy);
});

for (const fault of ["corrupt", "unsupported_version"] as const) test(`真实宿主SQLite ${fault}明确失败并保留原文件`, async t => {
  const fixture = await hostWorkStoreFixture(t), lifetime = new AbortController();
  const client = new HostSemanticClient(fixture.owned, AbortSignal.any([fixture.context.signal, lifetime.signal]));
  const completed = await submitRecordedHostTask(client, fixture.requests[0]!, fixture.reviews[0]!.findings);
  lifetime.abort();
  const legacyPath = join(fixture.queueDirectory, "tasks.json"), legacy = Buffer.from(stringifyApplicationJson({ version: 1, owner: fixture.owned.owner, tasks: storedHostTasks(fixture.taskPath) }));
  await writeFile(legacyPath, legacy, { mode: 0o600, flag: "wx" });
  if (fault === "corrupt") await writeFile(fixture.taskPath, "not a sqlite database", { mode: 0o600 });
  else {
    const database = new DatabaseSync(fixture.taskPath);
    try { database.exec("PRAGMA user_version=2"); } finally { database.close(); }
  }
  const before = await readFile(fixture.taskPath), restarted = new HostSemanticClient(fixture.owned, fixture.context.signal);
  if (fault === "corrupt") await assert.rejects(restarted.get({ task_id: completed.task.task_id }), { code: "ERR_SQLITE_ERROR", message: /file is not a database/ });
  else await assert.rejects(restarted.get({ task_id: completed.task.task_id }), /host_work_storage_version_invalid/);
  assert.deepEqual(await readFile(fixture.taskPath), before);
  assert.deepEqual(await readFile(legacyPath), legacy);
});
