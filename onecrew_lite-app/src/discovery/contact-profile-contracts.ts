import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { instagramVisiblePosts, instagramVisiblePostsMatchUsername } from "../platforms/opencli/instagram-visible-contract.ts";

const text = z.string().refine(value => value.isWellFormed());
const savedInstagramProfile = z.strictObject({
  adapter_version: z.literal("1.0.0"), id: text.refine(value => /^[1-9][0-9]{0,24}$/u.test(value) || /^https:\/\/www\.instagram\.com\/[a-z0-9_.]{1,30}\/$/u.test(value)), username: text.regex(/^[a-z0-9_.]{1,30}$/u),
  display_name: text.refine(value => [...value].length <= 200 && !/\p{Cc}/u.test(value)), biography: text.refine(value => [...value].length <= 2000 && !value.includes("\0")),
  follower_count: z.number().int().nonnegative().safe().nullable(), follower_text: text.max(80).optional(), private_account: z.boolean().nullable(), verified: z.boolean().nullable(), profile_url: text,
  evidence_source: z.enum(["native_profile_response", "public_profile_dom_search_bound", "public_profile_dom_profile_bound"]), identity_kind: z.enum(["native_id", "profile_url"]).optional(),
  execution_mode: z.enum(["opencli_dom", "computer_use"]).optional(), bio_truncated: z.boolean().optional(), bio_status: z.enum(["complete", "truncated", "missing"]).optional(),
  public_email: text.refine(value => Buffer.byteLength(value) <= 320 && value.includes("@") && !/\p{Cc}/u.test(value)).nullable().optional(),
  contact_links: z.array(text.refine(source => {
    const url = URL.parse(source); return Buffer.byteLength(source) <= 2048 && !/\p{Cc}/u.test(source) && url !== null && ["http:", "https:"].includes(url.protocol) && !!url.hostname && !url.username && !url.password;
  })).max(10).optional(), web_text: z.unknown().optional(), visible_posts: instagramVisiblePosts.optional(),
}).superRefine((value, context) => {
  if (value.profile_url !== `https://www.instagram.com/${value.username}/`) context.addIssue({ code: "custom", message: "Saved Instagram profile URL does not match username" });
  if (value.id === value.profile_url && (value.identity_kind !== "profile_url" || value.evidence_source !== "public_profile_dom_profile_bound" || !["opencli_dom", "computer_use"].includes(value.execution_mode ?? ""))) context.addIssue({ code: "custom", message: "Instagram page identity requires visible account verification" });
  if (value.id !== value.profile_url && (value.identity_kind === "profile_url" || value.evidence_source === "public_profile_dom_profile_bound")) context.addIssue({ code: "custom", message: "Instagram identity kind differs from its locator" });
  if (!instagramVisiblePostsMatchUsername(value.visible_posts, value.username)) context.addIssue({ code: "custom", message: "Saved Instagram post URL does not match username" });
  if (value.evidence_source !== "native_profile_response") {
    if (typeof value.web_text !== "string" || [...value.web_text].length > 4000 || value.web_text.includes("\0")) context.addIssue({ code: "custom", message: "Saved Instagram web evidence is invalid" });
  } else if (value.private_account === null || value.verified === null) context.addIssue({ code: "custom", message: "Native Instagram visibility flags are required" });
});
export function validateSavedDiscoveryInstagramProfile(authorId: string, raw: unknown): Record<string, unknown> {
  const value = savedInstagramProfile.parse(raw);
  if (value.id !== authorId || Buffer.byteLength(stringifyApplicationJson(value)) > 16000) throw new Error("review_native_profile_binding_unavailable");
  return value;
}
