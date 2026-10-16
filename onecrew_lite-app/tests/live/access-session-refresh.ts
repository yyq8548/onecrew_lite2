import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout } from "node:timers/promises";
import test from "node:test";

test("actual authenticated Access refresh preserves the existing browser session", { timeout: 180_000 }, async t => {
  const directory = process.env.ONECREW_LIVE_DATA;
  assert.ok(directory, "An explicit private live preview directory is required");
  const record = JSON.parse(await readFile(join(directory, "server.json"), "utf8"));
  const control = { Authorization: `Bearer ${record.control_token}` };
  const ticketResponse = await fetch(`${record.url}/_local/workspace`, { method: "POST", headers: control });
  assert.equal(ticketResponse.status, 200);
  const ticket = new URL((await ticketResponse.json()).url).hash.slice(1);
  const exchange = await fetch(`${record.url}/api/local/session`, { method: "POST", headers: { Origin: record.url, "Content-Type": "application/json" }, body: JSON.stringify({ ticket: new URLSearchParams(ticket).get("onecrew_ticket") }) });
  assert.equal(exchange.status, 200);
  const cookie = exchange.headers.getSetCookie()[0]?.split(";")[0];
  assert.ok(cookie);
  const session = async () => {
    const response = await fetch(`${record.url}/api/session`, { headers: { Cookie: cookie } });
    assert.equal(response.status, 200, "The original browser cookie must survive a verified metadata refresh");
    return (await response.json()).result;
  };
  const before = await session();
  for (let cycle = 1; cycle <= 3; cycle++) {
    await setTimeout(31_000);
    const refreshed = await fetch(`${record.url}/_local/access/status`, { headers: control });
    assert.equal(refreshed.status, 200);
    assert.equal((await refreshed.json()).state, "signed_in");
    const after = await session();
    assert.equal(after.accountId, before.accountId);
    assert.equal(after.epoch, before.epoch);
    t.diagnostic(`Verified refresh cycle ${cycle}`);
  }
});
