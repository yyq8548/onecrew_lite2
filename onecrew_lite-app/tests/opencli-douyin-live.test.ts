import assert from "node:assert/strict";
import { test } from "node:test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolveOpenCliInstallation } from "../src/platforms/opencli/installation.ts";
import { douyinProfilePlan } from "../src/platforms/opencli/douyin.ts";
import { validateOpenCliProfile } from "../src/platforms/opencli/registry.ts";
const { withOwnedSearchPage } = await import(new URL("../../onecrew_lite-opencli/onecrew-browser-search/owned-page.mjs", import.meta.url).href);
const { observeDouyinEvidence, projectDouyinProfile } = await import(new URL("../../onecrew_lite-opencli/onecrew-browser-search/douyin-evidence.mjs", import.meta.url).href);
interface LiveProfilePage {
  goto(url: string, options: { settleMs: number }): Promise<void>;
  sleep(seconds: number): Promise<void>;
  evaluate(expression: string): Promise<unknown>;
}

const evidence = process.env.ONECREW_DOUYIN_LIVE_EVIDENCE;
test("真实抖音主页在简介后存在群聊区域时仍读取原作者简介", { skip: !evidence, timeout: 60000 }, async () => {
  const context = process.env.ONECREW_DOUYIN_LIVE_PROFILE;
  const configuration = process.env.ONECREW_DOUYIN_LIVE_CONFIG_DIRECTORY;
  assert(context && configuration);
  validateOpenCliProfile(context);
  const captured = JSON.parse(await readFile(evidence!, "utf8"));
  const expected = douyinProfilePlan(captured.profile.author_id).decode([captured.profile]);
  const installation = await resolveOpenCliInstallation();
  assert(installation.script);
  const main = pathToFileURL(installation.script);
  assert.equal(JSON.parse(await readFile(new URL("../../package.json", main), "utf8")).version, "1.8.8");
  const previous = process.env.OPENCLI_CONFIG_DIR;
  process.env.OPENCLI_CONFIG_DIR = configuration;
  try {
    const { Page } = await import(new URL("./browser/page.js", main).href);
    const page = new Page("onecrew-douyin-regression-" + randomUUID(), 60, context, "background", "adapter", "ephemeral");
    const raw = await withOwnedSearchPage(page, async (owned: LiveProfilePage) => {
      await owned.goto(expected.profile_url, { settleMs: 1500 });
      await owned.sleep(2);
      return owned.evaluate(`(() => {
        const observe = ${observeDouyinEvidence.toString()};
        const observation = observe("profile",${JSON.stringify(expected.author_id)},${JSON.stringify(expected.profile_url)},false);
        const headers = [...document.querySelectorAll('[data-e2e="user-info"]')].filter(node => node.getClientRects().length);
        const metadata = headers.length === 1 ? [...headers[0].children].find(node => node.tagName === "P" && node.textContent.startsWith("抖音号：")) : null;
        return { observation, following: metadata?.nextElementSibling?.nextElementSibling?.textContent ?? null };
      })()`);
    });
    const result = raw && "session" in Object(raw) && "data" in Object(raw) ? raw.data : raw;
    assert(result.observation, "真实主页应能读取简介");
    assert.match(result.following, /加入TA的群聊/);
    const actual = douyinProfilePlan(expected.author_id).decode([projectDouyinProfile(result.observation, expected.author_id)]);
    assert.equal(actual.bio, expected.bio);
    assert.equal(actual.author_id, expected.author_id);
    assert(!actual.bio.includes("加入TA的群聊") && !actual.bio.includes("IP属地"));
  } finally {
    if (previous === undefined) delete process.env.OPENCLI_CONFIG_DIR;
    else process.env.OPENCLI_CONFIG_DIR = previous;
  }
});
