import { describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@/lib/local-runtime", () => ({ invoke: invokeMock }));

import {
  connectYoutubeApiKey,
  deleteYoutubeApiKey,
  renameYoutubeApiKey,
  rotateYoutubeApiKey,
  testYoutubeApiKey,
} from "@/lib/service-credentials";
import type { ServiceCredentialProjection, YoutubeCredentialDeleteOutcome } from "@/types";

describe("YouTube service credential client", () => {
  it("keeps command names and Tauri payload shapes typed", async () => {
    invokeMock.mockResolvedValue({
      credential_id: "credential-id",
      ok: true,
      checked_at: 1,
      channel_id: "UC-test",
      issue: null,
      message: "available",
    });
    connectYoutubeApiKey({ label: "prod", project: null, api_key: "AIza-new" });
    await testYoutubeApiKey("credential-id");
    const deleteResult = deleteYoutubeApiKey({
      credentialId: "credential-id",
      confirmed: true,
      idempotencyKey: "delete:credential-id",
    });
    rotateYoutubeApiKey({
      credential_id: "credential-id",
      api_key: "AIza-rotated",
      confirmed: true,
    });
    const renameResult = renameYoutubeApiKey({
      credential_id: "credential-id",
      label: "renamed",
      confirmed: true,
    });
    const typedRenameResult: Promise<ServiceCredentialProjection> = renameResult;
    const typedDeleteResult: Promise<YoutubeCredentialDeleteOutcome> = deleteResult;
    void typedRenameResult;
    void typedDeleteResult;

    expect(invokeMock.mock.calls).toEqual([
      ["connect_youtube_api_key", { input: { label: "prod", project: null, api_key: "AIza-new" } }],
      ["test_youtube_api_key", { credentialId: "credential-id" }],
      ["delete_youtube_api_key", { credentialId: "credential-id", confirmed: true, idempotencyKey: "delete:credential-id" }],
      ["rotate_youtube_api_key", { input: { credential_id: "credential-id", api_key: "AIza-rotated", confirmed: true } }],
      ["rename_youtube_api_key", { input: { credential_id: "credential-id", label: "renamed", confirmed: true } }],
    ]);
  });

  it("rejects a structured provider rejection before Accounts can show success", async () => {
    invokeMock.mockResolvedValueOnce({
      credential_id: "credential-id",
      ok: false,
      checked_at: 2,
      channel_id: null,
      issue: "api_key_invalid",
      message: "provider text is not shown by the helper",
    });

    await expect(testYoutubeApiKey("credential-id")).rejects.toThrow(
      "YouTube API key was rejected",
    );
  });
});
