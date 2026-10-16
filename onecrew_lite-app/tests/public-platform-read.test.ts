import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { projectNativeXProfile, projectNativeXTimeline } from "../src/platforms/browser/native-x-projection.ts";
import { projectNativeInstagramUsers } from "../src/platforms/browser/native-instagram-search.ts";
import { nativeYoutubeChannelSchema, projectNativeYoutubeChannel } from "../src/platforms/public-youtube-channel.ts";
import { youtubeChannelReadInput, publicReadScope, publicReadOperationIds } from "../src/platforms/public-read-operations.ts";
import { loadOriginalMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";

const root = new URL("./fixtures/public-platform-read/", import.meta.url);
const fixture = (name: string) => parseApplicationJson(readFileSync(new URL(name, root)));
const normalized = (value: unknown) => parseApplicationJson(Buffer.from(stringifyApplicationJson(value)));
const parsedCase = z.object({ input: z.unknown(), expected: z.unknown() });
const wire = (operation: string, result: unknown) => ({ batch_id: "public-read-contract", outcomes: [{ status: "completed", call_id: "read", operation_id: operation, result }], result_ref: null, presentation_failure: null });

test("原Rust X纯解析器四个真实timeline与完整User/u64字段逐项相同并符合MCP输出", async () => {
  const oracle = z.object({ timelines: z.array(parsedCase.extend({ name: z.string() })), profiles: z.array(parsedCase) }).parse(fixture("x.json"));
  const validator = new FrozenMcpOutputValidator(await loadOriginalMcpRegistry(), "full");
  for (const row of oracle.timelines) {
    const actual = projectNativeXTimeline(row.input); assert.deepEqual(normalized(actual), row.expected, row.name);
    validator.validate("onecrew_tool_execute", wire("x.user_posts.list", { page: actual }));
    validator.validate("onecrew_tool_execute", wire("x.post_thread.get", actual));
  }
  for (const row of oracle.profiles) {
    if (row.expected === null) { assert.throws(() => projectNativeXProfile(row.input), /profile_missing/); continue; }
    const actual = projectNativeXProfile(row.input); assert.deepEqual(normalized(actual), row.expected);
    validator.validate("onecrew_tool_execute", wire("x.profile.get", { profile: actual }));
  }
});
test("原Rust Instagram纯解析器保留i64边界与原可选公开资料字段", async () => {
  const cases = z.array(parsedCase).parse(fixture("instagram.json")), validator = new FrozenMcpOutputValidator(await loadOriginalMcpRegistry(), "full");
  for (const row of cases) { const actual = projectNativeInstagramUsers({ users: [row.input] })[0]; assert.deepEqual(normalized(actual), row.expected); validator.validate("onecrew_tool_execute", wire("instagram.profile.get", { profile: actual })); }
});
test("原Rust YouTube真实serde默认、u64数值与读取深度合同逐项匹配", async () => {
  const oracle = z.object({ channels: z.array(parsedCase), counts: z.array(parsedCase), light_omitted: z.array(z.string()), full_omitted: z.array(z.string()) }).parse(fixture("youtube.json"));
  const validator = new FrozenMcpOutputValidator(await loadOriginalMcpRegistry(), "full");
  for (const row of oracle.channels) {
    const decoded = nativeYoutubeChannelSchema.safeParse(row.input);
    if (row.expected === null) { assert.equal(decoded.success, false); continue; }
    assert.equal(decoded.success, true);
    const actual = projectNativeYoutubeChannel(decoded.data, null, { items: [] }, { items: [] }, "full"), expected = z.record(z.string(), z.unknown()).parse(row.expected);
    for (const [name, value] of Object.entries(expected)) assert.deepEqual(normalized(actual[name as keyof typeof actual]), value, name);
    validator.validate("onecrew_tool_execute", wire("youtube.channel.get", { channel: actual }));
  }
  for (const row of oracle.counts) {
    const decoded = nativeYoutubeChannelSchema.parse({ id: "UC_x5XG1OV2P6uZZ5FSM9Ttw", statistics: { subscriberCount: row.input } });
    const actual = projectNativeYoutubeChannel(decoded, null, null, null, "full"); assert.deepEqual(normalized(actual.subscriber_count), row.expected);
  }
  const channel = nativeYoutubeChannelSchema.parse({ id: "UC_x5XG1OV2P6uZZ5FSM9Ttw" });
  const light = projectNativeYoutubeChannel(channel, null, null, null, "light"); assert.deepEqual(light.omitted_sections, oracle.light_omitted); assert.deepEqual(light.unavailable_sections, ["recent_videos", "about"]);
  const full = projectNativeYoutubeChannel(channel, null, { items: [{ snippet: { resourceId: { videoId: "v" }, title: "title", description: "full description", publishedAt: "2026-01-01" } }] }, null, "full"); assert.deepEqual(full.omitted_sections ?? [], oracle.full_omitted); assert.deepEqual(full.unavailable_sections, ["public_comments", "about"]); assert.equal(full.recent_videos[0]?.description, "full description");
  assert.equal(youtubeChannelReadInput.safeParse({ channel_id: channel.id }).success, false);
  assert.equal(publicReadScope("youtube.channel.get").safeParse({ operation_id: "youtube.channel.get", schema_revision: "1", connection: { kind: "none" }, arguments: { channel_id: channel.id, depth: "light" } }).success, false);
});
test("五个保留公开读取scope严格校验连接与原thread范围且没有发送入口", () => {
  assert.deepEqual([...publicReadOperationIds], ["x.profile.get", "x.user_posts.list", "x.post_thread.get", "instagram.profile.get", "youtube.channel.get"]);
  const connection = { kind: "operator_account", toolkit: "x", operator_account_id: "owner-profile" }, base = { operation_id: "x.post_thread.get", schema_revision: "1", connection };
  for (const count of [1, 40, 100]) assert.equal(publicReadScope("x.post_thread.get").safeParse({ ...base, arguments: { content_id: "500", count } }).success, true);
  for (const count of [0, 101, 1.5, "40"]) assert.equal(publicReadScope("x.post_thread.get").safeParse({ ...base, arguments: { content_id: "500", count } }).success, false);
  assert.equal(publicReadScope("x.post_thread.get").safeParse({ ...base, connection: { ...connection, toolkit: "instagram" }, arguments: { content_id: "500" } }).success, false);
  assert.equal(publicReadScope("x.post_thread.get").safeParse({ ...base, arguments: { content_id: "500", text: "send" } }).success, false);
  assert.equal(publicReadScope("instagram.profile.get").safeParse({ operation_id: "instagram.profile.get", schema_revision: "1", connection: { ...connection, toolkit: "instagram" }, arguments: { handle: "creator" } }).success, true);
});
