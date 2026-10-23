import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContextService } from "../identity/account-context.ts";
import type { OwnedPlatformAccountResolver } from "./browser/owned-accounts.ts";
import { mcpConnection } from "../service/mcp-contracts.ts";
import { readNativeXGraphql } from "./browser/native-x-search.ts";
import { projectNativeXProfile, projectNativeXTimeline } from "./browser/native-x-projection.ts";
import { createReadSession } from "../native-capabilities/network.ts";
import { instagramReadHeaders, nativeProxy } from "./candidates/social-read.ts";
import { projectNativeInstagramUsers } from "./browser/native-instagram-search.ts";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { readNativeYoutubeChannel } from "./public-youtube-channel.ts";

export const publicReadOperationIds = ["x.profile.get", "x.user_posts.list", "x.post_thread.get", "instagram.profile.get", "youtube.channel.get"] as const;
const text = z.string().refine(value => value.isWellFormed() && !/\p{Cc}/u.test(value));
const handle = z.strictObject({ handle: text.transform(value => value.trim().replace(/^@+/u, "")).refine(value => value.length > 0) });
const userPosts = z.strictObject({ external_user_id: text.trim().min(1), cursor: text.nullish() });
const thread = z.strictObject({ content_id: text.trim().min(1), cursor: text.nullish(), count: z.number().int().min(1).max(100).nullish() });
export const youtubeChannelReadInput = z.strictObject({ channel_id: text.transform(value => value.trim()).refine(value => /^UC[A-Za-z0-9_-]{22}$/u.test(value)), depth: z.enum(["light", "full"]) });
const schemas = { "x.profile.get": handle, "x.user_posts.list": userPosts, "x.post_thread.get": thread, "instagram.profile.get": handle, "youtube.channel.get": youtubeChannelReadInput };
export function publicReadScope(operationId: typeof publicReadOperationIds[number]) {
  return z.strictObject({ operation_id: z.literal(operationId), schema_revision: text, connection: mcpConnection, arguments: schemas[operationId] }).superRefine((value, issue) => {
    const platform = operationId.split(".")[0];
    if (platform === "youtube" ? value.connection.kind !== "service_credential" || value.connection.toolkit !== "youtube" : value.connection.kind !== "operator_account" || value.connection.toolkit !== platform) issue.addIssue({ code: "custom", message: "public_read_connection_invalid" });
  });
}
export function registerPublicReadOperations(registry: OperationRegistry, accounts: AccountContextService, platforms: OwnedPlatformAccountResolver): void {
  for (const operationId of publicReadOperationIds) registry.register(operationId, publicReadScope(operationId), async (input, context) => {
    accounts.assertCurrent(context); context.signal.throwIfAborted();
    if (input.connection.kind === "service_credential") {
      if (operationId !== "youtube.channel.get") throw new Error("public_read_connection_invalid");
      const value = youtubeChannelReadInput.parse(input.arguments);
      return platforms.withYoutubeCredential(context, input.connection.credential_id, async (_credential, key, assertOwned, request) => {
        const current = async () => { accounts.assertCurrent(context); await assertOwned(); context.signal.throwIfAborted(); };
        return { channel: await readNativeYoutubeChannel(key, value, context.signal, current, request) };
      });
    }
    if (input.connection.kind !== "operator_account" || input.connection.toolkit === "telegram") throw new Error("public_read_connection_invalid");
    const toolkit = input.connection.toolkit;
    return platforms.withProfile(context, input.connection.operator_account_id, toolkit, async (_profile, secret, assertOwned) => {
      const current = async () => { accounts.assertCurrent(context); await assertOwned(); context.signal.throwIfAborted(); };
      await current();
      if (operationId === "instagram.profile.get" && toolkit === "instagram") {
        const value = handle.parse(input.arguments);
        if (!/^[A-Za-z0-9_.]{1,30}$/u.test(value.handle)) throw new Error("instagram.profile_target_invalid");
        const path = `/api/v1/users/${value.handle}/usernameinfo/`, session = await createReadSession(["https://i.instagram.com"], nativeProxy(secret.proxy), { browser: "okhttp_5", headers: instagramReadHeaders(secret), allowPath: url => url.pathname === path });
        try {
          await current(); const payload = z.object({ status: z.string().optional(), user: z.record(z.string(), z.unknown()) }).parse(parseStoredJson(await session.getText(new URL(`https://i.instagram.com${path}`), context.signal))); await current();
          if (payload.status === "fail") throw new Error("instagram.provider_refused");
          return { profile: projectNativeInstagramUsers({ users: [payload.user] })[0]! };
        } finally { await session.close(); }
      }
      if (toolkit !== "x") throw new Error("public_read_connection_invalid");
      if (operationId === "x.profile.get") {
        const value = handle.parse(input.arguments);
        return { profile: projectNativeXProfile(await readNativeXGraphql(secret, "UserByScreenName", { screen_name: value.handle, withSafetyModeUserFields: true }, "GET", context.signal, current)) };
      }
      if (operationId === "x.user_posts.list") {
        const value = userPosts.parse(input.arguments);
        return { page: projectNativeXTimeline(await readNativeXGraphql(secret, "UserTweets", { userId: value.external_user_id, count: 20, includePromotedContent: true, withQuickPromoteEligibilityTweetFields: true, withVoice: true, withV2Timeline: true, ...(value.cursor == null ? {} : { cursor: value.cursor }) }, "GET", context.signal, current)) };
      }
      if (operationId === "x.post_thread.get") {
        const value = thread.parse(input.arguments);
        return projectNativeXTimeline(await readNativeXGraphql(secret, "TweetDetail", { focalTweetId: value.content_id, count: Math.min(value.count ?? 40, 40), referrer: "tweet", with_rux_injections: false, includePromotedContent: true, rankingMode: "Relevance", withCommunity: true, withQuickPromoteEligibilityTweetFields: true, withBirdwatchNotes: true, withVoice: true, ...(value.cursor == null ? {} : { cursor: value.cursor }) }, "GET", context.signal, current));
      }
      throw new Error("public_read_operation_invalid");
    });
  });
}
