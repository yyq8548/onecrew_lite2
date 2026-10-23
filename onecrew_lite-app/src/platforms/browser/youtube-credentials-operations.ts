import { z } from "zod";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { OwnerPlatformVaultStore } from "./owner-vault-store.ts";
import { YoutubeCredentials } from "./youtube-credentials.ts";
import { youtubeCredentialConnectInput, youtubeCredentialRenameInput, youtubeCredentialRotateInput } from "./youtube-credentials-validation.ts";
import type { YoutubeRequest } from "./youtube-request-budget.ts";

export function registerYoutubeCredentialOperations(registry: OperationRegistry, accounts: AccountContextService, store: OwnerPlatformVaultStore, onChanged: ((context: AccountContext) => void) | undefined, requestForCredential: (id: string) => YoutubeRequest): YoutubeCredentials {
  const credentials = new YoutubeCredentials(accounts, store, onChanged, requestForCredential);
  registry.register("connect_youtube_api_key", z.strictObject({ input: youtubeCredentialConnectInput }), async ({ input }, context) => credentials.connect(context, input));
  registry.register("rotate_youtube_api_key", z.strictObject({ input: youtubeCredentialRotateInput }), async ({ input }, context) => credentials.rotate(context, input));
  registry.register("rename_youtube_api_key", z.strictObject({ input: youtubeCredentialRenameInput }), async ({ input }, context) => credentials.rename(context, input));
  registry.register("test_youtube_api_key", z.strictObject({ credentialId: z.string() }), async ({ credentialId }, context) => credentials.test(context, credentialId));
  registry.register("delete_youtube_api_key", z.strictObject({ credentialId: z.string(), confirmed: z.boolean(), idempotencyKey: z.string() }), async (input, context) => credentials.delete(context, { credential_id: input.credentialId, confirmed: input.confirmed, idempotency_key: input.idempotencyKey }));
  return credentials;
}
