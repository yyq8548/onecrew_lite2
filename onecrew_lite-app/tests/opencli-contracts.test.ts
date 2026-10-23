import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { z } from "zod";
import { OpenCliRegistry, parseConnectedOpenCliProfiles, parseOpenCliBridgeStatus, parseOpenCliJson } from "../src/platforms/opencli/registry.ts";
import { CREATOR_SEARCH_SOURCES, RETIRED_CREATOR_SOURCES, creatorSearchArguments, creatorSearchSource } from "../src/platforms/opencli/sources.ts";
import { decodeCreatorSearch } from "../src/platforms/opencli/search-results.ts";
import { OpenCliAddons, OPENCLI_ADDON_FILES } from "../src/platforms/opencli/addons.ts";
import { OpenCliReader } from "../src/platforms/opencli/reader.ts";
import { resolveOpenCliInstallation } from "../src/platforms/opencli/installation.ts";

const bundled = fileURLToPath(new URL("../../onecrew_lite-opencli/", import.meta.url));

test("当前11来源使用审核后的只读入口，YouTube绑定独立页面", async () => {
  const fixture = z.object({ sources: z.array(z.object({ platform: z.string(), command: z.string(), browser_required: z.literal(true), registered: z.literal(true) })) }).parse(JSON.parse(await readFile(new URL("./fixtures/registered-sources.json", import.meta.url), "utf8")));
  const sources = Object.entries(CREATOR_SEARCH_SOURCES).filter(([, source]) => source.discovery).map(([platform, source]) => ({ platform, command: `${source.site}/${source.name}`, browser_required: true, registered: true }));
  const expected = fixture.sources.map(source => source.platform === "youtube" ? { ...source, command: "onecrew-search-youtube/search" } : source.platform === "yahoo" ? { ...source, command: "onecrew-search-yahoo/search" } : source);
  assert.equal(sources.length, 11);
  assert.deepEqual(sources, expected);
  for (const source of RETIRED_CREATOR_SOURCES) assert.throws(() => creatorSearchSource(source), { code: "opencli_read_command_unavailable" });
  for (const source of ["reddit", "pinterest", "google", "yahoo"]) assert.throws(() => creatorSearchArguments("profile", source, "query", 1), { code: "opencli_read_command_unavailable" });
  assert.throws(() => creatorSearchArguments("profile", "__proto__", "query", 1));
  assert.throws(() => creatorSearchArguments("profile", "twitter", "query", 0));
  assert.throws(() => creatorSearchArguments("profile", "linkedin", "query", 11));
  assert.throws(() => creatorSearchArguments("profile", "twitter", "query\nreply", 1));
  assert.throws(() => creatorSearchArguments("profile --eval", "twitter", "query", 1));
  assert.deepEqual(creatorSearchArguments("profile", "twitter", "--reply && arbitrary", 1), ["--profile", "profile", "onecrew-search-twitter", "search", "--limit", "1", "-f", "json", "--", "--reply && arbitrary"]);
});

test("只读注册与扩展通信合同采用原返回格式，写命令不通过读取门禁", async () => {
  const registry = new OpenCliRegistry(await parseOpenCliJson('[{"command":"youtube/search","site":"youtube","name":"search","access":"read","browser":true},{"command":"youtube/like","site":"youtube","name":"like","access":"write","browser":true}]'));
  assert.equal(registry.hasBrowserRead("youtube", "search"), true);
  assert.equal(registry.hasBrowserRead("youtube", "like"), false);
  assert.deepEqual(parseOpenCliBridgeStatus("Daemon: not running\n"), { daemon_running: false, extension_connected: false });
  assert.deepEqual(parseOpenCliBridgeStatus("Daemon: running (PID 123)\nExtension: 2 profiles connected, none selected\n"), { daemon_running: true, extension_connected: true });
  assert.deepEqual(parseConnectedOpenCliProfiles("  ctx_1 dedicated — connected\n  ctx_2 default — connected\n  ctx_3 — disconnected\n"), [{ context_id: "ctx_1", alias: "dedicated" }, { context_id: "ctx_2", alias: null }]);
  await assert.rejects(parseOpenCliJson("private invalid JSON"), { code: "opencli_invalid_output" });
});

test("原搜索结果合同保留原生ID和来源，拒绝错属、重复卡片与伪造字段", () => {
  const tiktok = { adapter_version: "1.0.0", id: "1234567890123456789", author_id: "9876543210123456789", author: "shop.creator", desc: "Ecommerce walkthrough\nOriginal content", url: "https://www.tiktok.com/@shop.creator/video/1234567890123456789" };
  assert.equal(decodeCreatorSearch("tiktok", [tiktok], 3)[0]!.author_id, tiktok.author_id);
  assert.throws(() => decodeCreatorSearch("tiktok", [{ ...tiktok, author_id: "0" }], 3));
  assert.throws(() => decodeCreatorSearch("tiktok", [tiktok, tiktok], 3));
  assert.throws(() => decodeCreatorSearch("tiktok", [{ ...tiktok, followers: 10000 }], 3));
  assert.throws(() => decodeCreatorSearch("bilibili", [], 3));
  assert.throws(() => decodeCreatorSearch("twitter", [{ id: "123", author: "one", text: "original", url: "https://x.com/other/status/123" }], 3));
  assert.throws(() => decodeCreatorSearch("google", [{ title: "ad", url: "https://www.google.com/url?target=x" }], 3));
  assert.throws(() => decodeCreatorSearch("yahoo", [{ title: "private", url: "https://127.0.0.1/" }], 3));
  assert.equal(decodeCreatorSearch("google", [{ title: "The Software Creators", url: "http://www.youtube.com/@TheSoftwareCreators" }], 1)[0]!.source_url, "https://www.youtube.com/@TheSoftwareCreators");
  assert.throws(() => decodeCreatorSearch("google", [{ title: "unreviewed transport", url: "http://example.org/" }], 1));
  assert.throws(() => decodeCreatorSearch("google", [{ title: "credential", url: "http://name@www.youtube.com/@TheSoftwareCreators" }], 1));
  const twitter = { id: "123", author: "@Owner", text: "original", url: "https://x.com/Owner/status/123" };
  assert.deepEqual(decodeCreatorSearch("twitter", [twitter, twitter], 3), [{ source_url: twitter.url, title: "original", author_label: "@Owner", author_id: "owner", author_url: "https://x.com/owner", material: { source_url: twitter.url, author_id: "owner", author_url: "https://x.com/owner", native_author_id: null, body_text: twitter.text, body_completeness: "unknown", bio: null } }]);
});

test("原bundled适配器清单每个文件实际存在，不包含测试或发送入口", async () => {
  for (const [id, files] of Object.entries(OPENCLI_ADDON_FILES)) for (const name of files) {
    assert.equal(name.includes("test"), false);
    assert.ok((await readFile(join(bundled, id, name))).length > 0);
  }
});

test("实际已安装1.8.8只读取版本和注册目录，不打开平台页面", async context => {
  const script = join(homedir(), ".opencli", "node_modules", "@jackwener", "opencli", "dist", "src", "main.js");
  const installed = process.env.ONECREW_OPENCLI_TEST_INSTALLATION === "1" ? await resolveOpenCliInstallation() : null;
  if (!installed && !existsSync(script)) { context.skip("No existing OpenCLI installation for local version inspection"); return; }
  const reader = await OpenCliReader.inspect(new OpenCliAddons(bundled, homedir()), installed ?? { executable: process.execPath, script });
  assert.equal(reader.version, "1.8.8");
  assert.ok(reader.inventory().command_count > 0);
});
