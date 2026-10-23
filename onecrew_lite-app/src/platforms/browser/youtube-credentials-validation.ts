import { z } from "zod";
import { createReadSession, NativeReadTransportError } from "../../native-capabilities/network.ts";
import { YoutubeApiReadError, youtubeHealthOutcome, youtubeHttpFailure, type YoutubeHealthFailure } from "./youtube-health.ts";
import { YoutubeRequestBudget, type YoutubeRequest } from "./youtube-request-budget.ts";

export const youtubeCredentialConnectInput = z.strictObject({ label: z.string(), project: z.string().nullish(), api_key: z.string() });
export const youtubeCredentialRotateInput = z.strictObject({ credential_id: z.string(), api_key: z.string(), confirmed: z.boolean() });
export const youtubeCredentialRenameInput = z.strictObject({ credential_id: z.string(), label: z.string(), confirmed: z.boolean() });
export type YoutubeCredentialProbeIssue = "api_key_invalid" | "access_not_configured" | "rate_limited" | "transient" | "network_unreachable" | "ambiguous_forbidden" | "infrastructure";
export class YoutubeCredentialProbeError extends Error {
  readonly issue: YoutubeCredentialProbeIssue; readonly health: YoutubeHealthFailure;
  constructor(issue: YoutubeCredentialProbeIssue, health: YoutubeHealthFailure) { super(youtubeHealthOutcome(health, 0).message); this.issue = issue; this.health = health; }
}
const response = z.object({ items: z.array(z.object({ id: z.string() })).default([]) });
function providerFailure(error: unknown): never {
  if (error instanceof YoutubeApiReadError) throw new YoutubeCredentialProbeError(error.health === "quota_exceeded" ? "rate_limited" : error.health, error.health);
  if (error instanceof NativeReadTransportError) throw new YoutubeCredentialProbeError(error.message === "native_read_request_failed" || error.message === "native_read_body_unavailable" ? "network_unreachable" : "infrastructure", error.message === "native_read_request_failed" || error.message === "native_read_body_unavailable" ? "transient" : "infrastructure");
  throw error;
}

// 与原 resolve_handle("YouTube") 相同；候选验证不读取或写入任何 Vault。
export async function validateYoutubeCredentialKey(rawKey: string, signal: AbortSignal, assertCurrent: () => Promise<void>, request?: YoutubeRequest) {
  const apiKey = rawKey.trim(); if (!apiKey) throw new Error("请输入 YouTube API Key");
  signal.throwIfAborted(); await assertCurrent();
  const budget = request ? null : new YoutubeRequestBudget(), run = request ?? budget!.run;
  const session = await createReadSession(["https://www.googleapis.com"], undefined, { browser: "chrome_149", headers: {}, allowPath: url => url.pathname === "/youtube/v3/channels", classifyHttpFailure: youtubeHttpFailure });
  try {
    const url = new URL("https://www.googleapis.com/youtube/v3/channels"); url.searchParams.set("part", "snippet"); url.searchParams.set("forHandle", "YouTube"); url.searchParams.set("key", apiKey);
    const raw = await run(async currentSignal => { await assertCurrent(); return session.getJson(url, currentSignal); }, signal).then(value => value, error => { signal.throwIfAborted(); return providerFailure(error); });
    await assertCurrent(); signal.throwIfAborted();
    const parsed = response.safeParse(raw); if (!parsed.success) throw new YoutubeCredentialProbeError("infrastructure", "infrastructure");
    const channel = parsed.data.items[0]; if (!channel) throw new YoutubeCredentialProbeError("transient", "transient");
    return { checked_at: Math.floor(Date.now() / 1000), channel_id: channel.id, message: "YouTube Data API Key 可用" };
  } finally { await session.close(); await budget?.close(); }
}
