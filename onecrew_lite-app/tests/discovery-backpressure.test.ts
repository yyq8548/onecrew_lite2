import assert from "node:assert/strict";
import test from "node:test";
import { discoveryShouldHold } from "../src/discovery/backpressure.ts";

test("待核实达到10暂停发现，降至5恢复，两个阈值之间保持状态", () => {
  for (const count of [0, 5, 6, 9]) assert.equal(discoveryShouldHold(false, count), false);
  for (const count of [10, 11, 35]) assert.equal(discoveryShouldHold(false, count), true);
  for (const count of [6, 9, 10, 35]) assert.equal(discoveryShouldHold(true, count), true);
  for (const count of [0, 4, 5]) assert.equal(discoveryShouldHold(true, count), false);
  for (const count of [-1, NaN, 0.5]) assert.throws(() => discoveryShouldHold(false, count), /discovery_pending_count_invalid/);
});
