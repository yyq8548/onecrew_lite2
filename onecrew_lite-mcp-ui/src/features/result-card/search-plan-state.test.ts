import { describe, expect, it } from "vitest";
import { rebaseSearchPlanEdit, searchPlanEditPatch, searchPlanEditStale, searchPlanFieldsError, startSearchPlanEdit } from "@mcp-ui/features/result-card/search-plan-state";
import { INSTAGRAM_SCOPE, SEARCH_PLAN, YOUTUBE_SCOPE, searchPlanSnapshot } from "@mcp-ui/test-support/search-plan-fixture";
import { BINDING } from "@mcp-ui/test-support/fixtures";

const TAB = String.fromCharCode(9);

describe("search plan local draft", () => {
  it("starts clean and derives only the fields the user changed", () => {
    const snapshot = searchPlanSnapshot();
    const edit = startSearchPlanEdit(SEARCH_PLAN, snapshot.binding, "digest-2");
    expect(searchPlanEditPatch(edit)).toEqual({});
    const changed = { ...edit, fields: { ...edit.fields, keywords: ["RWA México"] } };
    expect(searchPlanEditPatch(changed)).toEqual({ keywords: ["RWA México"] });
    expect(SEARCH_PLAN.keywords).toEqual(["cooking"]);
  });

  it("does not overwrite a local draft on conflict and rebases only explicit edits", () => {
    const original = searchPlanSnapshot();
    const edit = startSearchPlanEdit(SEARCH_PLAN, original.binding, "digest-2");
    const changed = { ...edit, fields: { ...edit.fields, keywords: ["baking"] } };
    const server = { ...SEARCH_PLAN, description: "Server changed the criteria" };
    const current = searchPlanSnapshot(server, 3);
    expect(searchPlanEditStale(changed, current.binding, "digest-3")).toBe(true);
    expect(changed.fields.description).toBe(SEARCH_PLAN.description);
    const rebased = rebaseSearchPlanEdit(changed, server, current.binding, "digest-3");
    expect(rebased.fields).toEqual({ description: server.description, keywords: ["baking"] });
    expect(searchPlanEditPatch(rebased)).toEqual({ keywords: ["baking"] });
    expect(searchPlanEditStale(rebased, current.binding, "digest-3")).toBe(false);
  });

  it("enforces the validate_typed description and keyword rules without any scope input", () => {
    expect(searchPlanFieldsError({ description: "criteria", keywords: ["RWA México"] })).toBeNull();
    expect(searchPlanFieldsError({ description: "criteria", keywords: [] })).toBeNull();
    expect(searchPlanFieldsError({ description: "criteria", keywords: ["x".repeat(101)] })).toBeNull();
    expect(searchPlanFieldsError({ description: "  ", keywords: [] })).toBe("description");
    expect(searchPlanFieldsError({ description: "汉".repeat(3000), keywords: [] })).toBe("description");
    expect(searchPlanFieldsError({ description: "criteria", keywords: ["汉".repeat(171)] })).toBe("keywords");
    expect(searchPlanFieldsError({ description: "criteria", keywords: [`x${TAB}y`] })).toBe("keywords");
    expect(searchPlanFieldsError({ description: "criteria", keywords: [" cooking"] })).toBe("keywords");
    expect(searchPlanFieldsError({ description: "criteria", keywords: ["cooking", "cooking"] })).toBe("keywords");
    expect(searchPlanFieldsError({ description: "criteria", keywords: Array.from({ length: 51 }, (_, index) => `k${index}`) })).toBe("keywords");
  });

  it("never derives a patch field from the read-only scopes", () => {
    const edit = startSearchPlanEdit({ ...SEARCH_PLAN, scopes: [INSTAGRAM_SCOPE, YOUTUBE_SCOPE] }, BINDING, "digest");
    expect(Object.keys(edit.fields).sort()).toEqual(["description", "keywords"]);
    expect(Object.keys(searchPlanEditPatch({ ...edit, fields: { description: "changed", keywords: ["a"] } })).sort()).toEqual(["description", "keywords"]);
  });
});
