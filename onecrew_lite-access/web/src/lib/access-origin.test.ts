import { describe, expect, it } from "vitest";

import { accessApiURL, accessOrigin } from "./access-origin";

describe("Access same-origin boundary", () => {
  it("never constructs a cross-origin API URL", () => {
    expect(accessOrigin).toBe("");
    expect(accessApiURL("/api/session")).toBe("/api/session");
  });
});
