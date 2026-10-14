import { readFile } from "node:fs/promises";

const configUrl = new URL("../site/wrangler.toml", import.meta.url);
const config = await readFile(configUrl, "utf8");

function requiredOrigin(name) {
  const match = config.match(new RegExp(`^${name}\\s*=\\s*"([^"]+)"\\s*$`, "m"));
  if (!match) throw new Error(`${name} is missing from site/wrangler.toml`);
  const value = match[1];
  const url = new URL(value);
  if (url.protocol !== "https:" || url.origin !== value || url.pathname !== "/" || url.port !== "") {
    throw new Error(`${name} must be an HTTPS origin without a path`);
  }
  return url.origin;
}

export const SITE_ORIGIN = requiredOrigin("SITE_ORIGIN");
export const WWW_ORIGIN = requiredOrigin("WWW_ORIGIN");
export const ACCESS_ORIGIN = requiredOrigin("ACCESS_ORIGIN");
export const BLOG_ORIGIN = requiredOrigin("BLOG_ORIGIN");

const routeHosts = [...config.matchAll(/\bpattern\s*=\s*"([^"]+)"/g)].map((match) => match[1]);
for (const origin of [SITE_ORIGIN, WWW_ORIGIN]) {
  if (!routeHosts.includes(new URL(origin).hostname)) {
    throw new Error("Every Site origin must have a matching custom-domain route");
  }
}
if (new Set([SITE_ORIGIN, WWW_ORIGIN, ACCESS_ORIGIN, BLOG_ORIGIN]).size !== 4) {
  throw new Error("Site, www, Access, and Blog origins must remain isolated");
}
if (new URL(WWW_ORIGIN).hostname !== `www.${new URL(SITE_ORIGIN).hostname}`) {
  throw new Error("WWW_ORIGIN must be the www host for SITE_ORIGIN");
}
