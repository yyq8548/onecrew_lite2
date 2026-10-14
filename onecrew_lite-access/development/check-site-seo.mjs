import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parse } from "parse5";
import { SITE_ORIGIN } from "./site-deployment-config.mjs";
import { artifactForPath } from "./site-csp.mjs";

const root = new URL("../", import.meta.url);
const output = new URL("site/build/client/", root);
const pages = JSON.parse(await readFile(new URL("site/marketing-pages.json", root), "utf8"));
const sitemap = await readFile(new URL("sitemap.xml", output), "utf8");
const locations = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map(match => match[1]);
const expectedLocations = [];
const attrs = node => Object.fromEntries((node.attrs ?? []).map(({ name, value }) => [name, value]));
function nodes(node) { return [node, ...(node.childNodes ?? []).flatMap(nodes)]; }
function readableText(node) {
  if (["script", "style", "template"].includes(node.tagName)) return "";
  return node.nodeName === "#text" ? node.value : (node.childNodes ?? []).map(readableText).join(" ");
}

for (const page of pages) {
  const englishUrl = `${SITE_ORIGIN}${page.path}`;
  const chinesePath = page.path === "/" ? "/zh" : `/zh${page.path}`;
  const chineseUrl = `${SITE_ORIGIN}${chinesePath}`;
  for (const [path, language] of [[page.path, "en"], [chinesePath, "zh-CN"]]) {
    const canonical = `${SITE_ORIGIN}${path}`;
    expectedLocations.push(canonical);
    const document = parse(await readFile(new URL(artifactForPath(path), output), "utf8"));
    const all = nodes(document);
    const scripts = all.filter(node => node.tagName === "script").map(attrs);
    assert.equal(scripts.filter(script => script.src === "/ga4-init.js").length, 1, `${path}: one GA4 initializer`);
    assert.equal(scripts.filter(script => script.src === "https://www.googletagmanager.com/gtag/js?id=G-E8PCYC26GN").length, 1, `${path}: one Google tag`);
    const links = all.filter(node => node.tagName === "link").map(attrs);
    const meta = all.filter(node => node.tagName === "meta").map(attrs);
    assert.equal(attrs(all.find(node => node.tagName === "html")).lang, language, `${path}: HTML language`);
    assert.deepEqual(links.filter(link => link.rel === "canonical").map(link => link.href), [canonical], `${path}: self canonical`);
    for (const [lang, url] of [["en", englishUrl], ["zh-CN", chineseUrl], ["x-default", englishUrl]]) {
      assert.deepEqual(links.filter(link => link.rel === "alternate" && link.hreflang === lang).map(link => link.href), [url], `${path}: ${lang} alternate`);
    }
    assert.equal(meta.find(item => item.property === "og:url")?.content, canonical, `${path}: social URL`);
    assert.ok(meta.find(item => item.name === "description")?.content?.trim(), `${path}: description`);
    assert.ok(meta.some(item => item.name === "robots" && item.content === "index, follow"), `${path}: index/snippet eligibility`);
    assert.equal(all.filter(node => node.tagName === "h1").length, 1, `${path}: main heading`);
    assert.ok(readableText(all.find(node => node.tagName === "body")).trim().length > 200, `${path}: prerendered text`);
    for (const node of all.filter(node => node.tagName === "script" && attrs(node).type === "application/ld+json")) {
      JSON.parse((node.childNodes ?? []).map(child => child.value ?? "").join(""));
    }
    for (const node of all.filter(node => node.tagName === "a" && attrs(node).href)) {
      const url = new URL(attrs(node).href, SITE_ORIGIN);
      assert.ok(url.origin !== SITE_ORIGIN || !/^\/en(?:\/|$)/.test(url.pathname), `${path}: avoid internal links through English redirects`);
    }
  }
}
assert.deepEqual(locations.sort(), expectedLocations.sort(), "Sitemap must contain exactly the canonical pages");
assert.equal(new Set(locations).size, expectedLocations.length, "Sitemap duplicates");
console.log(`Verified ${expectedLocations.length} prerendered pages: canonical, languages, indexability, structured-data syntax and sitemap`);
