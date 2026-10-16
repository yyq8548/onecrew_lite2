import { createReadSession } from "../../native-capabilities/network.ts";
import { instagramSearchInput } from "../../discovery/native-search-inputs.ts";
import type { PlatformProfileSecret } from "./platform-vault.ts";
import { instagramReadHeaders, nativeProxy } from "../candidates/social-read.ts";
import { isLosslessNumber } from "lossless-json";
import { parseStoredJson } from "../../contracts/stored-json.ts";

const record = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const string = (value: unknown) => typeof value === "string" ? value : "";
function integer(value: unknown): bigint | undefined {
  const integer = isLosslessNumber(value) ? /^-?\d+$/u.test(value.value) ? BigInt(value.value) : undefined : typeof value === "bigint" ? value : typeof value === "number" && Number.isSafeInteger(value) ? BigInt(value) : undefined;
  return integer !== undefined && integer >= -9223372036854775808n && integer <= 9223372036854775807n ? integer : undefined;
}
export function projectNativeInstagramUsers(payload: unknown) {
  const value = record(payload);
  if (value.status === "fail") throw new Error("instagram.provider_refused");
  return (Array.isArray(value.users) ? value.users : []).map(item => {
    const user = record(item), pk = user.pk;
    const result: Record<string, unknown> = { id: typeof pk === "string" ? pk : isLosslessNumber(pk) ? pk.value : typeof pk === "bigint" || typeof pk === "number" && Number.isFinite(pk) ? String(pk) : "", username: string(user.username), full_name: string(user.full_name) };
    for (const key of ["is_private", "is_verified"] as const) if (typeof user[key] === "boolean") result[key] = user[key];
    for (const key of ["profile_pic_url", "biography", "external_url", "public_email", "city_name", "address_street"] as const) if (typeof user[key] === "string" && user[key].length > 0) result[key] = user[key];
    for (const key of ["follower_count", "following_count", "media_count"] as const) { const value = integer(user[key]); if (value !== undefined) result[key] = value; }
    const links: string[] = [];
    for (const item of Array.isArray(user.bio_links) ? user.bio_links : []) { const link = record(item), url = string(link.url) || string(link.lynx_url); if (url && !links.includes(url)) links.push(url); }
    if (links.length) result.bio_links = links;
    return result;
  });
}
export async function readNativeInstagramUsers(secret: PlatformProfileSecret, arguments_: unknown, signal: AbortSignal, assertCurrent: () => Promise<void>) {
  const input = instagramSearchInput.parse(arguments_), path = "/api/v1/users/search/", url = new URL(`https://i.instagram.com${path}`);
  for (const [key, value] of Object.entries({ search_surface: "user_search_page", timezone_offset: "0", count: String(input.count), q: input.query })) url.searchParams.set(key, value);
  const session = await createReadSession(["https://i.instagram.com"], nativeProxy(secret.proxy), { browser: "okhttp_5", headers: instagramReadHeaders(secret), allowPath: url => url.pathname === path });
  try { await assertCurrent(); const payload = parseStoredJson(await session.getText(url, signal)); await assertCurrent(); return projectNativeInstagramUsers(payload); }
  finally { await session.close(); }
}
