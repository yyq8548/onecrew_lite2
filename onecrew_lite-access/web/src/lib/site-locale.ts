export type SiteLocale = "en" | "zh";

export function localeFromPathname(pathname: string): SiteLocale {
  return pathname === "/zh" || pathname.startsWith("/zh/") ? "zh" : "en";
}

export function stripLocalePrefix(pathname: string) {
  if (pathname === "/zh") return "/";
  if (pathname.startsWith("/zh/")) return pathname.slice(3);
  if (pathname === "/en") return "/";
  if (pathname.startsWith("/en/")) return pathname.slice(3);
  return pathname;
}

export function localizedPath(path: string, locale: SiteLocale) {
  if (!path.startsWith("/")) return path;
  const match = /^([^?#]*)(.*)$/.exec(path);
  const pathname = stripLocalePrefix(match?.[1] || "/");
  const suffix = match?.[2] || "";
  if (locale === "en") return `${pathname}${suffix}`;
  return `${pathname === "/" ? "/zh" : `/zh${pathname}`}${suffix}`;
}
