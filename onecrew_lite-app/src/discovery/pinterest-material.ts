import { z } from "zod";
import { publicLink as validDiscoverySavedLink } from "./historical-native-material.ts";

const text = z.string().refine(value => value.isWellFormed());
const id = text.regex(/^[1-9][0-9]{0,19}$/u), username = text.regex(/^[A-Za-z0-9_]{1,30}$/u);
const profile = z.strictObject({ author_id: id, username, author_label: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value)), profile_url: text, follower_count: z.number().int().nonnegative().safe().nullable(), bio: text.refine(value => Buffer.byteLength(value) <= 8000 && !value.includes("\0")), public_links: z.array(text.refine(validDiscoverySavedLink)).max(1) }).refine(value => value.profile_url === `https://www.pinterest.com/${value.username}/`);
export function validateDiscoveryPinterestProfile(author: string, authorUrl: string, raw: unknown): Record<string, unknown> {
  const value = profile.parse(raw);
  if (value.author_id !== author || value.profile_url !== authorUrl) throw new Error("review_native_profile_binding_unavailable");
  return value;
}
export function validateDiscoveryPinterestContent(author: string, requested: string, raw: unknown): Record<string, unknown> {
  const value = z.strictObject({ pin_id: id, author_id: z.literal(id.parse(author)), author_username: username, profile_url: text, source_url: text, text: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 16000 && !value.includes("\0")), text_truncated: z.literal(true) }).parse(raw);
  if (value.profile_url !== `https://www.pinterest.com/${value.author_username}/` || value.source_url !== `https://www.pinterest.com/pin/${value.pin_id}/` || requested !== value.profile_url && requested !== value.source_url) throw new Error("review_native_content_binding_unavailable");
  return value;
}
