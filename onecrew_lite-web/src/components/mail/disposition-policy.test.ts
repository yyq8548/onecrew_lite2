import { describe, expect, it } from "vitest";

import {
  shouldShowArchiveAction,
  shouldShowRestoreAction,
  shouldShowUnarchiveAction,
} from "@/components/mail/disposition-policy";

describe("mail disposition policy", () => {
  it("only exposes archive from an Inbox thread", () => {
    expect(shouldShowArchiveAction("inbox", "inbox")).toBe(true);
    expect(shouldShowArchiveAction("sent", "sent")).toBe(false);
    expect(shouldShowArchiveAction("all", "inbox")).toBe(false);
  });

  it("unarchives from Archive without a Sent destination", () => {
    expect(shouldShowUnarchiveAction("archive", "archive")).toBe(true);
    expect(shouldShowUnarchiveAction("sent", "archive")).toBe(false);
  });

  it("restores Deleted threads from the Deleted context", () => {
    expect(shouldShowRestoreAction("deleted", "deleted")).toBe(true);
    expect(shouldShowRestoreAction("archive", "deleted")).toBe(false);
  });
});
