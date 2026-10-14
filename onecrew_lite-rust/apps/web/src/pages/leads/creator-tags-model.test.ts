import { describe, expect, it } from "vitest";
import { creatorTagsFromDraft, validateCreatorTags } from "./creator-tags-model";
import { manualCreatorInput } from "./manual-entry-model";

describe("Creator tag form constraints", () => {
  it("counts Unicode scalars rather than UTF-16 units, including emoji and combining text", () => {
    const emoji = "😀".repeat(12);
    expect(validateCreatorTags([emoji])).toEqual({ tags: [emoji], error: null });
    expect(validateCreatorTags(["😀".repeat(13)]).error).toBe("tags.validation.length");
    expect(validateCreatorTags(["e\u0301".repeat(6)]).error).toBeNull();
    expect(validateCreatorTags(["e\u0301".repeat(7)]).error).toBe("tags.validation.length");
  });

  it("trims Rust Unicode whitespace while preserving order, case, and BOM characters", () => {
    expect(validateCreatorTags(["\u00a0艺术\u3000", "AI", "ai"])).toEqual({
      tags: ["艺术", "AI", "ai"], error: null,
    });
    expect(validateCreatorTags(["\ufeffAI\ufeff"])).toEqual({ tags: ["\ufeffAI\ufeff"], error: null });
  });

  it("rejects blank, duplicate, control, invalid scalar, and excessive labels without truncation", () => {
    expect(validateCreatorTags(["  "]).error).toBe("tags.validation.length");
    expect(validateCreatorTags(["AI", " AI "]).error).toBe("tags.validation.duplicate");
    for (const tag of ["\nAI", "AI\t", "\u0085AI", "AI\u007f", "\ud800"]) {
      expect(validateCreatorTags([tag]).error).toBe("tags.validation.characters");
    }
    expect(validateCreatorTags(["a", "b", "c", "d"]).error).toBe("tags.validation.tooMany");
    expect(validateCreatorTags([])).toEqual({ tags: [], error: null });
  });

  it("includes a pending label on explicit submission and preserves a failed draft for correction", () => {
    const draft = { tags: ["AI"], text: "  Art  " };
    expect(creatorTagsFromDraft(draft)).toEqual({ tags: ["AI", "Art"], error: null });
    expect(draft).toEqual({ tags: ["AI"], text: "  Art  " });
    const invalid = { tags: ["AI"], text: " AI " };
    expect(creatorTagsFromDraft(invalid).error).toBe("tags.validation.duplicate");
    expect(invalid.text).toBe(" AI ");
  });

  it("carries normalized selected and pending labels through manual creation, and blocks invalid submission", () => {
    expect(manualCreatorInput({ displayName: "Ada", notes: "", tags: { tags: ["Art"], text: " AI " } })).toEqual({
      input: { display_name: "Ada", notes: null, tags: ["Art", "AI"], contact_routes: [] }, error: null,
    });
    expect(manualCreatorInput({ displayName: "Ada", notes: "", tags: { tags: ["AI"], text: "AI" } }).input).toBeNull();
  });
});
