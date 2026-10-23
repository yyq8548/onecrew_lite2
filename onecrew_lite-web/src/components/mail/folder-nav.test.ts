import { describe, expect, it } from "vitest";

import { emailFolderCount } from "@/components/mail/folder-nav";
import type { EmailFolderSummary } from "@/lib/creator";

describe("mail folder counts", () => {
  it("reads the refreshed canonical draft count instead of deriving it from thread rows", () => {
    const before: EmailFolderSummary[] = [{ folder: "drafts", count: 0 }];
    const after: EmailFolderSummary[] = [{ folder: "drafts", count: 1 }];

    expect(emailFolderCount(before, "drafts")).toBe(0);
    expect(emailFolderCount(after, "drafts")).toBe(1);
    expect(emailFolderCount(undefined, "drafts")).toBeNull();
  });
});
