import { z } from "zod";
import { nativeContentObservations } from "../platforms/opencli/observations.ts";
import { publicLink } from "./historical-native-material.ts";
const text = (maximum: number) => z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= maximum && !value.includes("\0"));
const author = z.string().regex(/^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/u), label = text(512).refine(value => !!value.trim() && !/\p{Cc}/u.test(value));
function exactFollowers(value: string): number | null {
  if (!/^(?:0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+)$/u.test(value)) return null;
  const count = Number(value.replaceAll(",", "")); return Number.isSafeInteger(count) ? count : null;
}
export function validateSavedDiscoveryDouyinProfile(raw: unknown) {
  const profile = z.strictObject({ adapter_version: z.literal("1.0.0"), author_id: author, author_label: label, profile_url: z.string(), follower_count: z.number().int().nonnegative().safe().nullable(), follower_text: text(100), bio: text(8000), bio_truncated: z.boolean(), public_links: z.array(z.string().refine(publicLink)).max(10), evidence_source: z.literal("visible_profile_header") }).parse(raw);
  if (profile.profile_url !== `https://www.douyin.com/user/${profile.author_id}` || profile.follower_count !== exactFollowers(profile.follower_text)) throw new Error("discovery_douyin_profile_invalid"); return profile;
}
export function validateSavedDiscoveryDouyinContent(expectedAuthor: string, source: string, raw: unknown) {
  const post = /^https:\/\/www\.douyin\.com\/video\/([1-9][0-9]{0,19})$/u.exec(source)?.[1];
  if (!post) throw new Error("discovery_douyin_content_invalid"); author.parse(expectedAuthor);
  const content = z.strictObject({ post_id: z.literal(post), author_id: z.literal(expectedAuthor), author_label: label, profile_url: z.literal(`https://www.douyin.com/user/${expectedAuthor}`), source_url: z.literal(source), text: text(12000).refine(value => !!value.trim()), text_truncated: z.boolean(), evidence_source: z.literal("visible_native_video_caption"), observations: nativeContentObservations.optional() }).parse(raw);
  return content;
}
