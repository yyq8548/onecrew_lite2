import { invoke } from "@/lib/local-runtime";

import type {
  RenameYoutubeCredentialInput,
  RotateYoutubeCredentialInput,
  ServiceCredentialConnectResult,
  ServiceCredentialProjection,
  YoutubeApiKeyConnectInput,
  YoutubeCredentialDeleteInput,
  YoutubeCredentialDeleteOutcome,
  YoutubeCredentialTest,
} from "@/types";

/** Typed Tauri boundary for YouTube service-credential mutations. */
export function connectYoutubeApiKey(input: YoutubeApiKeyConnectInput) {
  return invoke<ServiceCredentialConnectResult>("connect_youtube_api_key", { input });
}

export async function testYoutubeApiKey(credentialId: string) {
  const result = await invoke<YoutubeCredentialTest>("test_youtube_api_key", { credentialId });
  if (!result.ok) {
    const message = result.issue === "api_key_invalid"
      ? "YouTube API key was rejected"
      : result.issue === "access_not_configured"
        ? "YouTube Data API access is not configured"
        : "YouTube credential validation was rejected";
    throw new Error(message);
  }
  return result;
}

export function deleteYoutubeApiKey(input: YoutubeCredentialDeleteInput) {
  return invoke<YoutubeCredentialDeleteOutcome>("delete_youtube_api_key", input);
}

export function rotateYoutubeApiKey(input: RotateYoutubeCredentialInput) {
  return invoke<ServiceCredentialConnectResult>("rotate_youtube_api_key", { input });
}

export function renameYoutubeApiKey(input: RenameYoutubeCredentialInput) {
  return invoke<ServiceCredentialProjection>("rename_youtube_api_key", { input });
}
