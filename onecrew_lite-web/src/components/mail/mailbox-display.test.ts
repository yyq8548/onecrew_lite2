import { describe, expect, it } from "vitest";

import { mailboxAddressLocalPart } from "./mailbox-display";

describe("mailboxAddressLocalPart", () => {
  it("uses the address local-part for a compact mailbox label", () => {
    expect(mailboxAddressLocalPart("partnerships@brand.com")).toBe("partnerships");
    expect(mailboxAddressLocalPart("outreach+hk@brand.com")).toBe("outreach+hk");
    expect(mailboxAddressLocalPart("合作@brand.com")).toBe("合作");
  });

  it("keeps a non-address value readable", () => {
    expect(mailboxAddressLocalPart("outreach")).toBe("outreach");
  });
});
