import { load } from "cheerio";
import { marked } from "marked";
import { z } from "zod";
import { publicWebUrl } from "../platforms/opencli/public-url.ts";
import type { PublicOpenCliWebRead } from "../platforms/opencli/web.ts";
import { discoveryWebSourceTarget } from "./web-source-contracts.ts";

const text = z.string().refine(value => value.isWellFormed());
const httpsUrl = text.max(2048).refine(value => publicWebUrl(value) !== null, "web_author_url_invalid");
export const discoveryWebAuthorResolution = z.strictObject({
  author_name: text.max(512),
  attribution_quote: text.max(2000),
  author_page_url: httpsUrl.nullable(),
  native_source_url: httpsUrl.nullable(),
  reason: text.max(1000).refine(value => value.trim().length > 0, "web_author_reason_required"),
});
export type DiscoveryWebAuthorResolution = z.infer<typeof discoveryWebAuthorResolution>;
export const discoveryWebAuthorEvidenceFailure = z.enum(["web_author_quote_invalid", "web_author_link_unobserved", "web_author_page_invalid", "web_author_identity_conflict", "web_author_native_source_invalid", "web_author_native_attribution_invalid"]);

const normalizedName = (value: string) => value.normalize("NFKC").trim().replace(/\s+/gu, " ").toLowerCase();
interface ObservedLink { url: URL; label: string; author: boolean }
function observedLinks(page: PublicOpenCliWebRead): { document: URL; links: ObservedLink[] } {
  const document = publicWebUrl(page.document_url);
  if (!document || !page.markdown_excerpt.isWellFormed()) throw new Error("web_author_page_invalid");
  const $ = load(marked.parse(page.markdown_excerpt, { async: false }), undefined, false), links: ObservedLink[] = [];
  $("a[href]").each((_index, element) => {
    const anchor = $(element), resolved = URL.parse(anchor.attr("href")!, document);
    const url = resolved === null ? null : publicWebUrl(resolved.href);
    if (url) links.push({ url, label: anchor.text(), author: (anchor.attr("rel") ?? "").split(/\s+/u).includes("author") });
  });
  return { document, links };
}

export function validateDiscoveryWebAuthorResolution(page: PublicOpenCliWebRead, raw: unknown, expectedAuthor?: string): DiscoveryWebAuthorResolution {
  const resolution = discoveryWebAuthorResolution.parse(raw), name = normalizedName(resolution.author_name);
  const hasAttribution = resolution.author_name !== "" || resolution.attribution_quote !== "" || resolution.author_page_url !== null || resolution.native_source_url !== null;
  if (!hasAttribution) return resolution;
  if (!name || !resolution.attribution_quote.trim() || !page.markdown_excerpt.includes(resolution.attribution_quote) || !normalizedName(resolution.attribution_quote).includes(name)) throw new Error("web_author_quote_invalid");
  if (expectedAuthor !== undefined && (!normalizedName(expectedAuthor) || normalizedName(expectedAuthor) !== name)) throw new Error("web_author_identity_conflict");
  const { document, links } = observedLinks(page);
  const linked = (value: string): ObservedLink => {
    const url = publicWebUrl(value)!, observed = links.find(link => link.url.href === url.href);
    if (!observed) throw new Error("web_author_link_unobserved");
    return observed;
  };
  if (resolution.author_page_url !== null) {
    const link = linked(resolution.author_page_url);
    const label = publicWebUrl(link.label.trim()) === null ? link.label : "";
    const introduction = link.author || normalizedName(label).includes(name)
      || /\b(?:author|contributor|writer|biography|bio|profile)\b|作者|撰稿人|关于作者|關於作者/iu.test(label)
      || /\/(?:authors?|contributors?|writers?|staff|team|people|profiles?)\/[^/]+/iu.test(link.url.pathname);
    if (link.url.hostname !== document.hostname || !introduction) throw new Error("web_author_page_invalid");
  }
  if (resolution.native_source_url !== null) {
    const link = linked(resolution.native_source_url);
    if (discoveryWebSourceTarget(link.url.href) === null) throw new Error("web_author_native_source_invalid");
    const label = publicWebUrl(link.label.trim()) === null ? normalizedName(link.label) : "";
    if (!label.includes(name)) {
      const quote = resolution.attribution_quote;
      const quoted = (quote.includes(resolution.native_source_url) || quote.includes(link.url.href))
        && observedLinks({ ...page, markdown_excerpt: quote }).links.some(observed => observed.url.href === link.url.href);
      if (!quoted) throw new Error("web_author_native_attribution_invalid");
    }
  }
  return resolution;
}
