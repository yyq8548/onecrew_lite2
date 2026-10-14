import { readdir, readFile } from "node:fs/promises";

import { ACCESS_ORIGIN, SITE_ORIGIN } from "./site-deployment-config.mjs";
import { artifactForPath } from "./site-csp.mjs";
import "./check-site-csp.mjs";

const root = new URL("../", import.meta.url);

async function javascriptText(directory) {
  const assets = new URL(directory, root);
  const files = (await readdir(assets)).filter((name) => name.endsWith(".js"));
  return (await Promise.all(files.map((name) => readFile(new URL(name, assets), "utf8")))).join("\n");
}

const accessJavaScript = await javascriptText("web/dist/access/assets/");
const siteJavaScript = await javascriptText("site/build/client/assets/");

const forbidden = [
  ["Access", accessJavaScript, ["/product/creator-discovery", "/compare/lev8", "达人外联不该靠复制粘贴维持上下文"]],
  ["Site", siteJavaScript, ["/api/session", "/api/account", "/pairing/authorize", "onecrew-locale", "正在检查登录状态"]],
];

let failed = false;
for (const [name, source, markers] of forbidden) {
  for (const marker of markers) {
    if (source.includes(marker)) {
      failed = true;
      console.error(`${name} bundle contains forbidden marker: ${marker}`);
    }
  }
}

if (!siteJavaScript.includes(ACCESS_ORIGIN)) {
  failed = true;
  console.error("Site bundle does not contain the configured Access CTA origin");
}

const pages = JSON.parse(await readFile(new URL("site/marketing-pages.json", root), "utf8"));
const localizedPages = pages.flatMap((page) => ["zh", "en"].map((locale) => {
  const path = locale === "zh" ? (page.path === "/" ? "/zh" : `/zh${page.path}`) : page.path;
  return { locale, path, basePath: page.path };
}));
const legacyPaths = new Set(pages.map(({ path }) => path === "/" ? "/en" : `/en${path}`));
if (localizedPages.length !== 48) throw new Error("Site manifest must resolve to 48 localized pages");
for (const { locale, path, basePath } of localizedPages) {
  const artifact = artifactForPath(path);
  const html = await readFile(new URL(`site/build/client/${artifact}`, root), "utf8");
  const canonical = `${SITE_ORIGIN}${path === "/" ? "/" : path}`;
  const chinese = `${SITE_ORIGIN}${basePath === "/" ? "/zh" : `/zh${basePath}`}`;
  const english = `${SITE_ORIGIN}${basePath}`;
  for (const marker of [
    "<title>",
    'meta name="description"',
    `rel="canonical" href="${canonical}"`,
    `hreflang="zh-CN" href="${chinese}"`,
    `hreflang="en" href="${english}"`,
    `hreflang="x-default" href="${english}"`,
    'property="og:title"',
    'name="twitter:title"',
    'type="application/ld+json"',
    '<main',
    'window.__reactRouterContext',
  ]) {
    if (!html.includes(marker)) {
      failed = true;
      console.error(`Site artifact ${artifact} is missing: ${marker}`);
    }
  }
  for (const match of html.matchAll(/href="(\/[^"]*)"/g)) {
    const linkedPath = match[1].split(/[?#]/, 1)[0];
    if (legacyPaths.has(linkedPath)) {
      failed = true;
      console.error(`Site artifact ${artifact} links to legacy URL: ${match[1]}`);
    }
  }
}

for (const file of ["robots.txt", "sitemap.xml", "llms.txt"]) {
  try {
    await readFile(new URL(`web/dist/access/${file}`, root));
    failed = true;
    console.error(`Access artifact unexpectedly contains ${file}`);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const rendered = await readFile(new URL(`site/build/client/${file}`, root), "utf8");
  if (!rendered.includes(SITE_ORIGIN) || rendered.includes("__ONECREW_SITE_ORIGIN__")) {
    failed = true;
    console.error(`Site artifact ${file} was not rendered from the configured origin`);
  }
}

const sitemap = await readFile(new URL("site/build/client/sitemap.xml", root), "utf8");
if ((sitemap.match(/<loc>/g) ?? []).length !== 48) {
  failed = true;
  console.error("Site sitemap does not contain exactly 48 localized URLs");
}
const robots = await readFile(new URL("site/build/client/robots.txt", root), "utf8");
const llms = await readFile(new URL("site/build/client/llms.txt", root), "utf8");
if (!robots.includes("Allow: /")) {
  failed = true;
  console.error("Site robots.txt is missing explicit locale crawl paths");
}
for (const legacyPath of legacyPaths) {
  const legacyUrl = `${SITE_ORIGIN}${legacyPath}`;
  if (llms.includes(`](${legacyUrl})`) || llms.includes(`](${legacyUrl}:`)) {
    failed = true;
    console.error(`Site llms.txt links to legacy URL: ${legacyUrl}`);
  }
}
const sitemapLocations = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
if (sitemapLocations.some((location) => {
  const pathname = new URL(location).pathname;
  return /^\/en(?:\/|$)/.test(pathname);
})) {
  failed = true;
  console.error("Site sitemap contains a redirect or unprefixed URL");
}

if (failed) process.exitCode = 1;
else console.log("Access and Site build boundaries verified");
