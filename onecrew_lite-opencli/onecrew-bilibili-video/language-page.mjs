import { readVideoPage, projectVideo } from './video.mjs';

// Only public rendered comments are sampled. A login-limited page can expose a
// smaller sample; this never claims that the entire comment section was read.
export function observePageComments() {
  const root = document.querySelector('bili-comments')?.shadowRoot;
  if (!root?.querySelector('#feed')) return null;
  const threads = [...root.querySelectorAll('bili-comment-thread-renderer')].slice(0, 5);
  if (!threads.length && !root.querySelector('#end')?.textContent.includes('没有更多评论')) return null;
  const comments = [];
  for (const thread of threads) {
    const body = thread.shadowRoot?.querySelector('bili-comment-renderer')?.shadowRoot;
    const text = body?.querySelector('bili-rich-text')?.shadowRoot?.querySelector('#contents')?.textContent?.trim();
    if (!text) continue; // Images and emoji labels do not establish written language.
    comments.push({ text });
  }
  return comments;
}

export function projectPageLanguage(rows, comments, mid, video) {
  const fields = Object.fromEntries(rows.map(row => [row.field, row.value]));
  if (fields.bvid !== video || !fields.author?.endsWith(` (mid: ${mid})`)
      || !/^[1-9][0-9]{0,19}$/.test(mid) || !/^BV[A-Za-z0-9]{10}$/.test(video)
      || !Array.isArray(comments) || comments.length > 5) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
  const source_url = `https://www.bilibili.com/video/${video}`;
  const text = `${fields.title}\n${fields.description}`;
  if (Buffer.byteLength(text) > 8000 || text.includes('\0')) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
  const seen = new Set();
  return { author_id: mid, source_url, text, comments: comments.map(comment => {
    if (typeof comment.text !== 'string'
        || !comment.text.trim() || Buffer.byteLength(comment.text) > 2000 || comment.text.includes('\0'))
      throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
    return { text: comment.text, source_url };
  }).filter(comment => {
    const key = comment.text;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }) };
}

export async function readBilibiliPageLanguage(page, mid, video) {
  await page.goto(`https://www.bilibili.com/video/${video}/`);
  await page.wait({ selector: 'h1.video-title', timeout: 10 });
  const rows = await page.evaluate(`(${readVideoPage.toString()})(${JSON.stringify(video)},${JSON.stringify(mid)},${projectVideo.toString()})`);
  await page.wait({ selector: 'bili-comments', timeout: 10 });
  await page.evaluate(`document.querySelector('bili-comments')?.scrollIntoView({block:'center'})`);
  let comments;
  for (let poll = 0; poll < 8; poll++) {
    comments = await page.evaluate(`(${observePageComments.toString()})()`);
    if (comments !== null) break;
    await page.sleep(1);
  }
  if (comments === null) throw new Error('ONECREW_BILIBILI_LANGUAGE_UNAVAILABLE');
  return [projectPageLanguage(rows, comments, mid, video)];
}
