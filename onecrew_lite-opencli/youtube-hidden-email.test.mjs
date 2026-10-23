import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { YoutubeHiddenEmailStore } from './onecrew-youtube/hidden-email-store.mjs';
import { youtubeDailyEmailLimit, youtubePublicEmails } from './onecrew-youtube/hidden-email.mjs';

const root = fileURLToPath(new URL('../.scratch/', import.meta.url));
mkdirSync(root, { recursive: true });
const reason = '已隐藏电子邮件地址。你已达到今日的访问次数限制。';
test('查看上限只阻止本次任务，重启仍保留状态且新任务不受旧任务限制', () => {
  const directory = mkdtempSync(root + 'youtube-email-'), now = Date.now();
  const first = new YoutubeHiddenEmailStore(directory), second = new YoutubeHiddenEmailStore(directory);
  const active = first.claim('current-search',now); assert.equal(active.status, 'ready');
  assert.equal(second.claim('current-search',now).status, 'busy');
  assert.equal(first.noteDailyLimit('current-search',now, reason).limited_at, now);
  first.release(active.lease); first.close(); second.close();
  const restarted = new YoutubeHiddenEmailStore(directory);
  assert.equal(restarted.claim('current-search',now).status, 'task_limited');
  assert.equal(restarted.gate('current-search').reason, reason);
  const available = restarted.claim('next-search',now);
  assert.equal(available.status, 'ready'); restarted.release(available.lease); restarted.close();
});
test('已观察的商务邮箱持久保存，后续限额或验证状态不会删除邮箱', () => {
  const directory = mkdtempSync(root + 'youtube-email-observation-');
  const store = new YoutubeHiddenEmailStore(directory), channel = 'UCL389N7dU2raGkfWtFipbIQ';
  store.record(channel, 'revealed', 'promisemonster216183@gmail.com', null);
  store.record(channel, 'task_limited', null, reason); store.close();
  const reopened = new YoutubeHiddenEmailStore(directory);
  assert.equal(reopened.observation(channel).email, 'promisemonster216183@gmail.com');
  assert.equal(reopened.observation(channel).status, 'task_limited'); reopened.close();
});
test('实际简介和限制文字分别识别，简介邮箱保持优先', () => {
  assert.deepEqual(youtubePublicEmails('㊣聯繫/合作請洽詢 : assassinyue2020@gmail.com'), ['assassinyue2020@gmail.com']);
  assert.equal(youtubeDailyEmailLimit(reason), true);
  assert.equal(youtubeDailyEmailLimit('查看电子邮件地址'), false);
  assert.equal(youtubeDailyEmailLimit('进行人机身份验证'), false);
});
