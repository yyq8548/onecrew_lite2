import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { publicWebUrl } from "./public-url.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const idValid = (value: string) => /^[1-9]\d{0,19}$/u.test(value);
function target(authorId: string, username: string) { if (!idValid(authorId) || !/^[A-Za-z\d_]{1,30}$/u.test(username)) throw new OpenCliError("opencli_invalid_input"); }
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
export function pinterestProfilePlan(authorId: string, username: string) {
  target(authorId, username);
  const schema = z.strictObject({ author_id: z.literal(authorId), username: z.literal(username), author_label: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value)), profile_url: z.literal(`https://www.pinterest.com/${username}/`), follower_count: z.number().int().nonnegative().safe().nullable(), bio: text.refine(value => Buffer.byteLength(value) <= 8000 && !value.includes("\0")), public_links: z.array(text.refine(value => publicWebUrl(value) !== null)).max(1) });
  return { site: "onecrew-pinterest-reader", name: "profile", args: ["-f", "json", username], addons: ["onecrew-browser-search"], maximumBytes: 20000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function pinterestContentPlan(authorId: string, username: string, pinId?: string) {
  target(authorId, username);
  if (pinId !== undefined && !idValid(pinId)) throw new OpenCliError("opencli_invalid_input");
  const schema = z.strictObject({ pin_id: text.refine(value => idValid(value) && (pinId === undefined || pinId === value)), author_id: z.literal(authorId), author_username: z.literal(username), profile_url: z.literal(`https://www.pinterest.com/${username}/`), source_url: text, text: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 16000 && !value.includes("\0")), text_truncated: z.literal(true) }).refine(row => row.source_url === `https://www.pinterest.com/pin/${row.pin_id}/`);
  return { site: "onecrew-pinterest-reader", name: pinId === undefined ? "content" : "content-pin", args: ["-f", "json", username, authorId, ...(pinId === undefined ? [] : [pinId])], addons: ["onecrew-browser-search"], maximumBytes: 24000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
