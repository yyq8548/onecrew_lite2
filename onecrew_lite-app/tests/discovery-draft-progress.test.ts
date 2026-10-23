import assert from "node:assert/strict";
import { test } from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { DiscoveryDraftProgressState } from "../src/discovery/draft-progress.ts";

test("原草稿进度按请求/账号/租户归属，完成时间冻结且下一请求不继承错误引用", () => {
  const accounts = new AccountContextService(), principal = { user: { id: "progress-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const }, context = accounts.activate(principal), state = new DiscoveryDraftProgressState(context, "request");
  state.phase("checking_codex"); state.phase("planning"); assert.equal(state.snapshot(context, "request")!.timings.length, 2); assert.equal(state.snapshot(context, "other"), null);
  state.fail("discovery_draft_conditions_unsupported", ["男明星"]);
  const ended = state.snapshot(context, "request")!; assert.deepEqual(ended.unsupported_conditions, ["男明星"]); assert.deepEqual(state.snapshot(context, "request"), ended);
  assert.deepEqual(new DiscoveryDraftProgressState(context, "next").snapshot(context, "next")!.unsupported_conditions, []);
  const other = accounts.activate({ ...principal, baseId: "different-base" }); assert.equal(state.snapshot(other, "request"), null);
});
