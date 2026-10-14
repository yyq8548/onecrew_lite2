import { describe, expect, it } from "vitest";
import type { DiscoveryHit } from "@/lib/opencli-discovery";
import { discoveryHitsCsv } from "./library-csv-export";

describe("old Library search-result interchange", () => {
  it("keeps provenance and neutralizes CSV formulas without claiming qualification", () => {
    const hit: DiscoveryHit = {
      id: "hit-1", platform: "twitter", source_url: "https://x.com/i/status/123",
      title: '=HYPERLINK("https://evil.example")', author_label: "=suspicious",
      search_keyword: "AI, tools", captured_at: 1_728_000_000, profile_ref: "dedicated",
      author_id: null, author_url: null, author_observed_at: null,
    };
    const csv = discoveryHitsCsv("campaign-1", [hit]);
    expect(csv).toContain('"campaign-1","hit-1","twitter"');
    expect(csv).toContain('"\'=suspicious"');
    expect(csv).toContain('"\'=HYPERLINK(""https://evil.example"")"');
    expect(csv).toContain('"AI, tools","Unverified search lead"');
    expect(csv).not.toContain("Qualified");
    expect(csv.startsWith("\uFEFF")).toBe(true);
  });
});
