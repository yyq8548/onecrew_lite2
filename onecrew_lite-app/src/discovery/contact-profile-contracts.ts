import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";

const text = z.string().refine(value => value.isWellFormed());
const savedInstagramProfile = z.strictObject({
  adapter_version: z.literal("1.0.0"), id: text.regex(/^[1-9][0-9]{0,24}$/u), username: text.regex(/^[a-z0-9_.]{1,30}$/u),
  display_name: text.refine(value => [...value].length <= 200 && !/\p{Cc}/u.test(value)), biography: text.refine(value => [...value].length <= 2000 && !value.includes("\0")),
  follower_count: z.number().int().nonnegative().safe().nullable(), private_account: z.boolean().nullable(), verified: z.boolean().nullable(), profile_url: text,
  evidence_source: z.enum(["native_profile_response", "public_profile_dom_search_bound"]),
  public_email: text.refine(value => Buffer.byteLength(value) <= 320 && value.includes("@") && !/\p{Cc}/u.test(value)).nullable().optional(),
  contact_links: z.array(text.refine(source => {
    const url = URL.parse(source); return Buffer.byteLength(source) <= 2048 && !/\p{Cc}/u.test(source) && url !== null && ["http:", "https:"].includes(url.protocol) && !!url.hostname && !url.username && !url.password;
  })).max(10).optional(), web_text: z.unknown().optional(),
}).superRefine((value, context) => {
  if (value.profile_url !== `https://www.instagram.com/${value.username}/`) context.addIssue({ code: "custom", message: "Saved Instagram profile URL does not match username" });
  if (value.evidence_source === "public_profile_dom_search_bound") {
    if (typeof value.web_text !== "string" || [...value.web_text].length > 4000 || value.web_text.includes("\0")) context.addIssue({ code: "custom", message: "Saved Instagram web evidence is invalid" });
  } else if (value.private_account === null || value.verified === null) context.addIssue({ code: "custom", message: "Native Instagram visibility flags are required" });
});
export function validateSavedDiscoveryInstagramProfile(authorId: string, raw: unknown): Record<string, unknown> {
  const value = savedInstagramProfile.parse(raw);
  if (value.id !== authorId || Buffer.byteLength(stringifyApplicationJson(value)) > 16000) throw new Error("review_native_profile_binding_unavailable");
  return value;
}
