import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { SITE_ORIGIN } from "./site-deployment-config.mjs";
import { artifactForPath } from "./site-csp.mjs";
import { renderGa4Init } from "./ga4-template.mjs";

const root = new URL("../", import.meta.url);
const output = new URL("site/build/client/", root);
const originPlaceholder = "__ONECREW_SITE_ORIGIN__";
const sitemapPlaceholder = "__ONECREW_SITEMAP_URLS__";
const pages = JSON.parse(await readFile(new URL("site/marketing-pages.json", root), "utf8"));
const contact = JSON.parse(await readFile(new URL("site/contact.json", root), "utf8"));
await writeFile(new URL("ga4-init.js", output), renderGa4Init(await readFile(new URL("web/public/ga4-init.js", root), "utf8")), "utf8");
// These URLs begin serving English content at this cutover, not on every rebuild.
const englishUrlCutover = "2026-09-09";

for (const file of ["robots.txt", "llms.txt"]) {
  const template = await readFile(new URL(`web/public/${file}`, root), "utf8");
  if (!template.includes(originPlaceholder)) {
    throw new Error(`${file} does not contain the site-origin placeholder`);
  }
  const rendered = template.replaceAll(originPlaceholder, SITE_ORIGIN).replaceAll("__ONECREW_CONTACT_EMAIL__", contact.email);
  await writeFile(new URL(file, output), rendered, "utf8");
}

function localizedPath(path, locale) {
  if (locale === "en") return path;
  return path === "/" ? "/zh" : `/zh${path}`;
}

const entries = pages.flatMap((page) => ["zh", "en"].map((locale) => {
  const path = localizedPath(page.path, locale);
  const chineseUrl = `${SITE_ORIGIN}${localizedPath(page.path, "zh")}`;
  const englishUrl = `${SITE_ORIGIN}${localizedPath(page.path, "en")}`;
  return [
    "  <url>",
    `    <loc>${SITE_ORIGIN}${path}</loc>`,
    `    <xhtml:link rel="alternate" hreflang="zh-CN" href="${chineseUrl}" />`,
    `    <xhtml:link rel="alternate" hreflang="en" href="${englishUrl}" />`,
    `    <xhtml:link rel="alternate" hreflang="x-default" href="${englishUrl}" />`,
    `    <lastmod>${locale === "en" ? [page.lastModified ?? "2026-09-05", englishUrlCutover].sort().at(-1) : page.lastModified ?? "2026-09-05"}</lastmod>`,
    `    <changefreq>${page.changeFrequency}</changefreq>`,
    `    <priority>${page.priority}</priority>`,
    "  </url>",
  ].join("\n");
}));

const sitemapTemplate = await readFile(new URL("web/public/sitemap.xml", root), "utf8");
if (!sitemapTemplate.includes(sitemapPlaceholder)) {
  throw new Error("sitemap.xml does not contain the sitemap entry placeholder");
}
const sitemap = sitemapTemplate.replace(sitemapPlaceholder, entries.join("\n"));
await writeFile(new URL("sitemap.xml", output), sitemap, "utf8");

for (const page of pages) {
  for (const locale of ["zh", "en"]) {
    const path = localizedPath(page.path, locale);
    const htmlUrl = new URL(artifactForPath(path), output);
    const html = await readFile(htmlUrl, "utf8");
    await writeFile(htmlUrl, html.replaceAll("hrefLang=", "hreflang="), "utf8");
  }
}

console.log(`Rendered ${entries.length} localized URLs and static metadata in ${fileURLToPath(output)}`);
