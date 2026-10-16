import { z } from "zod";
import type { ReviewedOpenCliRead } from "./read-plan.ts";
import { OpenCliError } from "./errors.ts";
import { youtubeVideoId } from "./youtube.ts";
import { publicWebUrl } from "./public-url.ts";

const text = (maximum: number) => z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= maximum && !value.includes("\0"));
function row<T>(schema: z.ZodType<T>, value: unknown): T { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
const link = z.object({ url: z.string(), label: text(2048), source: z.enum(["shortDescription", "description.runs", "attributedDescription.commandRuns"]) }).strict();
const evidence = z.object({ adapterVersion: z.literal("1.0.0"), videoId: z.string(), channelId: z.string().regex(/^UC[A-Za-z\d_-]{22}$/u), sourceUrl: z.string(), title: text(2000).refine(value => !!value.trim()), author: text(800).refine(value => !!value.trim()), subscribers: text(200), description: text(16000).refine(value => !!value.trim()), descriptionSource: z.literal("ytInitialPlayerResponse.videoDetails.shortDescription"), displayDescription: text(16000), links: z.array(link).max(100), playabilityStatus: z.literal("OK") }).strict();
export function youtubeVideoEvidencePlan(video: string) {
  const id = youtubeVideoId(video);
  return { site: "onecrew-youtube", name: "video-evidence", args: ["-f", "json", "--", id], addons: ["onecrew-youtube"], maximumBytes: 512000, decode(value: unknown) {
    const parsed = row(evidence, value);
    if (parsed.videoId !== id || parsed.sourceUrl !== `https://www.youtube.com/watch?v=${id}`) throw new OpenCliError("opencli_invalid_output");
    if (parsed.links.some(link => !publicWebUrl(link.url))) throw new OpenCliError("opencli_invalid_output");
    return parsed;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function youtubeChannelContactPlan(channel: string) {
  if (!/^UC[A-Za-z\d_-]{22}$/u.test(channel)) throw new OpenCliError("opencli_invalid_input");
  const contact = z.object({ adapterVersion: z.literal("1.0.0"), channelId: z.literal(channel), sourceUrl: z.literal(`https://www.youtube.com/channel/${channel}`), title: z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= 2000), description: z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= 64000), hiddenEmailAvailable: z.boolean(), aboutDataAvailable: z.boolean() }).strict();
  return { site: "onecrew-youtube", name: "channel-contact", args: ["-f", "json", "--", channel], addons: ["onecrew-youtube"], maximumBytes: 128000, decode: (value: unknown) => row(contact, value) } satisfies ReviewedOpenCliRead<unknown>;
}
