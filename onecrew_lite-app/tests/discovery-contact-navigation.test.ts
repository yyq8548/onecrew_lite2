import assert from "node:assert/strict";
import { test } from "node:test";
import { discoveryContactLeadMatches, discoveryContactLeadPriority, discoveryContactPageLinks, discoveryContactReadKey, discoveryContactSearchQuery, discoveryExactUsername, discoveryLinkedUsername, discoveryObservesLink, discoveryPromotional, discoveryThirdPartyLink } from "../src/discovery/contact-navigation.ts";
import { discoveryManualWechatLabels } from "../src/discovery/contact-native.ts";

test("原联系人导航只读取已见完整链接、同origin单跳且不把同名搜索当身份", () => {
  const url = "https://creator.example/contact";
  assert.equal(discoveryObservesLink(`Contact: (${url}).`, url), true);
  assert.equal(discoveryObservesLink(`${url}-other`, url), false);
  assert.equal(discoveryObservesLink("https://creator.example/cont…", url), false);
  assert.deepEqual(discoveryContactPageLinks("https://creator.example/", "[联系](/contact) https://creator.example/contact https://other.example/contact https://creator.example/shop"), [url]);
  for (const [source, title] of [["https://t.me/s/HW_HUB/21842", "HARDWARE HUB"], ["https://sponsorship.so/top-influencers/electronics", "Top 50 Electronics YouTube Influencers"], ["https://notrunner.example/contact", "Runnerup contact"]]) assert.equal(discoveryContactLeadMatches("Runner", null, source!, title!), false);
  assert.equal(discoveryContactLeadMatches("小Lin说", null, "https://agency.example/talent/42", "小Lin说商务合作联系方式"), true);
  assert.equal(discoveryContactLeadMatches("小Lin说", null, "https://news.example/articles/123", "小Lin说与会计师讨论经济"), false);
  assert.equal(discoveryContactLeadMatches("Hardware Canucks", null, "https://hardwarecanucks.com/contact", "Contact"), true);
});
test("原staff归属、推广过滤和已存读取身份保持一致", () => {
  const staff = "Follow the staff:\nAdam https://twitter.com/adam\nEric https://eric.example\n\nFollow The Studio:\nhttps://twitter.com/TheStudio\nhttps://www.instagram.com/thestudio/";
  assert.equal(discoveryThirdPartyLink("https://twitter.com/adam", staff), true);
  assert.equal(discoveryThirdPartyLink("https://twitter.com/TheStudio", staff), false);
  assert.equal(discoveryLinkedUsername(staff), "TheStudio");
  assert.equal(discoveryPromotional("https://bit.ly/item", "Buy shoes https://bit.ly/item"), true);
  assert.equal(discoveryPromotional("https://bit.ly/item", "Official website https://bit.ly/item"), false);
  assert.equal(discoveryPromotional("https://bit.ly/unknown", "Zygen NP-01 - https://bit.ly/unknown"), false);
  assert.equal(discoveryContactReadKey("https://twitter.com/boardzyxd/"), discoveryContactReadKey("https://x.com/boardzyxd"));
  assert.equal(discoveryContactReadKey("https://www.youtube.com/channel/UCGTxQKN8nd47EDvnbBWIY-g/about"), discoveryContactReadKey("https://www.youtube.com/channel/UCGTxQKN8nd47EDvnbBWIY-g"));
  assert.equal(discoveryExactUsername("https://www.pinterest.com/a/"), "a");
  assert.equal(discoveryExactUsername("https://news.ycombinator.com/user?id=a"), "a");
});
test("原批准语言词与作者标识保留，非法planner查询符号被拒绝", () => {
  for (const [language, expected] of [["ja", "お問い合わせ"], ["es-ES", "colaboración"], ["de", "Kooperation"], ["ru", "сотрудничество"], ["ar", "تواصل"], ["pt-BR", "contato"], ["th", "ติดต่อ"]]) assert.ok(discoveryContactSearchQuery("ThinkPadCreator", language!, []).includes(expected!));
  assert.ok(discoveryContactSearchQuery("Creator", "芬兰语", ["yhteystiedot", "yhteistyö"]).includes("yhteistyö"));
  assert.equal(discoveryContactSearchQuery("Creator", "芬兰语", ["site:evil"]).includes("evil"), false);
  assert.ok(discoveryContactLeadPriority("Creator", null, "https://creator.example/", "Home") < discoveryContactLeadPriority("Creator", null, "https://forum.example/posts/1", "Discuss Creator"));
});
test("原手动微信标签只保留明确bio标签，保持上限与去重", () => {
  assert.deepEqual(discoveryManualWechatLabels("公众号叫「一只马丁靴」，个人微信：DINGDINGCATHERINE读者分析请详见公众号"), [{ kind: "wechat", value: "DINGDINGCATHERINE" }, { kind: "wechat_official", value: "一只马丁靴" }]);
  assert.deepEqual(discoveryManualWechatLabels("看过微信文章，公众号很多，其他作者 test123，https://example.com"), []);
  assert.deepEqual(discoveryManualWechatLabels("微信号：abcdefghijklmnopqrstuvwxyz"), []);
  assert.equal(discoveryManualWechatLabels("个人微信：abc123\n个人微信：abc123").length, 1);
  assert.equal(discoveryManualWechatLabels(Array.from({ length: 15 }, (_, i) => `微信号：author${i}x`).join("\n")).length, 10);
});
