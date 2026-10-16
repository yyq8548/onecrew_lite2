import assert from "node:assert/strict";
import { test } from "node:test";
import { DiscoveryScoreGate } from "../src/discovery/score-gate.ts";

test("原评分门禁只允许两个不同作者，释放后重新接纳且重复释放不增加容量", () => {
  const gate = new DiscoveryScoreGate(), first = gate.acquire("author-a");
  assert.throws(() => gate.acquire("author-a"), /initial_match_busy/);
  const second = gate.acquire("author-b"); assert.throws(() => gate.acquire("author-c"), /initial_match_busy/);
  first.release(); const third = gate.acquire("author-c"); assert.throws(() => gate.acquire("author-b"), /initial_match_busy/);
  first.release(); assert.throws(() => gate.acquire("author-d"), /initial_match_busy/);
  second.release(); third.release(); gate.acquire("author-a").release();
});
