import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import test from "node:test";
import { WorkspaceCardRequests } from "../src/service/workspace-card-requests.ts";

test("one actual started plan permits one display request and manual current cards retire older views", () => {
  const owner = new WorkspaceCardRequests(), view = randomUUID(), old = randomUUID(), campaign = randomUUID();
  owner.register(view, "account-a", old); owner.started("account-a", campaign);
  assert.deepEqual(owner.claim(view, "account-a", campaign, true), { kind: "none" });
  const claimed = owner.claim(view, "account-a", campaign, true);
  assert.equal(claimed.kind, "message"); if (claimed.kind === "message") assert.match(claimed.message, /不要启动、恢复或批准搜索/u);
  owner.started("account-a", campaign);
  assert.deepEqual(owner.claim(view, "account-a", campaign, true), { kind: "none" });
  owner.register(randomUUID(), "account-a", campaign);
  assert.deepEqual(owner.request({ view_id: view }, "account-a", campaign, true), { kind: "none" });
  assert.equal(owner.request({ view_id: view, manual: true }, "account-a", campaign, true).kind, "message");
});
test("ambiguous chats, changed accounts, drafts and old starts never request business actions", () => {
  const owner = new WorkspaceCardRequests(), first = randomUUID(), second = randomUUID(), campaign = randomUUID();
  owner.register(first, "a", null); owner.register(second, "a", null); owner.started("a", campaign);
  assert.deepEqual(owner.claim(first, "a", campaign, false), { kind: "none" });
  assert.deepEqual(owner.claim(second, "a", campaign, false), { kind: "none" });
  assert.deepEqual(owner.claim(first, "a", campaign, false), { kind: "none" });
  assert.deepEqual(owner.claim(first, "b", campaign, false), { kind: "none" });
  const old = new WorkspaceCardRequests(), view = randomUUID(); old.started("a", campaign); old.register(view, "a", null);
  for (let index = 0; index < 2; index++) {
    assert.deepEqual(old.claim(view, "a", "draft", false), { kind: "none" });
    assert.deepEqual(old.claim(view, "a", campaign, false), { kind: "none" });
  }
});
test("a surviving view recovers only display state after process restart and binds one account", () => {
  const owner = new WorkspaceCardRequests(), view = randomUUID(), campaign = randomUUID(), baseline = randomUUID();
  owner.started("a", campaign);
  const request = { view_id: view, baseline_campaign_id: baseline };
  assert.deepEqual(owner.request(request, "a", campaign, false), { kind: "none" });
  const result = owner.request(request, "a", campaign, false);
  assert.equal(result.kind, "message");
  if (result.kind === "message") assert.match(result.message, /do not open or switch browser pages, start, resume or approve searches/u);
  assert.deepEqual(owner.request(request, "a", campaign, false), { kind: "none" });
  assert.deepEqual(owner.request({ ...request, manual: true }, "b", campaign, false), { kind: "none" });
  const login = new WorkspaceCardRequests(); login.register(view, null, null); login.started("a", campaign);
  assert.deepEqual(login.claim(view, "a", campaign, false), { kind: "none" });
  assert.deepEqual(login.claim(view, "b", campaign, false), { kind: "none" });
});

test("同一计划的连续模型任务只唤醒一次，不依赖已有搜索计划", () => {
  const owner = new WorkspaceCardRequests(), view = randomUUID(), campaign = randomUUID();
  owner.register(view, "account-a", null);
  const finishPlan = owner.hostWork("account-a", { id: "a".repeat(64), scope: { kind: "plan", campaign_id: campaign, revision: 0 } });
  assert.deepEqual(owner.request({ view_id: view }, "account-a", null, true), { kind: "none" });
  const first = owner.request({ view_id: view }, "account-a", null, true);
  assert.equal(first.kind, "message");
  if (first.kind === "message") {
    assert.match(first.message, /host_work\.claim/u); assert.match(first.message, /host_work\.submit/u);
    assert.match(first.message, /不要启动、恢复或批准搜索/u);
  }
  assert.deepEqual(owner.request({ view_id: view }, "account-a", null, true), { kind: "none" });
  finishPlan();
  const finishAudit = owner.hostWork("account-a", { id: "b".repeat(64), scope: { kind: "query_audit", campaign_id: campaign, revision: 0 } });
  assert.deepEqual(owner.request({ view_id: view }, "account-a", null, true), { kind: "none" });
  finishAudit();
  assert.deepEqual(owner.request({ view_id: view }, "account-a", null, true), { kind: "none" });
});

test("多张历史卡同时在线时，仅最新登记的当前账号卡接收模型任务", () => {
  const owner = new WorkspaceCardRequests(), first = randomUUID(), second = randomUUID(), campaign = randomUUID();
  owner.register(first, "a", null); owner.register(second, "a", null);
  const finish = owner.hostWork("a", { id: "c".repeat(64), scope: { kind: "plan", campaign_id: campaign, revision: 0 } });
  assert.deepEqual(owner.request({ view_id: first }, "b", null, true), { kind: "none" });
  assert.deepEqual(owner.request({ view_id: first }, "a", null, true), { kind: "none" });
  assert.deepEqual(owner.request({ view_id: second }, "a", null, true), { kind: "none" });
  assert.deepEqual(owner.request({ view_id: first }, "a", null, true), { kind: "none" });
  owner.register(randomUUID(), "a", null);
  assert.equal(owner.request({ view_id: second }, "a", null, true).kind, "message");
  assert.deepEqual(owner.request({ view_id: first }, "a", null, true), { kind: "none" });
  assert.deepEqual(owner.request({ view_id: second }, "a", null, true), { kind: "none" });
  finish();
  assert.deepEqual(owner.request({ view_id: first }, "a", null, true), { kind: "none" });
});

test("修复通知无需搜索计划，只向当前账号发送一次，完成后可退休", () => {
  const cards = new WorkspaceCardRequests(), view = randomUUID(), repair = randomUUID();
  cards.register(view, "a", null);
  let sent = 0;
  const retire = cards.repair("a", repair, () => { sent++; });
  assert.deepEqual(cards.request({ view_id: view }, "b", null, true), { kind: "none" });
  assert.deepEqual(cards.request({ view_id: view }, "a", null, true), { kind: "none" });
  const notice = cards.request({ view_id: view }, "a", null, true);
  assert.equal(notice.kind, "message");
  if (notice.kind === "message") {
    assert.ok(notice.message.includes(repair));
    assert.ok(notice.message.includes("onecrew.repair.get"));
    assert.ok(notice.message.includes("不要启动或恢复搜索"));
  }
  for (let index = 0; index < 3; index++) assert.deepEqual(cards.request({ view_id: view }, "a", null, true), { kind: "none" });
  assert.equal(sent, 1);
  retire();
  assert.deepEqual(cards.request({ view_id: view }, "a", null, true), { kind: "none" });
  cards.repair("a", randomUUID(), () => { sent++; })();
  assert.deepEqual(cards.request({ view_id: view }, "a", null, true), { kind: "none" });
  assert.equal(sent, 1);
});
