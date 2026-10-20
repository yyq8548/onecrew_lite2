import assert from "node:assert/strict";
import { test } from "node:test";
import { youtubeChannelPlan, youtubeTranscriptPlan, youtubeVideoId, youtubeVideoPlan, youtubeRecentLanguagePlan } from "../src/platforms/opencli/youtube.ts";
import { youtubeChannelContactPlan, youtubeVideoEvidencePlan } from "../src/platforms/opencli/youtube-evidence.ts";
import { publicOpenCliAddress, publicWebUrl } from "../src/platforms/opencli/public-url.ts";

test("YouTube原字段合同保留频道、订阅文本、视频ID及字幕时间，拒绝重复字段", () => {
  const channel = [{ field: "name", value: "AI Maker" }, { field: "channelId", value: "UC1234567890" }, { field: "subscribers", value: "12.4K subscribers" }, { field: "url", value: "https://www.youtube.com/channel/UC1234567890" }];
  assert.equal(youtubeChannelPlan("@maker").decode(channel).subscribers_raw, "12.4K subscribers");
  assert.throws(() => youtubeChannelPlan("@maker").decode([...channel, channel[0]]));
  assert.equal(youtubeVideoId("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30s"), "dQw4w9WgXcQ");
  assert.throws(() => youtubeVideoId("https://www.youtube.com/watch?v=dQw4w9WgXcQ&v=dQw4w9WgXcQ"));
  assert.throws(() => youtubeVideoId("https://example.com/watch?v=dQw4w9WgXcQ"));
  assert.throws(() => youtubeVideoPlan("dQw4w9WgXcQ").decode([{ field: "videoId", value: "abcdefghijk" }]));
  assert.deepEqual(youtubeTranscriptPlan("dQw4w9WgXcQ").decode([{ timestamp: "0:10", speaker: "", text: "I open the AI tool." }]), [{ timestamp: "0:10", speaker: "", text: "I open the AI tool." }]);
  assert.throws(() => youtubeTranscriptPlan("dQw4w9WgXcQ").decode([{ timestamp: "soon", text: "demo" }]));
});

test("原生YouTube证据保留原description，隐藏邮件仅保留可见入口事实", () => {
  const value = { adapterVersion: "1.0.0", videoId: "abcdefghijk", channelId: "UCaaaaaaaaaaaaaaaaaaaaaa", sourceUrl: "https://www.youtube.com/watch?v=abcdefghijk", title: "Tool walkthrough", author: "Creator", subscribers: "12K subscribers", description: "I remove pauses with https://www.descript.com/", descriptionSource: "ytInitialPlayerResponse.videoDetails.shortDescription", displayDescription: "https://www.descript.com/…", playabilityStatus: "OK", links: [{ url: "https://www.descript.com/", label: "Descript", source: "shortDescription" }] };
  const plan = youtubeVideoEvidencePlan("abcdefghijk");
  assert.equal(plan.decode([value]).description, value.description);
  assert.throws(() => plan.decode([{ ...value, channelId: "@not-stable" }]));
  assert.throws(() => plan.decode([{ ...value, links: [{ ...value.links[0], url: "https://127.0.0.1/" }] }]));
  assert.throws(() => plan.decode([{ ...value, description: "字".repeat(5334) }]));
  const contact = youtubeChannelContactPlan(value.channelId, "youtube-channel-contract").decode([{ adapterVersion: "1.0.0", channelId: value.channelId, sourceUrl: `https://www.youtube.com/channel/${value.channelId}`, title: "Creator", description: "Public description", hiddenEmailAvailable: true, aboutDataAvailable: true }]);
  assert.equal(contact.hiddenEmailAvailable, true);
  assert.equal("email" in contact, false);
  assert.throws(() => youtubeRecentLanguagePlan(value.channelId).decode([{ source_url: value.sourceUrl }, { source_url: value.sourceUrl }]));
});

test("原公开URL规则拒绝截断、凭据和私网地址，使用Node成熟IP处理", () => {
  for (const value of ["https://127.0.0.1/", "https://example.com/...", "https://user:pass@example.com/", "https://example.com/?access_token=private", "https://example.test/", "https://example.com/#password=private"]) assert.equal(publicWebUrl(value), null);
  assert.equal(publicWebUrl("https://example.com/article")!.hostname, "example.com");
  for (const value of ["10.0.0.1", "127.0.0.1", "169.254.1.1", "100.64.0.1", "192.0.2.1", "198.18.1.1", "224.0.0.1", "::1", "::ffff:8.8.8.8", "2001:db8::1", "2002::1", "3fff::1"]) assert.equal(publicOpenCliAddress(value), false, value);
  assert.equal(publicOpenCliAddress("8.8.8.8"), true);
  assert.equal(publicOpenCliAddress("2606:4700:4700::1111"), true);
});
