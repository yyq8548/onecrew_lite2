import { describe, expect, it } from "vitest";

import {
  credentialMissingFromReadback,
  selectYoutubeDeleteIntent,
  youtubeCredentialEditAction,
} from "./youtube-credential-form";

describe("credentialMissingFromReadback", () => {
  it("accepts only a readback where the target id is absent", () => {
    expect(credentialMissingFromReadback([{ id: "other" }], "deleted")).toBe(true);
    expect(credentialMissingFromReadback([{ id: "deleted" }], "deleted")).toBe(false);
  });
});

describe("selectYoutubeDeleteIntent", () => {
  it("reuses the same idempotency key for a retry of one candidate", () => {
    const first = selectYoutubeDeleteIntent(null, "credential-id", "delete-key-1");
    expect(selectYoutubeDeleteIntent(first, "credential-id", "delete-key-2")).toBe(first);
  });

  it("creates a new intent when the candidate changes", () => {
    const first = selectYoutubeDeleteIntent(null, "credential-a", "delete-key-1");
    expect(selectYoutubeDeleteIntent(first, "credential-b", "delete-key-2")).toEqual({
      credentialId: "credential-b",
      idempotencyKey: "delete-key-2",
    });
  });
});

describe("youtubeCredentialEditAction", () => {
  it("uses a secret-free rename action when the key is blank", () => {
    expect(youtubeCredentialEditAction({
      credentialId: "credential-id",
      label: "  renamed  ",
      apiKey: "   ",
    })).toEqual({
      kind: "rename",
      input: {
        credential_id: "credential-id",
        label: "renamed",
        confirmed: true,
      },
    });
  });

  it("uses only the replacement action when a new key is supplied", () => {
    expect(youtubeCredentialEditAction({
      credentialId: "credential-id",
      label: "renamed but not submitted",
      apiKey: "  AIza-new  ",
    })).toEqual({
      kind: "rotate",
      input: {
        credential_id: "credential-id",
        api_key: "AIza-new",
        confirmed: true,
      },
    });
  });

  it("rejects an empty label without manufacturing a mutation", () => {
    expect(youtubeCredentialEditAction({
      credentialId: "credential-id",
      label: "  ",
      apiKey: "AIza-new",
    })).toBeNull();
  });
});
