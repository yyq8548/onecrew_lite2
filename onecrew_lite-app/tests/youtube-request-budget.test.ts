import test from "node:test";
import assert from "node:assert/strict";
import { createServer, type ServerResponse } from "node:http";
import { once } from "node:events";
import { YoutubeRequestBudget } from "../src/platforms/browser/youtube-request-budget.ts";

test("YouTube credential budget holds six real HTTP requests and cancels queued work before invocation", async () => {
  const responses: ServerResponse[] = []; let reportSix!: () => void;
  const six = new Promise<void>(resolve => { reportSix = resolve; });
  const server = createServer((_, response) => { responses.push(response); if (responses.length === 6) reportSix(); });
  server.listen(0, "127.0.0.1"); await once(server, "listening"); const address = server.address(); assert.ok(address && typeof address !== "string");
  const budget = new YoutubeRequestBudget(), controller = new AbortController(), url = `http://127.0.0.1:${address.port}/`;
  const tasks = Array.from({ length: 6 }, () => budget.run(async signal => (await fetch(url, { signal })).text(), controller.signal));
  try {
    await six;
    const queued = new AbortController(); let invoked = false;
    const seventh = budget.run(async signal => { invoked = true; return (await fetch(url, { signal })).text(); }, queued.signal);
    queued.abort(); await assert.rejects(seventh, { name: "AbortError" }); assert.equal(invoked, false); assert.equal(responses.length, 6);
    for (const response of responses) response.end("released");
    assert.deepEqual(await Promise.all(tasks), Array(6).fill("released"));
    await budget.close(); assert.equal(responses.length, 6);
  } finally { controller.abort(); for (const response of responses) if (!response.writableEnded) response.end(); await Promise.allSettled(tasks); await budget.close(); server.close(); await once(server, "close"); }
});
