export function zhihuIdentity(author) {
  if (!author || !/^[a-f0-9]{32}$/.test(author.id) || !/^[A-Za-z0-9_-]{1,128}$/.test(author.url_token)
    || typeof author.name !== 'string' || !author.name.trim()) throw new Error('ONECREW_ZHIHU_INVALID_AUTHOR');
  return { author_id: author.id, url_token: author.url_token, author_label: author.name,
    author_url: `https://www.zhihu.com/people/${author.url_token}` };
}

export function zhihuProfile(data, expectedId, token) {
  const identity = zhihuIdentity(data);
  if (identity.author_id !== expectedId || identity.url_token !== token) throw new Error('ONECREW_ZHIHU_INVALID_PROFILE_OWNER');
  const followers = data.follower_count ?? null;
  if (followers !== null && (!Number.isSafeInteger(followers) || followers < 0)) throw new Error('ONECREW_ZHIHU_INVALID_FOLLOWERS');
  const bio = [data.headline, data.description].filter(value => typeof value === 'string' && value.trim()).join('\n');
  const bio_status = typeof data.headline === 'string' && typeof data.description === 'string' ? bio.trim() ? 'complete' : 'missing' : 'unknown';
  return { ...identity, follower_count: followers, bio, bio_status };
}

export function zhihuSearchRows(data, limit) {
  if (!Array.isArray(data?.data)) throw new Error('ONECREW_ZHIHU_INVALID_SEARCH_RESPONSE');
  const rows = [], seen = new Set();
  const id = value => typeof value === 'string' && /^[1-9][0-9]{0,19}$/.test(value) ? value : Number.isSafeInteger(value) && value > 0 ? String(value) : null;
  for (const item of data.data) {
    const entry = item?.object;
    if (item?.type !== 'search_result' || !['answer', 'article'].includes(entry?.type)) continue;
    // 匿名内容不能证明创作者身份。
    if (!entry.author || entry.author.type === 'anonymous' || !entry.author.url_token) continue;
    const identity = zhihuIdentity(entry.author), contentId = id(entry.id), questionId = entry.type === 'answer' ? id(entry.question?.id) : null;
    const title = entry.title || entry.question?.name || entry.question?.title;
    if (!contentId || entry.type === 'answer' && !questionId || typeof title !== 'string' || !title.trim()) throw new Error('ONECREW_ZHIHU_INVALID_SEARCH_IDENTITY');
    const url = entry.type === 'answer' ? `https://www.zhihu.com/question/${questionId}/answer/${contentId}` : `https://zhuanlan.zhihu.com/p/${contentId}`;
    if (seen.has(url)) continue;
    seen.add(url); rows.push({ ...identity, author: identity.author_label, title, url });
    if (rows.length === limit) break;
  }
  return rows;
}

// 只读取当前已登录浏览器中的同站接口；保留状态与平台错误，不重试或绕过拒绝。
export async function fetchZhihuPage(url) {
  const response = await fetch(url, { credentials: 'include', signal: AbortSignal.timeout(15000) });
  const body = await response.text();
  let data;
  if ((response.headers.get('content-type') || '').includes('json')) {
    data = JSON.parse(body, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^\d+$/.test(context.source) ? context.source : value);
  }
  if (!response.ok) return { failure: { status: response.status, code: data?.error?.code ?? null,
    message: String(data?.error?.message ?? data?.message ?? response.statusText).slice(0, 500) } };
  if (!data || typeof data !== 'object') return { failure: { status: response.status, code: 'INVALID_JSON', message: 'Expected JSON response' } };
  if (data.error) return { failure: { status: response.status, code: data.error.code ?? null, message: String(data.error.message || 'API error').slice(0, 500) } };
  // DOMParser 解析页面正文与简介，不手写 HTML 解析器。
  const plain = html => {
    const document = new DOMParser().parseFromString(html, 'text/html');
    for (const node of document.querySelectorAll('br,p,div,li,h1,h2,h3,h4,blockquote,pre,tr,a')) node.after('\n');
    return document.body.textContent.trim();
  };
  if (typeof data.content === 'string') {
    const document = new DOMParser().parseFromString(data.content, 'text/html');
    data.plain_text = document.body.textContent.trim();
    data.public_links = [...new Set([...document.querySelectorAll('a[href]')].map(anchor => {
      const url = new URL(anchor.getAttribute('href'), 'https://www.zhihu.com');
      return url.hostname === 'link.zhihu.com' ? url.searchParams.get('target') : url.href;
    }).filter(value => value && /^(https:\/\/|mailto:)/.test(value)))].slice(0, 10);
  }
  if (typeof data.description === 'string') data.description = plain(data.description);
  if (Array.isArray(data.data)) for (const item of data.data) {
    const entry = item?.object;
    if (typeof entry?.title === 'string') entry.title = plain(entry.title);
    if (typeof entry?.question?.name === 'string') entry.question.name = plain(entry.question.name);
    if (typeof entry?.question?.title === 'string') entry.question.title = plain(entry.question.title);
  }
  return { data };
}

export function zhihuContent(data, source, expectedId) {
  const identity = zhihuIdentity(data.author);
  if (identity.author_id !== expectedId) throw new Error('ONECREW_ZHIHU_INVALID_CONTENT_OWNER');
  const answer = /^https:\/\/www\.zhihu\.com\/question\/([1-9][0-9]{0,19})\/answer\/([1-9][0-9]{0,19})$/.exec(source);
  const article = /^https:\/\/zhuanlan\.zhihu\.com\/p\/([1-9][0-9]{0,19})$/.exec(source);
  if ((!answer && !article) || String(data.id) !== (answer?.[2] ?? article?.[1]) || answer && String(data.question?.id) !== answer[1]) throw new Error('ONECREW_ZHIHU_INVALID_CONTENT_IDENTITY');
  if (typeof data.plain_text !== 'string' || !data.plain_text.trim() || data.content_need_truncated === true) throw new Error('ONECREW_ZHIHU_INVALID_CONTENT_INCOMPLETE');
  return { ...identity, source_url: source, text: data.plain_text, public_links: data.public_links,
    ...(answer ? { answer_id: answer[2], question_id: answer[1] } : { article_id: article[1], title: data.title }) };
}

export async function observeZhihuContent(source) {
  if (location.origin + location.pathname !== source) throw new Error('ONECREW_ZHIHU_CONTENT_ROUTE');
  const answer = /^https:\/\/www\.zhihu\.com\/question\/(\d+)\/answer\/(\d+)$/.exec(source);
  const article = /^https:\/\/zhuanlan\.zhihu\.com\/p\/(\d+)$/.exec(source);
  if (!answer && !article) throw new Error('ONECREW_ZHIHU_INVALID_CONTENT_URL');
  const id = answer?.[2] ?? article[1];
  for (let attempt = 0; attempt < 20; attempt++) {
    const script = document.querySelector('#js-initialData');
    if (script) {
      const initial = JSON.parse(script.textContent, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^\d+$/.test(context.source) ? context.source : value);
      const entities = initial.initialState?.entities;
      const entry = (answer ? entities?.answers : entities?.articles)?.[id];
      if (entry) {
        if (entry.contentNeedTruncated === true || typeof entry.content !== 'string' || !entry.content.trim()) throw new Error('ONECREW_ZHIHU_INVALID_CONTENT_INCOMPLETE');
        const body = new DOMParser().parseFromString(entry.content, 'text/html');
        const links = [...body.querySelectorAll('a[href]')].map(anchor => {
          const url = new URL(anchor.getAttribute('href'), source);
          return url.hostname === 'link.zhihu.com' ? url.searchParams.get('target') : url.href;
        }).filter(value => value && /^(https:\/\/|mailto:)/.test(value));
        for (const node of body.querySelectorAll('br,p,div,li,h1,h2,h3,h4,blockquote,pre,tr,a')) node.after('\n');
        return { id: entry.id, title: entry.title, question: answer ? { id: typeof entry.question === 'object' ? entry.question.id : entry.question } : undefined,
          author: { id: entry.author?.id, name: entry.author?.name, url_token: entry.author?.urlToken },
          plain_text: body.body.textContent.trim(), public_links: [...new Set(links)].slice(0, 10), content_need_truncated: false };
      }
    }
    if (attempt < 19) await new Promise(resolve => setTimeout(resolve, 250));
  }
  throw new Error('ONECREW_ZHIHU_CONTENT_UNAVAILABLE');
}
