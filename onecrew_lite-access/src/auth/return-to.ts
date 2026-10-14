// Shared browser/Worker allowlist. Never accept arbitrary paths or query fields.
const accountPanels = new Set(["overview", "access", "profile", "security", "email", "devices"]);
export function safeAccountReturnTo(value: string | null | undefined): string | null {
  if (value === "/account") return value;
  if (!value?.startsWith("/account?") || value.includes("#")) return null;
  const query = new URLSearchParams(value.slice("/account?".length));
  if (!query.size) return null;
  const seen = new Set<string>();
  for (const [key, item] of query) {
    if (seen.has(key)) return null;
    seen.add(key);
    if (key === "panel" ? !accountPanels.has(item)
      : key === "apply" ? item !== "1"
      : key === "lng" ? !["zh-CN", "en-US"].includes(item)
      : true) return null;
  }
  return `/account?${query.toString()}`;
}

export function safeAuthReturnTo(value: string | null | undefined): string | null {
  return safeAccountReturnTo(value)
    ?? (value && /^\/d\/[A-Za-z0-9_-]{43}$/.test(value) ? value : null);
}
