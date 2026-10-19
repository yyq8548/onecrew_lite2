import { DISCOVERY_PAGE_SIZE, listDiscoveryHits, type DiscoveryHit } from "@/lib/opencli-discovery";

// The old Library imports CSV into a real, editable Teable table. Keep this
// interchange limited to observed search hits; it never asserts qualification.
const HEADERS = ["Campaign ID", "Search hit ID", "Platform", "Author label", "Content title", "Source URL", "Search keyword", "Status", "Captured at"];
const MAX_LIBRARY_ROWS = 10_000;

const csvCell = (value: string): string => {
  // Neutralize spreadsheet formulas, including values with leading whitespace.
  const safe = /^[\s\uFEFF]*[=+\-@]/u.test(value) ? `'${value}` : value;
  return `"${safe.replaceAll('"', '""')}"`;
};

export function discoveryHitsCsv(campaignId: string, hits: readonly DiscoveryHit[]): string {
  const rows = hits.map(hit => [
    campaignId, hit.id, hit.platform, hit.author_label, hit.title, hit.source_url,
    hit.search_keyword, "Unverified search lead", new Date(hit.captured_at * 1000).toISOString(),
  ]);
  return `\uFEFF${[HEADERS, ...rows].map(row => row.map(csvCell).join(",")).join("\r\n")}\r\n`;
}

/** Read all local Campaign pages; this does not issue any platform search. */
export async function loadAllDiscoveryHits(campaignId: string): Promise<DiscoveryHit[]> {
  const hits: DiscoveryHit[] = [];
  let afterId: string | null = null;
  for (;;) {
    const page = await listDiscoveryHits(campaignId, afterId);
    if (hits.length + page.length > MAX_LIBRARY_ROWS) throw new Error("Library row limit exceeded");
    hits.push(...page);
    if (page.length < DISCOVERY_PAGE_SIZE) return hits;
    const next = page.at(-1)?.id;
    if (!next || next === afterId) throw new Error("Discovery pagination did not advance");
    afterId = next;
  }
}
