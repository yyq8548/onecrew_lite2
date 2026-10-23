import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

export const BILIBILI_VIDEO_FIELDS = ["bvid", "aid", "title", "author", "category", "publish_time", "duration", "view", "danmaku", "reply", "like", "coin", "favorite", "share", "parts", "thumbnail", "description", "requires_payment", "payment_type", "pay_preview", "redirect_url"] as const;
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
export function bilibiliVideoPlan(sourceUrl: string, authorId?: string) {
  const bvid = sourceUrl.startsWith("https://www.bilibili.com/video/") ? sourceUrl.slice(31) : "";
  if (!/^BV[A-Za-z\d]{10}$/u.test(bvid) || (authorId !== undefined && !/^[1-9]\d{0,19}$/u.test(authorId))) throw new OpenCliError("opencli_invalid_input");
  return { site: "bilibili", name: "video", args: ["-f", "json", "--", bvid], addons: [], maximumBytes: 160000, decode(value: unknown) {
    const parsed = z.array(z.strictObject({ field: z.enum(BILIBILI_VIDEO_FIELDS), value: z.string().refine(text => text.isWellFormed() && !text.includes("\0")) })).length(BILIBILI_VIDEO_FIELDS.length).safeParse(value);
    if (!parsed.success) return invalid();
    const fields = new Map<string, string>();
    for (const row of parsed.data) {
      if (fields.has(row.field) || Buffer.byteLength(row.value) > (row.field === "description" ? 128000 : 2048)) return invalid();
      fields.set(row.field, row.value);
    }
    const get = (key: string) => fields.get(key) ?? invalid();
    const author = /^(.*) \(mid: ([1-9]\d{0,19})\)$/su.exec(get("author"));
    if (get("bvid") !== bvid || !author || !author[1]!.trim() || [...author[1]!].length > 200 || /\p{Cc}/u.test(author[1]!) || (authorId !== undefined && author[2] !== authorId)) return invalid();
    const title = get("title");
    if (!title.trim() || [...title].length > 500 || /\p{Cc}/u.test(title)) return invalid();
    const boolean = (key: string) => { const value = get(key); return value === "true" ? true : value === "false" ? false : invalid(); };
    const requires_payment = boolean("requires_payment"); const pay_preview = boolean("pay_preview"); const payment = get("payment_type");
    if (!["", "vip", "ugc_pay", "upower"].includes(payment) || requires_payment !== Boolean(payment) || get("redirect_url") !== "") return invalid();
    const metric = (key: string) => { const value = get(key); return /^\d+$/u.test(value) && Number.isSafeInteger(Number(value)) ? Number(value) : null; };
    const observations = nativeContentObservations.parse({ published_at: get("publish_time") || null, view_count: metric("view"), like_count: metric("like"), comment_count: metric("reply"), share_count: metric("share"), save_count: metric("favorite") });
    const raw = get("description"); let description = ""; let bytes = 0;
    for (const character of raw) { const length = Buffer.byteLength(character); if (bytes + length > 8192) break; bytes += length; description += character; }
    return { bvid, author_id: author[2]!, author: author[1]!, source_url: sourceUrl, title, description, description_truncated: bytes < Buffer.byteLength(raw), requires_payment, pay_preview, payment_status_known: true, observations };
  } } satisfies ReviewedOpenCliRead<unknown>;
}

export function bilibiliVideoPagePlan(sourceUrl: string, authorId: string) {
  const native = bilibiliVideoPlan(sourceUrl, authorId);
  const bvid = sourceUrl.slice(31);
  return { site: "onecrew-bilibili-video", name: "read", args: ["-f", "json", "--", bvid, authorId], addons: ["onecrew-bilibili-video", "onecrew-browser-search"], maximumBytes: native.maximumBytes,
    decode(value: unknown) {
      return { ...native.decode(value), payment_status_known: false };
    },
  } satisfies ReviewedOpenCliRead<unknown>;
}
