import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { additionalNativeContentBound, additionalNativeReviewBound } from './discovery-native-content';

const native = JSON.parse(readFileSync(new URL('../../../onecrew_lite-app/tests/fixtures/xiaohongshu-content-live-20261027.json', import.meta.url), 'utf8'))[0];
const claim = { platform: 'xiaohongshu', author_id: native.author_id, source_url: native.source_url };

describe('小红书真实正文的网页证据合同', () => {
  it('保留原生作者与笔记绑定', () => {
    expect(additionalNativeContentBound(claim, native)).toBe(true);
    expect(additionalNativeReviewBound('xiaohongshu', claim, native, native.author_id, native.author_url, native.source_url)).toBe(true);
  });
  it('拒绝另一个作者、评论者主页及其他笔记地址', () => {
    expect(additionalNativeContentBound(claim, { ...native, author_id: '0'.repeat(24) })).toBe(false);
    expect(additionalNativeReviewBound('xiaohongshu', claim, native, native.author_id, native.author_url + '/comments', native.source_url)).toBe(false);
    expect(additionalNativeContentBound({ ...claim, source_url: native.author_url }, native)).toBe(false);
  });
});
