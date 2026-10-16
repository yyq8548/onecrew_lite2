import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

export type NativeOpenCliCommentSource = { readonly kind: "bilibili_video" | "zhihu_answer" | "weibo_post" | "reddit_post" | "tiktok_video" | "instagram_post" | "facebook_post" | "facebook_reel"; readonly source: string };
export function nativeCommentsPlan(target: NativeOpenCliCommentSource, limit: number) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || !target.source.isWellFormed()) throw new OpenCliError("opencli_invalid_input");
  let site: string;
  let name = "comments";
  const source = target.source;
  let valid = false;
  switch (target.kind) {
    case "bilibili_video": site = "onecrew-comments-bilibili"; valid = /^BV[A-Za-z\d]{10}$/u.test(source); break;
    case "zhihu_answer": site = "onecrew-comments-zhihu"; name = "answer-comments"; valid = /^\d{1,20}$/u.test(source); break;
    case "weibo_post": site = "onecrew-comments-weibo"; valid = /^\d{1,20}$/u.test(source); break;
    case "reddit_post": site = "onecrew-comments-reddit"; name = "read"; valid = /^[a-z\d]{1,15}$/u.test(source); break;
    case "tiktok_video": site = "onecrew-comments-tiktok"; valid = /^https:\/\/www\.tiktok\.com\/@[A-Za-z\d_.]{1,24}\/video\/[1-9]\d{0,24}$/u.test(source); break;
    case "instagram_post": site = "onecrew-comments-instagram"; valid = /^https:\/\/www\.instagram\.com\/(?:[A-Za-z\d_.]{1,30}\/)?(?:p|reel)\/[A-Za-z\d_-]{1,32}\/$/u.test(source); break;
    case "facebook_post": site = "onecrew-comments-facebook"; valid = /^https:\/\/www\.facebook\.com\/[A-Za-z\d.]{1,100}\/posts\/(?:pfbid[A-Za-z\d]{10,150}|[1-9]\d{0,24})\/*$/u.test(source); break;
    case "facebook_reel": site = "onecrew-comments-facebook"; valid = /^https:\/\/www\.facebook\.com\/reel\/[1-9]\d{0,24}\/?$/u.test(source); break;
    default: throw new OpenCliError("opencli_invalid_input");
  }
  if (!valid) throw new OpenCliError("opencli_invalid_input");
  const bounded = ["bilibili_video", "weibo_post", "instagram_post", "facebook_post", "facebook_reel"].includes(target.kind) ? Math.min(limit, 5) : limit;
  return { site, name, args: ["--limit", String(bounded), "-f", "json", "--", source], addons: ["onecrew-browser-search"], maximumBytes: 128000, decode(value: unknown) {
    const parsed = z.array(z.record(z.string(), z.unknown())).max(bounded * 2 + 3).safeParse(value);
    if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
    return parsed.data;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
