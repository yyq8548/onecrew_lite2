import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { readVerifiedRuntime } from "../src/service/runtime.ts";

const directory = process.env.ONECREW_PAGE_LATENCY_DATA_DIRECTORY;
const campaign = process.env.ONECREW_PAGE_LATENCY_CAMPAIGN_ID;
const table = process.env.ONECREW_CREATOR_UPDATES_TABLE_ID;

test("installed unified publication notifies the actual table stream", { skip: !directory || !campaign || !table }, async () => {
  const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
  assert(executable);
  const runtime = await readVerifiedRuntime(directory!);
  const authorization = `Bearer ${runtime.control_token}`;
  const status = await (await fetch(runtime.url + "/_local/status", { headers: { authorization } })).json();
  assert.equal(status.resource_identity.executable_sha256, createHash("sha256").update(await readFile(executable)).digest("hex"));
  const issued = await fetch(runtime.url + "/_local/workspace", { method: "POST", headers: { authorization } });
  assert.equal(issued.status, 200);
  const ticket = new URLSearchParams(new URL((await issued.json()).url).hash.slice(1)).get("onecrew_ticket");
  const exchanged = await fetch(runtime.url + "/api/local/session", { method: "POST", headers: { authorization, "content-type": "application/json" }, body: JSON.stringify({ ticket }) });
  assert.equal(exchanged.status, 200);
  const session = await exchanged.json();
  const headers = { authorization, "content-type": "application/json", "x-onecrew-account-id": session.accountId, "x-onecrew-auth-epoch": session.epoch };
  const executing = await fetch(runtime.url + "/api/commands/onecrew_discovery_execution_get", { method: "POST", headers, body: "{}" });
  assert.equal(executing.status, 200);
  const execution = await executing.json();
  assert.equal(execution.ok, true);
  assert(!["searching", "verifying", "pausing"].includes(execution.result?.stage), "先暂停实际搜索再验证表格通知");
  const controller = new AbortController();
  const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(30000)]);
  const stream = await fetch(runtime.url + "/api/v1/dynamic-tables/" + encodeURIComponent(table!) + "/creator-updates", { headers, signal });
  assert.equal(stream.status, 200);
  assert(stream.body);
  const reader = stream.body.getReader();
  try {
    const initial = await reader.read();
    assert.equal(new TextDecoder().decode(initial.value), '{"changed":true}\n');
    const next = reader.read();
    const published = await fetch(runtime.url + "/api/commands/onecrew_discovery_library_sync", { method: "POST", headers, body: JSON.stringify({ campaignId: campaign }), signal });
    assert.equal(published.status, 200);
    const result = await published.json();
    assert.equal(result.ok, true);
    assert.equal(result.result.table_id, table);
    assert.equal(result.result.campaign_id, campaign);
    const notification = await next;
    assert.equal(notification.done, false);
    assert.equal(new TextDecoder().decode(notification.value), '{"changed":true}\n');
  } finally {
    controller.abort();
    reader.releaseLock();
  }
});
