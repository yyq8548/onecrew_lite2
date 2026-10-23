import test from "node:test";
import assert from "node:assert/strict";
import { parseStoredJson } from "../src/contracts/stored-json.ts";
import { projectNativeYoutubeSearch } from "../src/platforms/browser/native-youtube-search.ts";
import { projectNativeInstagramUsers } from "../src/platforms/browser/native-instagram-search.ts";
import { projectNativeXTimeline } from "../src/platforms/browser/native-x-projection.ts";
import { findSearchQueryIdInJavaScript } from "../src/platforms/browser/native-x-search.ts";

test("YouTube native projection retains video order, absent channel metadata, cursor and original u64 counts", () => {
  const result = projectNativeYoutubeSearch({ items: [{ id: { videoId: "second" }, snippet: { channelId: "shared", title: "second" } }, { id: { videoId: "first" }, snippet: { channelId: "shared" } }, { id: { channelId: "missing" } }], nextPageToken: "opaque+/= cursor" }, { items: [{ id: "shared", snippet: { title: "channel", thumbnails: { high: { url: "high" }, medium: { url: "medium" } } }, statistics: { subscriberCount: "18446744073709551615", viewCount: "18446744073709551616", videoCount: "+001" } }] });
  assert.deepEqual(result.items.map(item => item.matched.video_id), ["second", "first", null]);
  assert.equal(result.items[2]!.channel_id, "missing"); assert.equal(result.channels.missing, undefined);
  assert.equal(result.channels.shared!.subscriber_count, 18446744073709551615n); assert.equal(result.channels.shared!.view_count, null); assert.equal(result.channels.shared!.video_count, 1n);
  assert.equal(result.channels.shared!.thumbnail_url, "high"); assert.equal(result.next_page_token, "opaque+/= cursor");
  assert.deepEqual(projectNativeYoutubeSearch({ nextPageToken: "continue" }, {}), { items: [], channels: {}, next_page_token: "continue" });
});
test("Instagram native projection preserves numeric identifiers and i64 fields without rounding or accepting floats as integers", () => {
  const [user] = projectNativeInstagramUsers(parseStoredJson('{"users":[{"pk":9223372036854775807,"username":"local","is_private":false,"follower_count":9223372036854775807,"following_count":3.0,"media_count":-4,"bio_links":[{"url":"one"},{"url":"one"},{"lynx_url":"two"}]}]}'));
  assert.equal(user!.id, "9223372036854775807"); assert.equal(user!.follower_count, 9223372036854775807n); assert.equal(user!.following_count, undefined); assert.equal(user!.media_count, -4n); assert.equal(user!.is_private, false); assert.deepEqual(user!.bio_links, ["one", "two"]);
  assert.throws(() => projectNativeInstagramUsers({ status: "fail" }), /provider_refused/u);
});
test("X native timeline preserves original article, media tie selection, cursor, duplicates and u64 lexical rules", () => {
  const tweet = { rest_id: "post", core: { user_results: { result: { rest_id: "author", core: { name: "new", screen_name: "handle" }, legacy: { name: "old", verified: false }, is_blue_verified: true } } }, legacy: { full_text: "legacy", favorite_count: "18,446,744,073,709,551,615", extended_entities: { media: [{ type: "video", video_info: { variants: [{ content_type: "video/mp4", bitrate: 1, url: "first" }, { content_type: "video/mp4", bitrate: 1, url: "last" }] } }] } }, note_tweet: { note_tweet_results: { result: { text: "note" } } }, article: { article_results: { result: { title: "article", content_state: { blocks: [{ type: "ordered-list-item", text: "one" }, { type: "ordered-list-item", text: "two" }, { type: "atomic", entityRanges: [{ key: 0 }] }], entityMap: [{ key: 0, value: { data: { markdown: "atomic" } } }] } } } } };
  const page = projectNativeXTimeline({ entries: [{ tweet_results: { result: tweet }, promotedMetadata: {} }, { tweet_results: { result: tweet } }, { cursorType: "Bottom", value: "cursor" }] });
  assert.equal(page.items.length, 1); assert.equal(page.items[0]!.promoted, true); assert.equal(page.items[0]!.text, "note"); assert.equal(page.items[0]!.author.name, "new"); assert.equal(page.items[0]!.author.verified, true); assert.equal(page.items[0]!.metrics.likes, 18446744073709551615n); assert.equal(page.items[0]!.media[0]!.url, "last"); assert.equal(page.items[0]!.article!.text, "1. one\n\n2. two\n\natomic"); assert.equal(page.next_cursor, "cursor");
  assert.equal(findSearchQueryIdInJavaScript('const registry={queryId:"real-id",operationName:"SearchTimeline"};'), "real-id");
  assert.equal(findSearchQueryIdInJavaScript('const text="queryId:fake operationName:SearchTimeline";'), null);
});
