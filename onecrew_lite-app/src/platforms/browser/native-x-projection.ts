import { isLosslessNumber } from "lossless-json";
const record = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const text = (value: unknown): string => typeof value === "string" ? value : "";
const path = (value: unknown, ...keys: string[]): unknown => keys.reduce<unknown>((current, key) => record(current)[key], value);
const array = (value: unknown): unknown[] => Array.isArray(value) ? value : [];
const boolean = (value: unknown): boolean | null => typeof value === "boolean" ? value : null;
function unsigned(value: unknown): bigint | null {
  const number = isLosslessNumber(value) ? /^\d+$/u.test(value.value) ? BigInt(value.value) : null : typeof value === "bigint" ? value : typeof value === "number" && Number.isSafeInteger(value) ? BigInt(value) : null;
  return number !== null && number >= 0n && number <= 18446744073709551615n ? number : null;
}
function counter(value: unknown): bigint {
  const native = unsigned(value); if (native !== null) return native;
  if (typeof value !== "string") return 0n;
  const normalized = value.replaceAll(",", ""); if (!/^\+?\d+$/u.test(normalized)) return 0n;
  return unsigned(BigInt(normalized)) ?? 0n;
}
export function projectNativeXProfile(payload: unknown) {
  let found: ReturnType<typeof user> | null = null;
  function user(value: unknown) {
    const source = record(value), id = text(source.rest_id);
    if (!id || source.__typename === "UserUnavailable") return null;
    const legacy = record(source.legacy), core = record(source.core), blue = boolean(source.is_blue_verified) ?? false;
    const links: { display_text: string; target_url: string }[] = [];
    for (const section of ["url", "description"]) for (const item of array(path(legacy, "entities", section, "urls"))) {
      const target = text(record(item).expanded_url) || text(record(item).url), display = text(record(item).display_url) || target;
      if (target && !links.some(link => link.target_url === target && link.display_text === display)) links.push({ display_text: display, target_url: target });
    }
    return { id, name: text(core.name) || text(legacy.name), username: text(core.screen_name) || text(legacy.screen_name), bio: text(legacy.description), location: text(path(source, "location", "location")) || text(legacy.location), profile_links: links,
      followers: counter(legacy.followers_count), following: counter(legacy.friends_count), posts: counter(legacy.statuses_count), likes: counter(legacy.favourites_count), verified: blue || (boolean(legacy.verified) ?? false), blue_verified: blue,
      profile_image_url: text(path(source, "avatar", "image_url")) || text(legacy.profile_image_url_https), viewer_follows: boolean(legacy.following), viewer_followed_by: boolean(legacy.followed_by), can_dm: boolean(legacy.can_dm) };
  }
  function visit(value: unknown, depth: number): void {
    if (found) return;
    if (depth > 256) throw new Error("x.response_depth_invalid");
    found = user(value); if (found || value === null || typeof value !== "object") return;
    for (const key of Object.keys(value).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) visit((value as Record<string, unknown>)[key], depth + 1);
  }
  visit(payload, 0); if (!found) throw new Error("x.profile_missing"); return found;
}
function article(value: unknown): { title: string; text: string } | null {
  const result = path(value, "article", "article_results", "result"), blocks = path(result, "content_state", "blocks");
  if (!Array.isArray(blocks)) return null;
  const entityMap = path(result, "content_state", "entityMap"), parts: string[] = []; let ordered = 0;
  for (const block of blocks) {
    const kind = typeof record(block).type === "string" ? text(record(block).type) : "unstyled", content = text(record(block).text);
    if (kind === "atomic") {
      const raw = path(array(record(block).entityRanges)[0], "key"), key = typeof raw === "string" ? raw : unsigned(raw)?.toString();
      const entity = key === undefined ? undefined : Array.isArray(entityMap) ? entityMap.find(item => { const raw = record(item).key; return (typeof raw === "string" ? raw : unsigned(raw)?.toString()) === key; }) : record(entityMap)[key];
      const markdown = path(entity, "value", "data", "markdown"); if (typeof markdown === "string") parts.push(markdown);
      ordered = 0; continue;
    }
    if (!content) continue;
    if (kind !== "ordered-list-item") ordered = 0;
    const prefixes: Record<string, string> = { "header-one": "# ", "header-two": "## ", "header-three": "### ", blockquote: "> ", "unordered-list-item": "- " };
    parts.push(kind === "ordered-list-item" ? `${++ordered}. ${content}` : kind === "code-block" ? `\`\`\`\n${content}\n\`\`\`` : `${prefixes[kind] ?? ""}${content}`);
  }
  return { title: text(record(result).title), text: parts.join("\n\n") };
}
export function projectNativeXTweet(input: unknown) {
  const value = record(input).__typename === "TweetWithVisibilityResults" ? record(input).tweet : input;
  if (record(value).__typename === "TweetTombstone" || !Object.hasOwn(record(value), "legacy")) return null;
  const user = path(value, "core", "user_results", "result"), id = text(record(user).rest_id);
  if (!id || record(user).__typename === "UserUnavailable") return null;
  const userLegacy = record(record(user).legacy), core = record(record(user).core), legacy = record(record(value).legacy), blue = boolean(record(user).is_blue_verified) ?? false;
  const author = { id, name: text(core.name) || text(userLegacy.name), username: text(core.screen_name) || text(userLegacy.screen_name), profile_image_url: text(path(user, "avatar", "image_url")) || text(userLegacy.profile_image_url_https), verified: blue || (boolean(userLegacy.verified) ?? false), blue_verified: blue, viewer_follows: boolean(userLegacy.following), viewer_followed_by: boolean(userLegacy.followed_by), can_dm: boolean(userLegacy.can_dm) };
  const media = array(path(legacy, "extended_entities", "media")).map(item => {
    const kind = text(record(item).type); let url = text(record(item).media_url_https), best: unknown;
    if (kind === "video" || kind === "animated_gif") for (const candidate of array(path(item, "video_info", "variants"))) if (record(candidate).content_type === "video/mp4" && (best === undefined || counter(record(candidate).bitrate) >= counter(record(best).bitrate))) best = candidate;
    if (best !== undefined) url = text(record(best).url);
    return { kind, url, width: unsigned(path(item, "original_info", "width")), height: unsigned(path(item, "original_info", "height")) };
  });
  return { id: text(record(value).rest_id), text: text(path(value, "note_tweet", "note_tweet_results", "result", "text")) || text(legacy.full_text), author,
    metrics: { likes: counter(legacy.favorite_count), retweets: counter(legacy.retweet_count), replies: counter(legacy.reply_count), quotes: counter(legacy.quote_count), views: counter(path(value, "views", "count")), bookmarks: counter(legacy.bookmark_count) },
    created_at: text(legacy.created_at), language: text(legacy.lang), media, urls: array(path(legacy, "entities", "urls")).flatMap(item => typeof record(item).expanded_url === "string" ? [record(item).expanded_url as string] : []), promoted: false,
    article: article(value), in_reply_to_status_id: typeof legacy.in_reply_to_status_id_str === "string" ? legacy.in_reply_to_status_id_str : null, conversation_id: typeof legacy.conversation_id_str === "string" ? legacy.conversation_id_str : null };
}
export function projectNativeXTimeline(payload: unknown) {
  const items: NonNullable<ReturnType<typeof projectNativeXTweet>>[] = []; let nextCursor: string | null = null;
  function visit(value: unknown, depth: number): void {
    if (depth > 256) throw new Error("x.response_depth_invalid");
    if (Array.isArray(value)) { for (const item of value) visit(item, depth + 1); return; }
    if (value === null || typeof value !== "object") return;
    const object = record(value);
    if (object.cursorType === "Bottom") nextCursor = typeof object.value === "string" ? object.value : null;
    if (Object.hasOwn(record(object.tweet_results), "result")) {
      const tweet = projectNativeXTweet(record(object.tweet_results).result);
      if (tweet && !items.some(item => item.id === tweet.id)) { tweet.promoted = Object.hasOwn(object, "promotedMetadata"); items.push(tweet); }
    }
    for (const key of Object.keys(object).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) visit(object[key], depth + 1);
  }
  visit(payload, 0); return { items, next_cursor: nextCursor };
}
