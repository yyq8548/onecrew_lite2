import assert from "node:assert/strict";
import { test } from "node:test";
import { DiscoveryProjectionLane, discoveryScoreCapacity } from "../src/discovery/projection-lane.ts";

test("投影只排空当前与最新待办，评分并发三项加预读三项且不超过剩余人数", async () => {
  assert.deepEqual([30, 3, 1, 0].map(discoveryScoreCapacity), [6, 3, 1, 0]);
  assert.throws(() => discoveryScoreCapacity(-1), /remaining_invalid/);
  const lane = new DiscoveryProjectionLane(), entered = Promise.withResolvers<void>(), release = Promise.withResolvers<void>(), order: number[] = [];
  lane.request(async () => { entered.resolve(); await release.promise; order.push(1); });
  await entered.promise;
  lane.request(async () => { order.push(2); }); lane.request(async () => { order.push(3); });
  assert.equal(lane.busy, true); assert.deepEqual(order, []);
  release.resolve(); await lane.close();
  assert.deepEqual(order, [1, 3]); assert.equal(lane.busy, false);
  assert.throws(() => lane.request(async () => undefined), /closed/);
});

test("投影失败仍排空最新真实本地工作后报告全部失败", async () => {
  const lane = new DiscoveryProjectionLane(), entered = Promise.withResolvers<void>(), release = Promise.withResolvers<void>();
  const first = new Error("first projection failed"), second = new Error("second projection failed");
  lane.request(async () => { entered.resolve(); await release.promise; throw first; }); await entered.promise;
  lane.request(async () => { throw second; }); release.resolve();
  await assert.rejects(lane.close(), error => error instanceof AggregateError && assert.deepEqual(error.errors, [first, second]) === undefined);
  assert.equal(lane.busy, false);
});

test("运行检查点推进已结束投影，保留新投影直到排空", async () => {
  const lane = new DiscoveryProjectionLane(), completed = Promise.withResolvers<void>(), nextEntered = Promise.withResolvers<void>(), release = Promise.withResolvers<void>();
  lane.request(async () => { completed.resolve(); }); await completed.promise;
  lane.request(async () => { nextEntered.resolve(); await release.promise; });
  // 等待原Promise链完成，不依赖计时器或真实网络。
  for (let count = 0; count < 5; count++) await Promise.resolve();
  await lane.reap(); await nextEntered.promise; assert.equal(lane.busy, true);
  release.resolve(); await lane.close(); assert.equal(lane.busy, false);
});
