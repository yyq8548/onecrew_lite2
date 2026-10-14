#!/usr/bin/env node

// Reproduce a connection and result-shape inventory from one pinned OpenCLI tree.
// This is a source audit, not a login or live-search check.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const manifestPath = process.argv[2] && resolve(process.argv[2]);
if (!manifestPath) {
  process.stderr.write("Usage: node audit-opencli-search.mjs /path/to/OpenCLI/cli-manifest.json\n");
  process.exit(2);
}

const root = dirname(manifestPath);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const version = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")).version;
const commit = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
if (!Array.isArray(manifest)) throw new Error("OpenCLI manifest must be an array");

const queryNames = new Set(["query", "keyword", "keywords", "word", "jql", "cql"]);
const linkNames = ["url", "profile_url", "item_url", "link"];
// Creator-lead entry points reviewed for OneCrew's fixed, read-only facade.
// Bilibili and TikTok use OneCrew add-ons for stronger author identity.
const creatorCommands = new Set([
  "youtube/search", "tiktok/search", "instagram/search", "twitter/search",
  "linkedin/people-search", "linkedin/salesnav-search", "facebook/search",
  "pinterest/search-users", "pinterest/search-pins", "pinterest/search-boards",
  "reddit/search", "bilibili/search", "xiaohongshu/search", "rednote/search",
  "weixin/search", "zhihu/search", "douyin/search", "weibo/search",
  "tieba/search", "1point3acres/search", "hupu/search", "weread/search",
  "medium/search", "substack/search", "apple-podcasts/search",
  "linkedin-learning/search", "archive/search", "bluesky/search", "jike/search",
  "hackernews/search", "linux-do/search", "nowcoder/search", "sinablog/search",
  "pixiv/search", "stackoverflow/search", "google/search", "brave/search",
  "duckduckgo/search", "yahoo/search", "dribbble/shot", "dribbble/designer", "google/images",
  "stackoverflow/user",
  "douban/search",
  "v2ex/node",
  "reddit/subreddit",
  "devto/tag", "lesswrong/tag", "lobsters/tag", "medium/tag", "stackoverflow/tag",
  "juejin/hot", "producthunt/posts", "hf/spaces",
  "hf/models", "hf/datasets",
]);
const scopedDiscovery = new Set(["v2ex/node", "reddit/subreddit", "devto/tag", "lesswrong/tag", "lobsters/tag", "medium/tag", "stackoverflow/tag", "juejin/hot", "hf/spaces", "hf/models", "hf/datasets"]);
// Every excluded search-shaped command needs an explicit, reviewable reason.
// Keep this list exact: new upstream commands must fail the audit until reviewed.
const excludedByReason = {
  "商品、交易或金融标的；结果不是 Creator 内容": [
    "1688/search", "amazon/bestsellers", "amazon/new-releases", "amazon/search",
    "coingecko/trending", "coupang/search", "dianping/search", "dongchedi/search",
    "jd/search", "smzdm/search", "steam/search", "taobao/search", "xianyu/search",
    "xueqiu/search",
  ],
  "交通、住宿或旅游服务；结果不是 Creator 内容": [
    "12306/stations", "booking/search", "ctrip/attraction", "ctrip/bus", "ctrip/cruise",
    "ctrip/ferry", "ctrip/flight", "ctrip/flight-round", "ctrip/hotel-search",
    "ctrip/hotel-suggest", "ctrip/package", "ctrip/search", "ctrip/tour", "ctrip/train",
    "trip/attraction", "trip/flight", "trip/flight-round", "trip/hotel-search",
    "trip/package", "trip/search", "trip/tour",
  ],
  "招聘、人才或招投标记录；结果不是公开 Creator 内容": [
    "51job/search", "boss/search", "indeed/search", "jianyu/detail", "jianyu/search",
    "linkedin/search", "maimai/search-talents", "powerchina/search", "upwork/search",
  ],
  "学术、法规、参考数据或软件包；结果不用于 Creator 线索": [
    "arxiv/search", "baidu-scholar/search", "cnki/search", "crates/search",
    "dblp/search", "dblp/venue", "dictionary/examples", "dictionary/search",
    "dictionary/synonyms", "dockerhub/search", "flathub/search", "gitee/search",
    "google-scholar/cite", "google-scholar/search", "gov-law/search", "gov-policy/search",
    "maven/search", "mdn/search", "npm/search", "nuget/search", "oeis/search",
    "openalex/search", "openfda/drug-label", "openfda/food-recall", "openreview/search",
    "packagist/search", "pubmed/author", "pubmed/clinical-trial", "pubmed/journal",
    "pubmed/mesh", "pubmed/related", "pubmed/review", "pubmed/search",
    "rubygems/search", "semanticscholar/search", "wanfang/search", "wikidata/search",
    "wikipedia/search", "zlibrary/search",
  ],
  "新闻编辑或影视条目；缺少可验证的 Creator 身份": [
    "36kr/search", "google/news", "imdb/search", "reuters/search", "tvmaze/search",
  ],
  "账号内或私密工作区内容；不可作为跨账号公开发现源": [
    "band/posts", "chatgpt/deep-research-result", "confluence/search", "discord-app/search",
    "gemini/deep-research-result", "gmail/search", "jira/search", "kimi/templates",
    "midjourney/history", "mubu/search", "qoder/search", "slock/message-search",
    "trae-solo/skill-search", "zsxq/search",
  ],
  "仅返回建议词、标签、地点或本账号内容；不返回可追溯 Creator 作品": [
    "douyin/hashtag", "douyin/location", "douyin/videos", "duckduckgo/suggest",
    "google/suggest", "google/trends", "nowcoder/hot", "nowcoder/suggest",
  ],
  "依赖已知账号、作品或频道；用于后续验证而非新候选发现": [
    "apple-podcasts/episodes", "dribbble/service", "linkedin/posts",
    "linkedin/profile-analytics", "nowcoder/detail", "tieba/posts", "weread/book-search",
  ],
  "只返回音轨和平台 URI，没有可追溯的 Creator 内容网页": ["spotify/search"],
  "活动条目有主办方显示名，但未提供可验证的 Creator 账号": ["huodongxing/events"],
  "混合玩家、战队、赛事和文章；未提供稳定的 Creator 身份": ["hltv/search"],
  "仅发现频道推荐，无可追溯搜索结果 URL": ["instagram/explore"],
  "公众号／文章搜索需 WEREAD_API_KEY，输出仅 weread:// 深链，缺少可验证 HTTPS 原文来源": ["weread-official/search"],
};
const exclusionReasons = new Map();
for (const [reason, names] of Object.entries(excludedByReason)) {
  for (const name of names) {
    if (exclusionReasons.has(name)) throw new Error(`Duplicate exclusion: ${name}`);
    exclusionReasons.set(name, reason);
  }
}
const commands = manifest.filter(command => command.access === "read"
  && (command.name.includes("search")
    || scopedDiscovery.has(`${command.site}/${command.name}`)
    || ["posts", "videos"].includes(command.name)
    || /search|find|discover|搜索|查找|检索|发现/i.test(command.description ?? "")
    || (command.args ?? []).some(arg => queryNames.has(arg.name))));
const methodCounts = new Map();

function connection(command) {
  const method = command.browser ? `Chrome ${command.strategy}` : `direct ${command.strategy}`;
  methodCounts.set(method, (methodCounts.get(method) ?? 0) + 1);
  return method;
}

function required(command) {
  return (command.args ?? []).filter(arg => arg.required).map(arg => arg.name).join(", ") || "—";
}

function searchShape(command) {
  if (["hf/models", "hf/datasets", "hf/spaces"].includes(`${command.site}/${command.name}`)) return "keyword filter";
  const requiredArgs = (command.args ?? []).filter(arg => arg.required);
  return requiredArgs.some(arg => queryNames.has(arg.name)) ? "keyword" : "scoped/list";
}

function sourceField(command) {
  const columns = command.columns ?? [];
  return linkNames.find(name => columns.includes(name)) ?? "—";
}

function cell(value) {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}

const rows = commands.map(command => ({
  name: `${command.site}/${command.name}`,
  method: connection(command),
  shape: searchShape(command),
  required: required(command),
  source: sourceField(command),
  selected: creatorCommands.has(`${command.site}/${command.name}`),
  reason: exclusionReasons.get(`${command.site}/${command.name}`) ?? "",
})).sort((a, b) => a.name.localeCompare(b.name, "en"));

const manifestNames = new Set(commands.map(command => `${command.site}/${command.name}`));
const missing = [...creatorCommands].filter(command => !manifestNames.has(command));
if (missing.length) throw new Error(`Selected commands missing from pinned manifest: ${missing.join(", ")}`);
const unreviewed = rows.filter(row => row.selected === Boolean(row.reason));
const staleExclusions = [...exclusionReasons.keys()].filter(name => !manifestNames.has(name));
if (unreviewed.length || staleExclusions.length) {
  throw new Error(`Unreviewed or contradictory selections: ${unreviewed.map(row => row.name).join(", ")}; stale exclusions: ${staleExclusions.join(", ")}`);
}
// Option-filtered feeds can be missed by a name/description search heuristic.
// Recheck them separately whenever the pinned manifest changes.
const secondaryReasons = new Map(Object.entries({
  "arxiv/recent": "学术论文分类列表，不是 Campaign Creator 内容",
  "bbc/topic": "编辑新闻栏目，没有可验证的创作者身份",
  "linux-do/feed": "同站点已有正文搜索；此列表未声明作者字段",
  "medium/feed": "同站点已有搜索和标签入口；此列表未声明文章 URL",
  "mercury/reimbursement-plan": "私有报销草案，不是内容发现",
  "producthunt/browse": "同站点已有分类 posts；此列表仅有产品和评价，无作者",
  "substack/feed": "同站点已有关键词搜索；分类 feed 是补充排序，非独立平台",
  "toutiao/recommend": "频道推荐只有媒体来源标签，缺少可验证的 Creator 身份",
  "weread/ranking": "同站点已有搜索；此分类榜未声明来源 URL",
}));
const secondary = manifest.filter(command => command.access === "read"
  && !manifestNames.has(`${command.site}/${command.name}`)
  && (command.args ?? []).some(arg => ["search", "tag", "topic", "category", "scope", "qs"].includes(arg.name)))
  .map(command => `${command.site}/${command.name}`).sort();
const missingSecondary = secondary.filter(name => !secondaryReasons.has(name));
const staleSecondary = [...secondaryReasons.keys()].filter(name => !secondary.includes(name));
if (missingSecondary.length || staleSecondary.length) {
  throw new Error(`Unreviewed scoped reads: ${missingSecondary.join(", ")}; stale: ${staleSecondary.join(", ")}`);
}
const otherAttributedReads = manifest.filter(command => command.access === "read"
  && !manifestNames.has(`${command.site}/${command.name}`)
  && (command.columns ?? []).some(column => /author|creator|user|maker|member|designer|instructor|owner|channel|pinner|host/i.test(column))
  && (command.columns ?? []).some(column => /url|link/i.test(column)));

process.stdout.write(`# OpenCLI 搜索命令连接清单\n\n`);
process.stdout.write(`来源：官方 OpenCLI ${version}，提交 \`${commit}\` 的 \`cli-manifest.json\`。`);
process.stdout.write(`从全部只读命令中筛出名称、描述或查询参数指向搜索／发现的命令，并包含 \`posts\`／\`videos\` 列表及 ${scopedDiscovery.size} 个明确范围的节点／社区／标签／分类命令。\n\n`);
process.stdout.write(`共 ${rows.length} 个命令，${new Set(commands.map(command => command.site)).size} 个站点。`);
process.stdout.write(`此表只说明注册连接方式和声明字段；不证明账号已登录、现场搜索成功或适合 Creator Campaign。\n\n`);
process.stdout.write(`连接方式：${[...methodCounts].sort().map(([method, count]) => `${method} ${count}`).join("；")}。`);
process.stdout.write(`\`Chrome public\` 仍需扩展连接，\`direct public\` 不需浏览器。\n\n`);
process.stdout.write(`Creator Campaign 选择 ${creatorCommands.size} 个命令；其余 ${rows.length - creatorCommands.size} 个未接入，每项原因见末列。`);
process.stdout.write(`商品、职位、酒店、论文、内部消息、平台自身内容列表和无稳定网页来源的命令不进入 Creator 线索。`);
process.stdout.write(`Bilibili／TikTok 搜索由 OneCrew 专用适配器执行，保留原生作者身份。\n\n`);
process.stdout.write("| 命令 | 连接方式 | 输入形状 | 必填参数 | 来源 URL 字段 | Creator Campaign | 判断依据／未接入原因 |\n");
process.stdout.write("| --- | --- | --- | --- | --- | --- | --- |\n");
for (const row of rows) {
  process.stdout.write(`| ${[row.name, row.method, row.shape, row.required, row.source, row.selected ? "接入" : "未接入", row.selected ? "公开 Creator／内容发现入口；仍需逐条验证" : row.reason].map(cell).join(" | ")} |\n`);
}
process.stdout.write(`\n## 名称筛选之外的范围参数复核\n\n`);
process.stdout.write(`另外复核所有只读命令中带 \`search\`／\`tag\`／\`topic\`／\`category\`／\`scope\`／\`qs\` 参数的入口；上述表之外有 ${secondary.length} 个，逐项如下。`);
process.stdout.write(`同时扫描到 ${otherAttributedReads.length} 个表外只读命令声明了作者类字段与链接字段；它们主要是已知账号／作品详情、已有站点的首页或榜单，不因此视为可按 Campaign 目标搜索的新平台。\n\n`);
process.stdout.write("| 额外命令 | 本轮判断 |\n| --- | --- |\n");
for (const name of secondary) process.stdout.write(`| ${cell(name)} | ${cell(secondaryReasons.get(name))} |\n`);
