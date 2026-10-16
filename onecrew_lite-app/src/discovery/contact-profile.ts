import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { validateSavedDiscoveryInstagramProfile } from "./contact-profile-contracts.ts";
import { pinterestProfilePlan } from "../platforms/opencli/pinterest.ts";
import { validateDiscoveryRetainedProfile } from "./native-material.ts";
import { loadDiscoveryProfileReceipt } from "./review-receipts.ts";
import type { DiscoveryReviewMaterial } from "./review-material.ts";
import { loadDiscoveryWebSourceReceipt } from "./web-receipt.ts";

const object = z.record(z.string(), z.unknown());
const strings = (value: unknown): string[] => Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
function pixivProfile(author: string, value: Record<string, unknown>) {
  const link = z.string().refine(source => {
    const url = URL.parse(source), host = url?.hostname;
    return Buffer.byteLength(source) <= 2048 && !/[\p{Cc}\p{White_Space}]/u.test(source) && url !== null && url.protocol === "https:" && !url.username && !url.password && !url.port && host !== undefined && !host.startsWith("[") && !/^[0-9.]+$/u.test(host) && host.includes(".") && !host.endsWith(".") && !["localhost", "local", "internal", "test", "invalid", "onion", "home.arpa"].some(suffix => host === suffix || host.endsWith(`.${suffix}`));
  });
  if (!/^[1-9][0-9]{0,19}$/u.test(author)) throw new Error("contact_native_profile_binding_unavailable");
  z.strictObject({ adapter_version: z.literal("1.0.0"), author_id: z.literal(author), author_label: z.string().refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value)), profile_url: z.literal(`https://www.pixiv.net/users/${author}`), follower_count: z.null(), bio: z.string().refine(value => Buffer.byteLength(value) <= 8000 && !value.includes("\0")), public_links: z.array(link).max(10), evidence_source: z.literal("native_ajax_response") }).parse(value);
}
export function discoveryProfileContactFields(database: DatabaseSync, material: DiscoveryReviewMaterial): string {
  if (!database.isTransaction) throw new Error("contact_projection_requires_snapshot");
  if (["google", "yahoo"].includes(material.hit.platform)) {
    const read = loadDiscoveryWebSourceReceipt(database, material.campaign_id, material.hit.id), native = read.native;
    if (read.is_primary_source !== 1 || read.status !== "completed" || read.stop_code !== null || read.expected_revision !== material.criteria_revision || read.profile_ref !== material.hit.profile_ref || read.source_url !== material.hit.source_url || read.candidate_id !== material.candidate_id || read.native_author_id !== material.native_author_id) throw new Error("contact_native_web_source_binding_unavailable");
    const social = ["twitter", "douyin"].includes(read.target_platform), profile = social ? object.parse(native.profile) : null;
    const original = social ? object.parse(native.content).text : native.description;
    if ((typeof original === "string" ? original : null) !== material.video_description) throw new Error("contact_native_web_source_binding_unavailable");
    if (profile !== null) {
      if (material.native_profile?.profile_url !== profile.profile_url) throw new Error("contact_native_web_source_binding_unavailable");
      return read.target_platform === "douyin" ? strings(z.array(z.unknown()).parse(profile.public_links)).join("\n") : "";
    }
    return z.array(z.unknown()).parse(native.links).flatMap(value => { const row = object.safeParse(value); return row.success && typeof row.data.url === "string" ? [row.data.url] : []; }).join("\n");
  }
  const platform = material.hit.platform;
  if (!["instagram", "facebook", "pinterest", "pixiv", "linkedin", "douyin"].includes(platform)) return "";
  const read = loadDiscoveryProfileReceipt(database, material.campaign_id, material.hit.id), native = object.parse(read.native_profile);
  if (read.status !== "completed" || read.stop_code !== null || read.claim.expected_revision !== material.criteria_revision || read.claim.platform !== platform || read.claim.profile_ref !== material.hit.profile_ref || (platform === "linkedin" ? native.account_id : read.claim.author_id) !== material.native_author_id || material.native_profile?.profile_url !== read.claim.author_url || Buffer.byteLength(stringifyApplicationJson(native)) > 16000) throw new Error("contact_native_profile_binding_unavailable");
  if (platform === "instagram") validateSavedDiscoveryInstagramProfile(read.claim.author_id, native);
  else if (platform === "pinterest") pinterestProfilePlan(read.claim.author_id, z.string().parse(native.username)).decode([native]);
  else if (platform === "pixiv") pixivProfile(read.claim.author_id, native);
  else validateDiscoveryRetainedProfile(platform, read.claim.author_id, read.claim.author_url, read.claim.source_url, native);
  const fields = typeof native.public_email === "string" ? [native.public_email] : [];
  fields.push(...strings(native[["pinterest", "pixiv", "linkedin", "douyin"].includes(platform) ? "public_links" : "contact_links"]));
  if (platform === "linkedin") fields.push(...strings(native.contact_links));
  return fields.join("\n");
}
