import assert from "node:assert/strict";
import { test } from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";
import { DiscoveryExecutionSupervisor } from "../src/discovery/execution-supervisor.ts";

const principal = { user: { id: "local-supervisor-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
const input = { campaign_id: "campaign", expected_revision: 1, profile_ref: "owned-browser", excluded_platforms: [] };

test("共同准入锁等待上传排空，完成状态之后仍阻止上传直到原工作真实排空", async () => {
  const accounts = new AccountContextService(), context = accounts.activate(principal), gate = new DiscoveryAdmissionGate(accounts), supervisor = new DiscoveryExecutionSupervisor(accounts, gate), upload = await gate.acquire(context);
  const started = Promise.withResolvers<void>(), release = Promise.withResolvers<void>(); let validations = 0;
  const pending = supervisor.start(context, input, 4, async () => { validations++; }, async owner => { owner.change(status => { status.stage = "finished"; }); started.resolve(); await release.promise; });
  await Promise.resolve(); assert.equal(validations, 0); upload.release();
  assert.equal((await pending).used, 4); await started.promise;
  assert.equal(supervisor.snapshot(context)!.stage, "finished"); assert.equal(supervisor.blocksBackendSync(), true);
  await assert.rejects(supervisor.start(context, { ...input, campaign_id: "other" }, 4, async () => undefined, async () => undefined), /busy/);
  let idle = false; const draining = supervisor.waitForIdle(context).then(status => { idle = true; return status; });
  await Promise.resolve(); assert.equal(idle, false);
  release.resolve(); assert.equal((await draining)?.stage, "finished"); assert.equal(supervisor.blocksBackendSync(), false);
  await supervisor.close();
  await assert.rejects(supervisor.start(context, input, 4, async () => undefined, async () => undefined), /stopping/);
});

test("关闭发生在真实准入检查期间时，检查完成后不能登记新工作", async () => {
  const accounts = new AccountContextService(), context = accounts.activate(principal), supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts));
  const entered = Promise.withResolvers<void>(), release = Promise.withResolvers<void>(); let runs = 0;
  const starting = supervisor.start(context, input, 0, async () => { entered.resolve(); await release.promise; }, async () => { runs++; });
  await entered.promise; await supervisor.close(); release.resolve();
  await assert.rejects(starting, /discovery_execution_stopping/); assert.equal(runs, 0); assert.equal(supervisor.snapshot(context), null); assert.equal(supervisor.blocksBackendSync(), false);
});

test("暂停取消下一次读取但等待已开始本地工作，重复启动不重复工作且其他账号不能读取旧状态", async () => {
  const accounts = new AccountContextService(), context = accounts.activate(principal), gate = new DiscoveryAdmissionGate(accounts), supervisor = new DiscoveryExecutionSupervisor(accounts, gate), started = Promise.withResolvers<void>(), finish = Promise.withResolvers<void>(); let runs = 0;
  await supervisor.start(context, input, 1, async () => undefined, async owner => { runs++; started.resolve(); await finish.promise; assert.equal(owner.signal.aborted, true); owner.change(status => { status.stage = "paused"; }); });
  await started.promise;
  assert.equal((await supervisor.start(context, input, 1, async () => undefined, async () => { runs++; })).stage, "searching"); assert.equal(runs, 1);
  assert.equal(supervisor.pause(context)!.stage, "pausing"); assert.equal(supervisor.blocksBackendSync(), true);
  const next = accounts.activate({ ...principal, user: { id: "other-owner", email: "other@example.com" } }); assert.equal(supervisor.snapshot(next), null);
  finish.resolve(); await supervisor.close(); assert.equal(supervisor.blocksBackendSync(), false);
});

test("工作失败保留原异常，close等待排空后报告错误而不声明成功", async () => {
  const accounts = new AccountContextService(), context = accounts.activate(principal), supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts)), failure = new Error("actual local projection failed");
  await supervisor.start(context, input, 0, async () => undefined, async () => { throw failure; });
  await assert.rejects(supervisor.close(), error => error instanceof AggregateError && error.errors[0] === failure);
  assert.equal(supervisor.snapshot(context)!.stage, "error"); assert.equal(supervisor.blocksBackendSync(), false);
});

test("单独Library投影也通过共同准入并阻止上传直到本地工作排空，不制造搜索状态", async () => {
  const accounts = new AccountContextService(), context = accounts.activate(principal), supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts)), entered = Promise.withResolvers<void>(), release = Promise.withResolvers<void>();
  const task = supervisor.ownTask(context, async () => { entered.resolve(); await release.promise; return 42; });
  await entered.promise; assert.equal(supervisor.snapshot(context), null); assert.equal(supervisor.blocksBackendSync(), true);
  let closed = false; const closing = supervisor.close().then(() => { closed = true; }); await Promise.resolve(); assert.equal(closed, false);
  release.resolve(); assert.equal(await task, 42); await closing; assert.equal(supervisor.blocksBackendSync(), false);
});
