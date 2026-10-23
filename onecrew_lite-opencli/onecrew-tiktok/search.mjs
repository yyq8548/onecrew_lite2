// Source projection runs inside the selected official OpenCLI browser page.
export function projectSearch(data, limit) {
  const fail = () => { throw new Error('ONECREW_TIKTOK_INVALID'); };
  if (data?.status_code !== 0 || !Array.isArray(data.data) || data.data.length > 200) fail();
  const rows = [];
  const seen = new Set();
  for (const entry of data.data) {
    if (entry?.type !== 1) continue;
    const video = entry.item;
    const id = video?.id;
    const authorId = video?.author?.id;
    const author = video?.author?.uniqueId;
    const description = video?.desc;
    if (typeof id !== 'string' || !/^[1-9][0-9]{0,24}$/.test(id)
        || typeof authorId !== 'string' || !/^[1-9][0-9]{0,24}$/.test(authorId)
        || typeof author !== 'string' || !/^[A-Za-z0-9_.]{1,24}$/.test(author)
        || typeof description !== 'string' || !description.trim()
        || new TextEncoder().encode(description).length > 16000
        || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(description)) fail();
    if (seen.has(id)) {
      const previous = rows.find(row => row.id === id);
      if (previous.author_id !== authorId || previous.author !== author || previous.desc !== description) fail();
      continue;
    }
    seen.add(id);
    rows.push({ adapter_version: '1.0.0', id, author_id: authorId, author, desc: description,
      url: 'https://www.tiktok.com/@' + author + '/video/' + id });
    if (rows.length === limit) break;
  }
  return rows;
}

export async function readSearchPage(query, limit, project) {
  if (location.origin !== 'https://www.tiktok.com') throw new Error('ONECREW_TIKTOK_ORIGIN');
  if (/verify you are human|verify to continue|complete the puzzle|security verification|too many requests/i.test((document.body?.innerText ?? '').slice(0, 100000))) {
    throw new Error('ONECREW_TIKTOK_RISK');
  }
  const response = await fetch('/api/search/general/full/?' + new URLSearchParams({ keyword: query, offset: '0', count: String(limit), aid: '1988' }), {
    credentials: 'include', redirect: 'error', signal: AbortSignal.timeout(15000),
  });
  if (response.status === 401) throw new Error('ONECREW_TIKTOK_AUTH');
  if (response.status === 429) throw new Error('ONECREW_TIKTOK_RISK');
  if (!response.ok || !response.body) throw new Error('ONECREW_TIKTOK_HTTP');
  const reader = response.body.getReader();
  const decoder = new TextDecoder('utf-8', { fatal: true });
  let size = 0, text = '';
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 4 * 1024 * 1024) throw new Error('ONECREW_TIKTOK_TOO_LARGE');
      text += decoder.decode(value, { stream: true });
    }
    return project(JSON.parse(text + decoder.decode()), limit);
  } finally { await reader.cancel(); }
}

export async function searchTikTok(page, query, limit) {
  if (typeof query !== 'string' || !query.trim() || [...query].length > 160 || /[\u0000-\u001f\u007f]/.test(query)
      || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error('ONECREW_TIKTOK_INPUT');
  // Do not call goto first or retry it: that path reproducibly fails on the
  // reviewed extension. newTab returns the exact identity used for this read.
  const identity = await page.newTab('https://www.tiktok.com/explore');
  if (!identity) throw new Error('ONECREW_TIKTOK_IDENTITY');
  await page.setActivePage(identity);
  await page.wait(3);
  await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
  const result = await page.evaluate(`(${readSearchPage.toString()})(${JSON.stringify(query)}, ${limit}, ${projectSearch.toString()})`);
  const rows = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(rows) || rows.length > limit) throw new Error('ONECREW_TIKTOK_INVALID');
  return rows;
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
