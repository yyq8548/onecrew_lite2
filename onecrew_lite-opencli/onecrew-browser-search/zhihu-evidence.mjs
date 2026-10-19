// Native API projections; never infer an author from a display name.
const fail = () => { throw new Error('ONECREW_ZHIHU_INVALID'); };
const numeric = value => {
  const s = typeof value === 'string' ? value : Number.isSafeInteger(value) ? String(value) : '';
  return /^[1-9][0-9]{0,19}$/.test(s) ? s : fail();
};
const bounded = (value, max) => {
  if (typeof value !== 'string' || value.includes('\0')) return fail();
  let result = ''; let size = 0;
  for (const c of value) { size += Buffer.byteLength(c); if (size > max) break; result += c; }
  return result;
};
export function nativeAuthor(author) {
  if (!author || author.is_anonymous === true || !/^[a-f0-9]{32}$/.test(author.id)
      || typeof author.url_token !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(author.url_token)) return fail();
  return {author_id: author.id, url_token: author.url_token,
    author_label: bounded(author.name, 512), author_url: `https://www.zhihu.com/people/${author.url_token}`};
}
// API HTML is original text. Preserve contact hrefs separately; do not use UI translations.
export function originalHtml(html) {
  if (typeof html !== 'string' || Buffer.byteLength(html) > 300000) return fail();
  const decode = s => s.replace(/&#(x[0-9a-f]+|[0-9]+);/gi, (_, v) => {
    const n = v[0].toLowerCase() === 'x' ? parseInt(v.slice(1), 16) : Number(v);
    return n > 0 && n <= 0x10ffff && !(n >= 0xd800 && n <= 0xdfff) ? String.fromCodePoint(n) : '';
  }).replace(/&nbsp;/g, ' ').replace(/&quot;/g, '"').replace(/&#39;/g, "'")
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
  const clean = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '');
  const links = [];
  for (const match of clean.matchAll(/\bhref\s*=\s*["']([^"']+)["']/gi)) {
    try {
      const url = new URL(decode(match[1]));
      if (url.username || url.password || url.href.length > 2048) continue;
      if (url.protocol !== 'https:' && url.protocol !== 'mailto:') continue;
      if (url.protocol === 'https:' && (!url.hostname.includes('.') || /^(localhost|127\.|10\.|192\.168\.|169\.254\.|\[)/i.test(url.hostname))) continue;
      if (!links.includes(url.href)) links.push(url.href);
      if (links.length === 10) break;
    } catch { /* malformed native link is not a contact */ }
  }
  return {text: bounded(decode(clean.replace(/<\/(p|div|li|h[1-6])>|<br\s*\/?\s*>/gi, '\n').replace(/<[^>]*>/g, '')).trim(), 16000), public_links: links};
}
export function projectProfile(payload, token) {
  const author = nativeAuthor(payload);
  if (author.url_token !== token) return fail();
  const followers = payload.follower_count;
  if (followers !== undefined && followers !== null && (!Number.isSafeInteger(followers) || followers < 0)) return fail();
  return [{...author, follower_count: followers ?? null,
    bio: bounded([payload.headline, payload.description].filter(v => typeof v === 'string').join('\n'), 8000)}];
}
export function projectAnswer(payload, answerId) {
  if (numeric(payload?.id) !== answerId || payload.type !== 'answer') return fail();
  const questionId = numeric(payload.question?.id);
  const author = nativeAuthor(payload.author);
  return [{...author, answer_id: answerId, question_id: questionId,
    source_url: `https://www.zhihu.com/question/${questionId}/answer/${answerId}`,
    ...originalHtml(payload.content)}];
}
export function projectSearch(payload, limit) {
  if (!Array.isArray(payload?.data) || payload.data.length > 20) return fail();
  const rows = [];
  for (const item of payload.data.slice(0, limit)) {
    if (item.type !== 'search_result') continue;
    const object = item.object;
    // Question/column aggregate results are not proof of an answer's author.
    if (!['answer', 'article'].includes(object?.type) || object.author?.is_anonymous === true) continue;
    const author = nativeAuthor(object.author);
    if (object.type === 'article') {
      const articleId = numeric(object.id);
      rows.push({...author, article_id: articleId,
        source_url: `https://zhuanlan.zhihu.com/p/${articleId}`, title: bounded(object.title, 2000)});
      continue;
    }
    const answerId = numeric(object.id), questionId = numeric(object.question?.id);
    rows.push({...author, answer_id: answerId, question_id: questionId,
      source_url: `https://www.zhihu.com/question/${questionId}/answer/${answerId}`,
      title: bounded(object.question?.title ?? object.question?.name ?? '', 2000)});
  }
  return rows;
}

export function projectArticle(payload, articleId) {
  if (numeric(payload?.id) !== articleId || payload.type !== 'article') return fail();
  return [{...nativeAuthor(payload.author), article_id: articleId,
    source_url: `https://zhuanlan.zhihu.com/p/${articleId}`,
    title: bounded(payload.title, 2000), ...originalHtml(payload.content)}];
}

export function projectComments(payload) {
  if (!Array.isArray(payload?.data) || payload.data.length > 20) return fail();
  const seen = new Set();
  return payload.data.slice(0, 5).map(comment => {
    const id = numeric(comment.id);
    if (seen.has(id)) return fail();
    seen.add(id);
    return {id, text: bounded(originalHtml(comment.content).text, 2000)};
  });
}

// Executed inside one owned page. One fixed same-origin request, no retry/paging.
export async function fetchNative(path) {
  if (location.origin !== 'https://www.zhihu.com') return {stop: 'auth'};
  const response = await fetch(path, {credentials: 'include', redirect: 'error', signal: AbortSignal.timeout(15000)});
  if (response.status === 401) return {stop: 'auth'};
  if (response.status === 403 || response.status === 429) return {stop: 'risk', http_status: response.status};
  if (!response.ok) return {stop: 'read'};
  const reader = response.body.getReader(); const chunks = []; let size = 0;
  while (true) {
    const {done, value} = await reader.read(); if (done) break;
    size += value.byteLength;
    if (size > 400000) { await reader.cancel(); return {stop: 'read'}; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(bytes));
}
export async function readEvidence(page, args, kind, ownPage) {
  let path;
  if (kind === 'profile' && typeof args.id === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(args.id)) {
    path = `/api/v4/members/${encodeURIComponent(args.id)}?include=follower_count,headline,description`;
  } else if (kind === 'answer' && typeof args.id === 'string' && /^[1-9][0-9]{0,19}$/.test(args.id)) {
    path = `/api/v4/answers/${args.id}?include=content,author,question,url`;
  } else if (kind === 'article' && typeof args.id === 'string' && /^[1-9][0-9]{0,19}$/.test(args.id)) {
    path = `/api/v4/articles/${args.id}?include=content,author,title,url`;
  } else if (['answer-comments', 'article-comments'].includes(kind) && typeof args.id === 'string' && /^[1-9][0-9]{0,19}$/.test(args.id)) {
    path = `/api/v4/comment_v5/${kind === 'answer-comments' ? 'answers' : 'articles'}/${args.id}/root_comment?limit=20&order_by=score`;
  } else if (kind === 'search' && typeof args.query === 'string' && args.query.trim()
      && args.query.length <= 160 && !/[\x00-\x1f]/.test(args.query)
      && Number.isInteger(args.limit) && args.limit >= 1 && args.limit <= 20) {
    path = `/api/v4/search_v3?q=${encodeURIComponent(args.query)}&t=general&offset=0&limit=20`;
  } else throw new Error('ONECREW_ZHIHU_INPUT');
  return ownPage(page, async owned => {
    await owned.goto('https://www.zhihu.com/');
    const raw = await owned.evaluate(`(${fetchNative.toString()})(${JSON.stringify(path)})`);
    const data = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    if (data?.stop) {
      const error = new Error(`ONECREW_ZHIHU_${String(data.stop).toUpperCase()}`);
      if (data.stop === 'risk' && [403, 429].includes(data.http_status)) error.httpStatus = data.http_status;
      throw error;
    }
    return kind === 'profile' ? projectProfile(data, args.id)
      : kind === 'answer' ? projectAnswer(data, args.id)
      : kind === 'article' ? projectArticle(data, args.id)
      : kind.endsWith('-comments') ? projectComments(data) : projectSearch(data, args.limit);
  });
}
