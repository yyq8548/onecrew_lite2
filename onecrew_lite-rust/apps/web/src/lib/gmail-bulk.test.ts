import { describe, expect, it } from "vitest";
import { uniqueRecipients } from "./gmail-bulk";

describe("Gmail bulk selection", () => {
  it("deduplicates addresses without discarding source evidence", () => {
    expect(uniqueRecipients([{email:" A@example.com ",source:"first"},{email:"a@example.com",source:"second"}])).toEqual([{email:"a@example.com",source:"first\nsecond"}]);
  });
});
