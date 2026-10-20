import test from "node:test";
import assert from "node:assert/strict";
import { discoveryScale } from "../src/discovery/qualification.ts";

const criteria = { min_subscribers: 2000, pinterest_min_monthly_views_exclusive: 50000 };
const exact = (value: number) => ({ nominal: BigInt(value), lower: BigInt(value), upper: BigInt(value) });

test("Pinterest 使用包含粉丝门槛和严格月浏览量门槛的 OR", () => {
  assert.equal(discoveryScale(criteria, exact(1999), "50000 monthly views").state, "failed");
  assert.equal(discoveryScale(criteria, exact(1999), "50001 monthly views").state, "passed");
  assert.equal(discoveryScale(criteria, exact(2000), null).state, "passed");
  assert.equal(discoveryScale(criteria, null, "50001 monthly views").state, "passed");
});

test("Pinterest 缺失与圆整门槛保留未知", () => {
  assert.equal(discoveryScale(criteria, exact(1999), null).state, "unknown");
  assert.equal(discoveryScale(criteria, exact(1999), "5 万 月浏览量").state, "unknown");
  assert.equal(discoveryScale(criteria, exact(1999), "50K monthly views").state, "unknown");
  assert.equal(discoveryScale(criteria, null, "0 monthly views").state, "unknown");
  assert.equal(discoveryScale(criteria, exact(1999), "6 万 月浏览量").state, "passed");
});

test("本轮真实公开显示值通过月浏览量分支", () => {
  assert.equal(discoveryScale(criteria, exact(32), "119.8 万 月浏览量").state, "passed");
  assert.deepEqual(discoveryScale(criteria, exact(32), "119.8 万 月浏览量").passed_by, ["monthly_views"]);
  assert.equal(discoveryScale(criteria, exact(706), null).state, "unknown");
});

test("达标依据分别保留粉丝数、月浏览量和同时达标", () => {
  assert.deepEqual(discoveryScale(criteria, exact(2000), null).passed_by, ["followers"]);
  assert.deepEqual(discoveryScale(criteria, exact(2000), "119.8 万 月浏览量").passed_by, ["followers", "monthly_views"]);
  assert.deepEqual(discoveryScale(criteria, exact(1999), "50K monthly views").passed_by, []);
});

test("其他平台与旧 Pinterest 计划继续使用原粉丝条件", () => {
  assert.equal(discoveryScale(criteria, exact(32)).state, "failed");
  assert.equal(discoveryScale({ min_subscribers: 2000 }, exact(32), "119.8 万 月浏览量").state, "failed");
  assert.equal(discoveryScale({ pinterest_min_monthly_views_exclusive: 50000 }, null, "50000 monthly views").state, "failed");
});
