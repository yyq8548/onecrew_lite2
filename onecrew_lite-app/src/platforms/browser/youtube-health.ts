import { parseStoredJson } from "../../contracts/stored-json.ts";

export type YoutubeHealthFailure = "api_key_invalid" | "access_not_configured" | "quota_exceeded" | "rate_limited" | "transient" | "ambiguous_forbidden" | "infrastructure";
const messages: Record<YoutubeHealthFailure, string> = {
  api_key_invalid: "YouTube API key was rejected",
  access_not_configured: "YouTube Data API access is not configured",
  quota_exceeded: "YouTube API quota is exhausted",
  rate_limited: "YouTube API rate limit is active",
  transient: "YouTube provider is temporarily unavailable",
  ambiguous_forbidden: "YouTube provider returned an ambiguous forbidden response",
  infrastructure: "YouTube validation infrastructure is unavailable",
};
export function youtubeHealthOutcome(failure: YoutubeHealthFailure, now: number) {
  return { message: messages[failure], disabled: failure === "api_key_invalid" || failure === "access_not_configured", cooldownUntil: failure === "quota_exceeded" || failure === "rate_limited" ? now + 900 : null };
}
export class YoutubeApiReadError extends Error {
  readonly health: YoutubeHealthFailure;
  constructor(health: YoutubeHealthFailure) { super(messages[health]); this.health = health; }
}
function object(value: unknown): Record<string, unknown> { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {}; }
export function classifyYoutubeApiFailure(status: number, value: unknown): YoutubeHealthFailure {
  if (status === 429) return "rate_limited";
  if (status >= 500) return "transient";
  const error = object(object(value).error);
  const reasons = Array.isArray(error.errors) ? error.errors.map(item => object(item).reason).filter((item): item is string => typeof item === "string").map(item => item.toLowerCase()) : [];
  const message = typeof error.message === "string" ? error.message.toLowerCase() : "";
  if (reasons.some(reason => ["keyinvalid", "iprefererblocked", "forbiddenfornonpartnere"].includes(reason)) || message.includes("api key not valid")) return "api_key_invalid";
  if (reasons.includes("accessnotconfigured")) return "access_not_configured";
  if (reasons.some(reason => ["quotaexceeded", "dailylimitexceeded"].includes(reason))) return "quota_exceeded";
  if (reasons.some(reason => ["ratelimitexceeded", "userratelimitexceeded"].includes(reason))) return "rate_limited";
  if (status === 401) return "api_key_invalid";
  if (status === 403) return "ambiguous_forbidden";
  return "infrastructure";
}
export async function youtubeHttpFailure(status: number, body: string): Promise<YoutubeApiReadError> {
  // 无效JSON仍按HTTP状态分类；原响应正文从不进入错误对象或健康记录。
  const value: unknown = await Promise.resolve().then(() => parseStoredJson(body, true)).then(value => value, error => { if (!(error instanceof SyntaxError)) throw error; return undefined; });
  return new YoutubeApiReadError(classifyYoutubeApiFailure(status, value));
}
