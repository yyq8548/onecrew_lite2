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
