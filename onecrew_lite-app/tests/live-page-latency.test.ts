import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { createHash } from "node:crypto";

const directory = process.env.ONECREW_PAGE_LATENCY_DATA_DIRECTORY;
const campaignId = process.env.ONECREW_PAGE_LATENCY_CAMPAIGN_ID;
test("real installed Windows service keeps concurrent page reads below the browser connection timeout", { skip: !directory || !campaignId }, async () => {
  const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
  assert.ok(executable);
  const descriptor = JSON.parse(await readFile(join(directory!, "server.json"), "utf8"));
  const origin = descriptor.url;
  const control = { authorization: `Bearer ${descriptor.control_token}` };
  const status = await (await fetch(origin + "/_local/status", { headers: control })).json();
  assert.equal(status.resource_identity.executable_sha256, createHash("sha256").update(await readFile(executable)).digest("hex"));
  const issued = await fetch(origin + "/_local/workspace", { method: "POST", headers: control });
  assert.equal(issued.status, 200);
  const ticket = new URLSearchParams(new URL((await issued.json()).url).hash.slice(1)).get("onecrew_ticket");
  const exchange = await fetch(origin + "/api/local/session", { method: "POST", headers: { origin, "content-type": "application/json" }, body: JSON.stringify({ ticket }) });
  assert.equal(exchange.status, 200);
  const cookie = exchange.headers.getSetCookie().map(value => value.split(";")[0]).join("; ");
  const session = await (await fetch(origin + "/api/session", { headers: { cookie, origin } })).json();
  assert.equal(session.ok, true);
  const headers = { cookie, origin, "content-type": "application/json", "X-OneCrew-CSRF": session.result.csrf, "X-OneCrew-Account-Id": session.result.accountId, "X-OneCrew-Auth-Epoch": session.result.epoch };
  for (let round = 0; round < 3; round++) {
    const samples = await Promise.all(Array.from({ length: 7 }, async () => {
      const start = performance.now();
      const response = await fetch(origin + "/api/commands/onecrew_discovery_campaign_get", { method: "POST", headers, body: JSON.stringify({ id: campaignId }), signal: AbortSignal.timeout(10000) });
      assert.equal(response.status, 200);
      const result = await response.json();
      assert.equal(result.ok, true); assert.equal(result.result.id, campaignId);
      return Math.round(performance.now() - start);
    }));
    assert.ok(samples.every(value => value < 5000), `Concurrent page latency in milliseconds: ${samples.join(", ")}`);
  }
});
