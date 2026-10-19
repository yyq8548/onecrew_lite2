import { describe, expect, it } from "vitest";
import { creatorBatchSelections, initialCreatorBatchDraft, rebaseCreatorBatchDraft, setCreatorBatchRoute, setCreatorBatchSelected } from "./creator-batch-state";
import { CREATOR_SAVE_PREVIEW } from "@mcp-ui/test-support/creator-save-fixture";

const rows = () => Array.from({ length: 5 }, (_, index) => ({ row_id: `row-${index}`, preview: { ...structuredClone(CREATOR_SAVE_PREVIEW), routes: CREATOR_SAVE_PREVIEW.routes.map(route => ({...structuredClone(route),collision:{status:"none" as const,creator_ids:[]}})), } }));

describe("Creator batch selection", () => {
  it("submits five distinct rows in one ordered selection with all eligible routes", () => {
    const review = rows();
    const selected = creatorBatchSelections(review, initialCreatorBatchDraft(review));
    expect(selected).toHaveLength(5);
    expect(selected.map(row => row.row_id)).toEqual(review.map(row => row.row_id));
    expect(selected.every(row => row.confirmed_route_ids.length === 3)).toBe(true);
  });
  // A creator's promo block is inherited by every upload, so a description
  // hands back sponsor, affiliate and forum URLs next to the real contacts.
  // The card checks what reaches them and withholds only a bare URL whose
  // every source is prose — the route stays listed and one click away.
  it("checks the contacts that reach a creator, not every URL a description mentioned", () => {
    const promo = (id: string, sources: string[]) => ({
      route_id: id, channel: "website", kind: "url", display_value: `https://${id}.example.test`,
      normalized_value: `https://${id}.example.test/`, sources, required: false,
      collision: { status: "none" as const, creator_ids: [] },
    });
    const preview = {
      ...structuredClone(CREATOR_SAVE_PREVIEW),
      routes: [
        { ...structuredClone(CREATOR_SAVE_PREVIEW.routes[0]!), sources: ["initial_platform_account"] },
        { ...structuredClone(CREATOR_SAVE_PREVIEW.routes[1]!), sources: ["profile_bio"], collision: { status: "none" as const, creator_ids: [] } },
        promo("official", ["profile_website"]),
        promo("sponsor", ["video_description"]),
        promo("forum", ["video_description", "channel_description"]),
        promo("unknown-provenance", []),
      ],
    };
    const review = [{ row_id: "row-0", preview }];
    const [selection] = creatorBatchSelections(review, initialCreatorBatchDraft(review));
    expect(selection!.confirmed_route_ids).toEqual([
      // required, kept whatever its source
      "route-profile",
      // an address reaches the creator
      "route-email",
      // a site the creator curated about themselves
      "official",
      // no provenance is not evidence of prose
      "unknown-provenance",
    ]);
    expect(selection!.confirmed_route_ids).not.toContain("sponsor");
    expect(selection!.confirmed_route_ids).not.toContain("forum");
  });
  it("keeps optional routes independent per row across fresh objects and row deselection", () => {
    const review = rows(); let draft = { ...initialCreatorBatchDraft(review), optionalIds: {} };
    const optional = review[0]!.preview.routes.find(route => !route.required)!;
    draft = setCreatorBatchRoute(draft, "row-0", optional.route_id, true, review);
    draft = setCreatorBatchSelected(draft, "row-0", false, structuredClone(review));
    expect(creatorBatchSelections(review, draft)).toHaveLength(4);
    draft = setCreatorBatchSelected(draft, "row-0", true, structuredClone(review));
    const selected = creatorBatchSelections(review, draft);
    expect(selected[0]!.confirmed_route_ids).toContain(optional.route_id);
    expect(selected[1]!.confirmed_route_ids).not.toContain(optional.route_id);
  });
  it("does not remove required routes and permits zero selected rows without fabricating a save", () => {
    const review = rows(); let draft = { ...initialCreatorBatchDraft(review), optionalIds: {} };
    const required = review[0]!.preview.routes.find(route => route.required)!;
    draft = setCreatorBatchRoute(draft, "row-0", required.route_id, false, review);
    expect(creatorBatchSelections(review, draft)[0]!.confirmed_route_ids).toContain(required.route_id);
    for (const row of review) draft = setCreatorBatchSelected(draft, row.row_id, false, review);
    expect(creatorBatchSelections(review, draft)).toEqual([]);
  });
  it("treats prototype-like row IDs as data and does not inherit optional routes", () => {
    const review = rows(); review[0]!.row_id = "__proto__";
    let draft = { ...initialCreatorBatchDraft(review), optionalIds: {} };
    const optional = review[1]!.preview.routes.find(route => !route.required)!;
    draft = setCreatorBatchRoute(draft, "row-1", optional.route_id, true, review);
    expect(creatorBatchSelections(review, draft)[0]!.confirmed_route_ids).toHaveLength(1);
    draft = setCreatorBatchRoute(draft, "__proto__", optional.route_id, true, review);
    expect(creatorBatchSelections(review, draft)[0]!.confirmed_route_ids).toHaveLength(2);
  });
  it("rebases only known row/route selections after an explicit revision acknowledgement", () => {
    const review = rows(); let draft = { ...initialCreatorBatchDraft(review), optionalIds: {} };
    const optional = review[0]!.preview.routes.find(route => !route.required)!;
    draft = setCreatorBatchRoute(draft, "row-0", optional.route_id, true, review);
    const newer = structuredClone(review).slice(0, 4);
    newer[0]!.preview.routes = newer[0]!.preview.routes.filter(route => route.required);
    const rebased = rebaseCreatorBatchDraft(newer, draft);
    expect(rebased.selectedIds).toHaveLength(4); expect(rebased.optionalIds["row-0"]).toEqual([]);
    expect(setCreatorBatchSelected(rebased, "forged", true, newer)).toBe(rebased);
  });
});

it("default batch save excludes conflicting contacts",()=>{
 const review=[{row_id:"row-1",preview:structuredClone(CREATOR_SAVE_PREVIEW)}];
 expect(creatorBatchSelections(review,initialCreatorBatchDraft(review))[0]!.confirmed_route_ids).toEqual(["route-profile","route-site"]);
});
