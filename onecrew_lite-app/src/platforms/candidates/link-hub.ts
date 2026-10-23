import { load } from "cheerio";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { candidateHubHosts } from "../../creators/candidate-extraction.ts";
import { record } from "./projection.ts";

const hosts = new Set(candidateHubHosts);
export class LinkHubReadError extends Error { readonly code: string; constructor(code: string) { super(`link_hub_${code}`); this.code = `link_hub_${({ too_many_redirects: "redirects_exceeded", invalid_utf8: "body_not_utf8" } as Record<string, string>)[code] ?? code}`; } }
export interface HeldLinkHubPage { source_url: string; links: { url: string; text: string | null }[]; emails: { value: string }[] }
export type LinkHubLayer = "anchor" | "json_ld" | "hydration";
export interface LinkHubExtraction { source_url: string; links: { layer: LinkHubLayer; url: string; text: string | null }[]; emails: { layer: LinkHubLayer; value: string }[] }
export function linkHubUrl(value: string): URL {
  const trimmed = value.trim().replace(/[.,;:!?)\]}'">，。；：！？、）】》〉」』…·“”‘’]+$/u, ""); const url = URL.parse(trimmed);
  if (!url || !["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new LinkHubReadError("url_invalid");
  if (!hosts.has(url.hostname.replace(/^www\./u, "").toLowerCase())) throw new LinkHubReadError("host_unsupported");
  return url;
}
export function parseLinkHubExtraction(html: string, initial: string, final: string): LinkHubExtraction {
  const source = linkHubUrl(initial); const base = linkHubUrl(final); const baseHost = base.hostname.replace(/^www\./u, "").toLowerCase();
  const $ = load(html); const links: LinkHubExtraction["links"] = []; const emails: LinkHubExtraction["emails"] = []; const seenLinks = new Set<string>(); const seenEmails = new Set<string>();
  const email = (text: string, layer: LinkHubLayer = "anchor") => { for (const match of text.matchAll(/\b[A-Z\d._%+-]+@[A-Z\d.-]+\.[A-Z]{2,}\b/giu)) { const value = match[0].toLowerCase(); if (!seenEmails.has(value)) { seenEmails.add(value); emails.push({ layer, value }); } } };
  const push = (value: string, title: string | null, layer: LinkHubLayer = "anchor") => {
    if (value.startsWith("mailto:")) { email(value.slice(7), layer); return; }
    const url = URL.parse(value); if (!url || !["http:", "https:"].includes(url.protocol) || url.username || url.password || !url.hostname || url.hostname.replace(/^www\./u, "").toLowerCase() === baseHost || seenLinks.has(url.href.toLowerCase())) return;
    seenLinks.add(url.href.toLowerCase()); links.push({ layer, url: url.href, text: title?.split(/\s+/u).filter(Boolean).join(" ") || null });
  };
  $("a[href]").each((_index, element) => {
    const anchor = $(element); if (anchor.parents("[data-seo-modal-panel]").length) return;
    const href = anchor.attr("href")!.trim(); if (href.startsWith("mailto:")) { email(href.slice(7)); return; }
    const url = URL.parse(href, base); if (!url || url.hostname.replace(/^www\./u, "").toLowerCase() === baseHost) return;
    const title = anchor.text(); email(title); push(url.href, title);
  });
  const sameAs = (value: unknown, depth = 0) => {
    if (depth > 64) throw new LinkHubReadError("structured_data_too_deep");
    if (Array.isArray(value)) { for (const member of value) sameAs(member, depth + 1); }
    else if (value && typeof value === "object") for (const [key, member] of Object.entries(value)) { if (key === "sameAs") { if (Array.isArray(member)) for (const url of member) if (typeof url === "string") push(url, null, "json_ld"); } else sameAs(member, depth + 1); }
  };
  $("script[type='application/ld+json']").each((_index, element) => sameAs(parseStoredJson($(element).text())));
  $("script#__NEXT_DATA__").each((_index, element) => {
    const root = record(parseStoredJson($(element).text())); if (!root.props || typeof root.props !== "object") return;
    const props = record(root.props).pageProps; if (!props || typeof props !== "object") return;
    const page = record(props);
    if (Array.isArray(page.socialLinks)) for (const item of page.socialLinks) { const row = record(item); if (typeof row.url === "string") push(row.url, null, "hydration"); }
    if (Array.isArray(page.links)) for (const item of page.links) { const row = record(item); const title = typeof row.title === "string" ? row.title : null; if (title !== null) email(title, "hydration"); if (typeof row.url === "string" && row.url) push(row.url, title, "hydration"); }
  });
  return { source_url: source.href, links, emails };
}
function held(page: LinkHubExtraction): HeldLinkHubPage { return { source_url: page.source_url, links: page.links.map(({ url, text }) => ({ url, text })), emails: page.emails.map(({ value }) => ({ value })) }; }
export function parseHeldLinkHub(html: string, initial: string, final: string): HeldLinkHubPage { return held(parseLinkHubExtraction(html, initial, final)); }
export async function readHeldLinkHub(value: string, signal: AbortSignal): Promise<HeldLinkHubPage> { return held(await readLinkHubExtraction(value, signal)); }
export async function readLinkHubExtraction(value: string, signal: AbortSignal): Promise<LinkHubExtraction> {
  const initial = linkHubUrl(value); let current = initial; const scoped = AbortSignal.any([signal, AbortSignal.timeout(15000)]);
  for (let redirects = 0; ; redirects++) {
    linkHubUrl(current.href); scoped.throwIfAborted();
    const response = await fetch(current, { redirect: "manual", credentials: "omit", headers: { "user-agent": "Mozilla/5.0 (compatible; ReacherContactDiscovery/1.0)" }, signal: scoped }).then(value => value, () => { signal.throwIfAborted(); throw new LinkHubReadError(scoped.aborted ? "request_timed_out" : "request_failed"); });
    if ([301, 302, 303, 307, 308].includes(response.status)) {
      await response.body?.cancel(); if (redirects === 5) throw new LinkHubReadError("too_many_redirects");
      const location = response.headers.get("location"); const next = location === null ? null : URL.parse(location, current);
      if (!next) throw new LinkHubReadError("redirect_invalid"); current = linkHubUrl(next.href); continue;
    }
    if (!response.ok || !response.body) throw new LinkHubReadError("request_failed");
    const length = response.headers.get("content-length"); if (length !== null && /^[0-9]+$/u.test(length) && BigInt(length) > 4n * 1024n * 1024n) { await response.body.cancel(); throw new LinkHubReadError("body_too_large"); }
    const reader = response.body.getReader(); const chunks: Uint8Array[] = []; let size = 0;
    try { for (;;) { const next = await reader.read().then(value => value, () => { signal.throwIfAborted(); throw new LinkHubReadError(scoped.aborted ? "request_timed_out" : "request_failed"); }); if (next.done) break; size += next.value.length; if (size > 4 * 1024 * 1024) throw new LinkHubReadError("body_too_large"); chunks.push(next.value); } } finally { await reader.cancel(); reader.releaseLock(); }
    const html = await Promise.resolve().then(() => new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks))).then(value => value, () => { throw new LinkHubReadError("invalid_utf8"); });
    return parseLinkHubExtraction(html, initial.href, current.href);
  }
}
