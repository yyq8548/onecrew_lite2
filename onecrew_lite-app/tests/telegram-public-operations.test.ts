import assert from "node:assert/strict";
import { test } from "node:test";
import { Chat, tl } from "@mtcute/node";
import { telegramCommunityLocator, telegramPublicUsername, projectTelegramCommunity, projectTelegramAdministrators } from "../src/platforms/browser/telegram-public-operations.ts";

test("原Telegram公开定位拒绝消息/邀请/认证材料，稳定ID保持原i64与规范十进制", () => {
  for (const value of ["https://t.me/name/123", "tg://join?invite=secret", "12345", "", "https://t.me/name?start=secret"]) assert.throws(() => telegramPublicUsername(value), /invalid_community_locator/);
  for (const value of ["@provider_handle", "provider_handle", "https://t.me/provider_handle", "telegram.me/provider_handle"]) assert.equal(telegramPublicUsername(value), "provider_handle");
  for (const id of ["+123", "00123", "-100123", "0", "", "9223372036854775808"]) assert.equal(telegramCommunityLocator.safeParse({ type: "stable_community", external_id: id, kind: "channel" }).success, false);
  assert.equal(telegramCommunityLocator.safeParse({ type: "stable_community", external_id: "123", kind: "channel", access_hash: "SECRET" }).success, false);
});
test("原公开社区投影保留提供方身份/类型/标题，管理员严格删除电话与peer授权", () => {
  const chat = new Chat({ _: "channel", id: 42, title: " Provider title ", username: "provider_handle", megagroup: true, photo: { _: "chatPhotoEmpty" }, date: 1 });
  assert.deepEqual(projectTelegramCommunity(chat, { type: "stable_community", external_id: "42", kind: "supergroup" }), { external_id: "42", title: "Provider title", username: "provider_handle", kind: "supergroup" });
  assert.throws(() => projectTelegramCommunity(chat, { type: "stable_community", external_id: "42", kind: "channel" }), /identity_mismatch/);
  const response: tl.channels.RawChannelParticipants = { _: "channels.channelParticipants", count: 1, participants: [{ _: "channelParticipantCreator", userId: 123, adminRights: { _: "chatAdminRights" } }], users: [{ _: "user", id: 123, username: "person", firstName: "Name", phone: "PHONE_SENTINEL", verified: true }], chats: [] };
  const projected = projectTelegramAdministrators(response);
  assert.deepEqual(projected, [{ role: "owner", user: { id: "123", username: "person", first_name: "Name", last_name: null, is_bot: false, verified: true } }]);
  for (const absent of ["phone", "access_hash", "PHONE_SENTINEL"]) assert.equal(JSON.stringify(projected).includes(absent), false);
  assert.deepEqual(projectTelegramAdministrators({ ...response, participants: [], users: [] }), []);
});
