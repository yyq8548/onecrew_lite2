import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";
import { nativeCommentsPlan } from "./native-comments.ts";

const text = z.string().refine(value => value.isWellFormed());
const numeric = (value: string) => /^[1-9]\d{0,19}$/u.test(value);
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
export function weiboProfilePlan(authorId: string) {
  if (!numeric(authorId)) throw new OpenCliError("opencli_invalid_input");
  const schema = z.strictObject({ id: z.literal(authorId), name: text.refine(value => !!value.trim() && [...value].length <= 200 && !/\p{Cc}/u.test(value)), follower_count: z.number().int().nonnegative().safe().nullable(), profile_url: z.literal(`https://weibo.com/u/${authorId}`), bio: text.refine(value => [...value].length <= 2000 && !value.includes("\0")), bio_truncated: z.boolean().optional(), bio_status: z.enum(["complete", "truncated", "missing"]).optional(), self_declared_location: text.refine(value => [...value].length <= 200 && !value.includes("\0")) });
  return { site: "onecrew-weibo-reader", name: "profile", args: ["-f", "json", "--", authorId], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode(value: unknown) {
    const parsed = z.array(schema).length(1).safeParse(value);
    return parsed.success ? parsed.data[0]! : invalid();
  } } satisfies ReviewedOpenCliRead<unknown>;
}
const postFields = ["id", "mblogid", "author", "text", "created_at", "source", "reposts", "comments", "likes", "pic_count", "url", "retweeted_from", "retweeted_text", "text_completeness"] as const;
export function weiboCommentsPlan(nativePostId: string) {
  if (!numeric(nativePostId)) throw new OpenCliError("opencli_invalid_input");
  const plan = nativeCommentsPlan({ kind: "weibo_post", source: nativePostId }, 5);
  return { ...plan, decode(value: unknown) {
    const rows = plan.decode(value); if (rows.length > 5) return invalid();
    const seen = new Set<string>();
    return rows.map((row, index) => {
      const body = text.safeParse(row.text); const id = row.id;
      if (typeof id !== "string" || !numeric(id) || row.rank !== index + 1 || row.text_truncated !== false || !body.success) return invalid();
      if (!body.data.trim() || Buffer.byteLength(body.data) > 2000 || body.data.includes("\0") || seen.has(id)) return invalid();
      seen.add(id); return { id, text: body.data };
    });
  } };
}
export function weiboPostPlan(source: string) {
  const match = /^https:\/\/weibo\.com\/([1-9]\d{0,19})\/([A-Za-z\d]{1,20})$/u.exec(source);
  if (!match || BigInt(match[1]!) > BigInt(Number.MAX_SAFE_INTEGER)) throw new OpenCliError("opencli_invalid_input");
  const authorId = match[1]!; const post = match[2]!;
  return { site: "onecrew-weibo-post", name: "post", args: ["-f", "json", "--", post], addons: ["onecrew-browser-search"], maximumBytes: 128000, decode(value: unknown) {
    const parsed = z.array(z.strictObject({ field: z.enum(postFields), value: text })).safeParse(value);
    if (!parsed.success) return invalid();
    const fields = new Map<string, string>();
    for (const row of parsed.data) { if (fields.has(row.field)) return invalid(); fields.set(row.field, row.value); }
    const get = (key: string) => fields.get(key) ?? invalid();
    const id = get("id"); const name = get("author"); const original = get("text");
    if (!numeric(id) || BigInt(id) > BigInt(Number.MAX_SAFE_INTEGER) || get("mblogid") !== post || get("url") !== source || !name.trim() || [...name].length > 200 || /\p{Cc}/u.test(name) || /[\p{Cc}&&[^\n\r\t]]/v.test(original)) return invalid();
    const characters = [...original];
    const count = (key: string) => { const value = fields.get(key); return value !== undefined && /^\d+$/u.test(value) && Number.isSafeInteger(Number(value)) ? Number(value) : null; };
    const completeness = characters.length > 4000 ? "truncated" : fields.get("text_completeness") ?? "unknown";
    const observations = nativeContentObservations.parse({ published_at: fields.get("created_at") || null, like_count: count("likes"), comment_count: count("comments"), share_count: count("reposts"), text_completeness: completeness });
    return { post_id: id, author_id: authorId, author: name, source_url: source, text: characters.slice(0, 4000).join(""), text_truncated: completeness === "truncated", text_completeness_known: completeness !== "unknown", is_repost: fields.has("retweeted_from") || fields.has("retweeted_text"), observations };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
