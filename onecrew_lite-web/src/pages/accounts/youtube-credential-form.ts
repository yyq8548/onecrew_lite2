import type {
  RenameYoutubeCredentialInput,
  RotateYoutubeCredentialInput,
} from "@/types";

export type YoutubeDeleteIntent = {
  credentialId: string;
  idempotencyKey: string;
};

/** Confirm a local list readback no longer projects the deleted credential. */
export function credentialMissingFromReadback(
  credentials: readonly { id: string }[],
  credentialId: string,
): boolean {
  return !credentials.some((credential) => credential.id === credentialId);
}

/** Reuse the key for one candidate and create a new key for a new candidate. */
export function selectYoutubeDeleteIntent(
  current: YoutubeDeleteIntent | null,
  credentialId: string,
  nextIdempotencyKey: string,
): YoutubeDeleteIntent {
  if (current?.credentialId === credentialId) return current;
  return { credentialId, idempotencyKey: nextIdempotencyKey };
}

export type YoutubeCredentialEditAction =
  | { kind: "rename"; input: RenameYoutubeCredentialInput }
  | { kind: "rotate"; input: RotateYoutubeCredentialInput };

/** Build one explicit edit action; a blank key always means preserve it. */
export function youtubeCredentialEditAction(input: {
  credentialId: string;
  label: string;
  apiKey: string;
}): YoutubeCredentialEditAction | null {
  const label = input.label.trim();
  if (!label) return null;

  const apiKey = input.apiKey.trim();
  if (!apiKey) {
    return {
      kind: "rename",
      input: {
        credential_id: input.credentialId,
        label,
        confirmed: true,
      },
    };
  }

  return {
    kind: "rotate",
    input: {
      credential_id: input.credentialId,
      api_key: apiKey,
      confirmed: true,
    },
  };
}
