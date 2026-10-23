import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as wait } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { HostOperationRuns, HostSemanticClient } from "../src/discovery/host-work.ts";
import { discoveryReviewFinding } from "../src/discovery/initial-match.ts";
import { resolveDiscoveryScoreDirectory } from "../src/discovery/saved-scores.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { loadCurrentMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { WorkspaceCardRequests } from "../src/service/workspace-card-requests.ts";

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
