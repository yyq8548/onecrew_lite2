export async function openExternalUrl(url: string) {
  const normalized = url.trim();
  if (!normalized) return;

  const parsed = new URL(normalized);
  if (!["http:", "https:", "mailto:", "tg:"].includes(parsed.protocol)) {
    throw new Error("unsupported_external_url");
  }
  window.open(normalized, "_blank", "noopener,noreferrer");
}
