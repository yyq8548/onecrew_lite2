import { useLandingLocale } from "./language-switcher";

function requiredHttpsOrigin(value: string | undefined, name: string) {
  if (!value) throw new Error(`${name} must be configured at build time`);
  const url = new URL(value);
  if (
    url.protocol !== "https:" ||
    url.pathname !== "/" ||
    url.search !== "" ||
    url.hash !== "" ||
    url.username !== "" ||
    url.password !== ""
  ) {
    throw new Error(`${name} must be an HTTPS origin without a path`);
  }
  return url.origin;
}

export const ACCESS_SITE_URL = requiredHttpsOrigin(
  import.meta.env.VITE_ACCESS_URL,
  "VITE_ACCESS_URL",
);

export function useAccessLoginUrl() {
  const locale = useLandingLocale();
  const url = new URL("/account", ACCESS_SITE_URL);
  url.searchParams.set("apply", "1");
  url.searchParams.set("lng", locale === "en" ? "en-US" : "zh-CN");
  return url.toString();
}
