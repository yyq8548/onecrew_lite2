const configuredSiteUrl = import.meta.env.VITE_SITE_URL;

if (!configuredSiteUrl) {
  throw new Error("VITE_SITE_URL must be configured at build time");
}

const parsedSiteUrl = new URL(configuredSiteUrl);

if (parsedSiteUrl.protocol !== "https:" || parsedSiteUrl.pathname !== "/") {
  throw new Error("VITE_SITE_URL must be an HTTPS origin without a path");
}

export const SITE_URL = parsedSiteUrl.origin;

export function absoluteUrl(path = "/") {
  return new URL(path, SITE_URL).toString();
}
