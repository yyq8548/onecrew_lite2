const numeric = value => (typeof value === 'string' || Number.isSafeInteger(value))
  && /^[1-9][0-9]{0,19}$/.test(String(value));
const bvid = value => typeof value === 'string' && /^BV[0-9A-Za-z]{10}$/.test(value);
const boundedText = (value, max) => typeof value === 'string' && !value.includes('\0')
  && Buffer.byteLength(value) <= max;

// Preserve transport status before parsing: Bilibili can return HTML for a stop.
export async function readBilibiliApi(page, path, options = {}, sign) {
  const params = options.signed ? await sign(page, options.params ?? {}) : options.params ?? {};
  const url = `https://api.bilibili.com${path}?${new URLSearchParams(params)}`;
  return page.evaluate(`async () => {
    const response = await fetch(${JSON.stringify(url)}, { credentials: 'include' });
    if (response.status === 412 || response.status === 429)
      throw new Error('Bilibili CAPTCHA or rate limit requires human review');
    if (response.status === 401) throw new Error('AUTH_REQUIRED: Bilibili sign-in required');
    if (!response.ok) throw new Error('Bilibili language API HTTP ' + response.status);
    return response.json();
  }`);
}

export function projectBilibiliLanguage(view, replies, expectedMid, expectedBvid) {
  if (!numeric(expectedMid) || !bvid(expectedBvid) || view?.bvid !== expectedBvid
      || !numeric(view?.owner?.mid) || String(view.owner.mid) !== expectedMid
      || !boundedText(view.title, 1000) || !boundedText(view.desc, 7000)
      || !Array.isArray(replies)) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
  const source_url = `https://www.bilibili.com/video/${expectedBvid}`;
  const seen = new Set();
  const comments = replies.slice(0, 5).map(reply => {
    const id = reply.rpid_str ?? reply.rpid;
    if (!numeric(id) || !boundedText(reply.content?.message, 2000)
        || !reply.content.message.trim() || seen.has(String(id)))
      throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
    seen.add(String(id));
    return { source_url: `${source_url}#reply${id}`, text: reply.content.message };
  });
  const text = [view.title, view.desc].filter(Boolean).join('\n');
  if (Buffer.byteLength(text) > 8000 || (!text.trim() && !comments.length))
    throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
  const count=value=>Number.isSafeInteger(value)&&value>=0?value:null;
  return { author_id: expectedMid, source_url, text, comments,
    observations:{published_at:Number.isSafeInteger(view.pubdate)&&view.pubdate>0?new Date(view.pubdate*1000).toISOString():null,
      view_count:count(view.stat?.view),like_count:count(view.stat?.like),comment_count:count(view.stat?.reply),share_count:count(view.stat?.share),save_count:count(view.stat?.favorite),text_completeness:'complete'}};
}

export function projectBilibiliRecent(data, expectedMid) {
  if (!numeric(expectedMid) || !Array.isArray(data?.list?.vlist))
    throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
  const seen = new Set();
  return data.list.vlist.slice(0, 3).map(video => {
    if (!bvid(video.bvid) || !numeric(video.mid) || String(video.mid) !== expectedMid
        || seen.has(video.bvid)) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
    seen.add(video.bvid);
    return { source_url: `https://www.bilibili.com/video/${video.bvid}` };
  });
}

export async function readBilibiliLanguage(page, mid, video, apiGet, ownPage, requireData) {
  if (!numeric(mid) || (video !== 'recent' && !bvid(video)))
    throw new Error('ONECREW_BILIBILI_LANGUAGE_INPUT');
  return ownPage(page, async owned => {
    await owned.goto('https://www.bilibili.com/');
    if (video === 'recent') {
      const data = requireData(await apiGet(owned, '/x/space/wbi/arc/search', {
        params: { mid, pn: 1, ps: 30, order: 'pubdate' }, signed: true,
      }));
      return projectBilibiliRecent(data, mid);
    }
    const view = requireData(await apiGet(owned, '/x/web-interface/view', { params: { bvid: video } }));
    // Check identity before fetching a different person's comment section.
    projectBilibiliLanguage(view, [], mid, video);
    if (!numeric(view.aid)) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
    const data = requireData(await apiGet(owned, '/x/v2/reply/main', {
      params: { oid: view.aid, type: 1, mode: 3, ps: 5 }, signed: true,
    }));
    if (!Object.hasOwn(data, 'replies')) throw new Error('ONECREW_BILIBILI_LANGUAGE_INVALID');
    return [projectBilibiliLanguage(view, data.replies === null ? [] : data.replies, mid, video)];
  });
}
