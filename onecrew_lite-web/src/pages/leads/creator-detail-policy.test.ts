import { describe, expect, it } from "vitest";

import { creatorDetailActionsEnabled } from "./creator-detail-policy";

describe("Creator detail policy", () => {
  it("makes archived Creator details strictly read-only", () => {
    expect(creatorDetailActionsEnabled(true)).toBe(false);
    expect(creatorDetailActionsEnabled(false)).toBe(true);
  });
});
