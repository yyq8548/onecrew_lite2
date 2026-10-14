import { describe, expect, it } from "vitest";

import {
  localDatesToTimestampRange,
  readTimestampDateRange,
  timestampMatchesDateRange,
} from "@/lib/date-range-filter";

function epochSeconds(date: Date) {
  return Math.floor(date.getTime() / 1000);
}

describe("local date-range filtering", () => {
  it("includes a complete selected local day", () => {
    const day = new Date(2026, 7, 19, 12);
    const range = localDatesToTimestampRange(day);

    expect(timestampMatchesDateRange(epochSeconds(new Date(2026, 7, 19, 0, 0, 0)), range)).toBe(true);
    expect(timestampMatchesDateRange(epochSeconds(new Date(2026, 7, 19, 23, 59, 59)), range)).toBe(true);
    expect(timestampMatchesDateRange(epochSeconds(new Date(2026, 7, 20, 0, 0, 0)), range)).toBe(false);
    expect(timestampMatchesDateRange(0, range)).toBe(false);
  });

  it("uses an inclusive end day and an exclusive next-day boundary", () => {
    const range = localDatesToTimestampRange(
      new Date(2026, 7, 19),
      new Date(2026, 7, 21),
    );

    expect(timestampMatchesDateRange(epochSeconds(new Date(2026, 7, 21, 23, 59, 59)), range)).toBe(true);
    expect(timestampMatchesDateRange(epochSeconds(new Date(2026, 7, 22, 0, 0, 0)), range)).toBe(false);
  });

  it("treats malformed persisted values as no filter", () => {
    expect(readTimestampDateRange({ from: 10, toExclusive: 10 })).toBeNull();
    expect(readTimestampDateRange({ from: "10", toExclusive: 20 })).toBeNull();
    expect(timestampMatchesDateRange(15, { from: 10, toExclusive: 10 })).toBe(true);
  });
});

