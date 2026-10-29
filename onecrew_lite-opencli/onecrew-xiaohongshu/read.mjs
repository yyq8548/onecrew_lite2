export function xiaohongshuSource(source, kind) {
  const url = new URL(source);
  const path = kind === 'profile' ? /^\/user\/profile\/([a-f0-9]{24})$/ : /^\/explore\/([a-f0-9]{24})$/;
  const id = path.exec(url.pathname)?.[1];
  if (url.origin !== 'https://www.xiaohongshu.com' || url.username || url.password || url.hash || !id
    || kind === 'profile' && url.search || kind === 'content' && ([...url.searchParams.keys()].some(key => !['xsec_token', 'xsec_source'].includes(key))
      || url.searchParams.getAll('xsec_token').length !== 1 || !/^[A-Za-z0-9_+/=-]{1,512}$/.test(url.searchParams.get('xsec_token') || '') || url.searchParams.get('xsec_source') !== 'pc_search')) throw new Error('ONECREW_XIAOHONGSHU_SOURCE_INVALID');
  return id;
}

export function xiaohongshuSearchRows(observed, limit) {
  if (!Array.isArray(observed)) throw new Error('ONECREW_XIAOHONGSHU_SEARCH_INVALID');
  const rows = [], seen = new Set();
  for (const row of observed) {
    const id = xiaohongshuSource(row.url, 'content');
    if (id !== row.id || xiaohongshuSource(row.author_url, 'profile') !== row.author_id
      || typeof row.title !== 'string' || !row.title.trim() || typeof row.author !== 'string' || !row.author.trim()) throw new Error('ONECREW_XIAOHONGSHU_SEARCH_IDENTITY_INVALID');
    if (seen.has(id)) continue;
    seen.add(id); rows.push(row); if (rows.length === limit) break;
  }
  return rows;
}

export function xiaohongshuProfile(observed, author, source) {
  if (xiaohongshuSource(source, 'profile') !== author || xiaohongshuSource(observed.observed_url, 'profile') !== author) throw new Error('ONECREW_XIAOHONGSHU_PROFILE_IDENTITY_INVALID');
  const basic = observed.basic;
  if (typeof basic?.nickname !== 'string' || !basic.nickname.trim() || typeof basic.desc !== 'string' || !Array.isArray(observed.interactions) || !Array.isArray(observed.posts)) throw new Error('ONECREW_XIAOHONGSHU_PROFILE_INVALID');
  const fans = observed.interactions.find(item => item.type === 'fans'), raw = fans?.count;
  const count = typeof raw === 'number' ? raw : typeof raw === 'string' && /^\d+$/.test(raw) ? Number(raw) : null;
  if (count !== null && (!Number.isSafeInteger(count) || count < 0)) throw new Error('ONECREW_XIAOHONGSHU_FOLLOWERS_INVALID');
  if (observed.posts.some(post => post.author_id !== null && post.author_id !== author)) throw new Error('ONECREW_XIAOHONGSHU_PROFILE_IDENTITY_INVALID');
  return { author_id: author, author_label: basic.nickname, profile_url: source, bio: basic.desc, bio_status: basic.desc.trim() ? 'complete' : 'missing',
    follower_count: count, follower_display: typeof fans?.i18nCount === 'string' ? fans.i18nCount : raw === undefined ? null : String(raw),
    identity_source: 'native_profile_page', visible_posts: observed.posts.filter(post => post.author_id === author).map(post => ({ note_id: post.id, title: post.title })).slice(0, 3) };
}

export function xiaohongshuContent(observed, author, source) {
  const id = xiaohongshuSource(source, 'content');
  if (observed.note_id !== id || xiaohongshuSource(observed.observed_url, 'content') !== id
    || observed.author_id !== author || xiaohongshuSource(observed.author_url, 'profile') !== author) throw new Error('ONECREW_XIAOHONGSHU_CONTENT_IDENTITY_INVALID');
  if (typeof observed.author_label !== 'string' || !observed.author_label.trim() || typeof observed.title !== 'string' || typeof observed.text !== 'string'
    || !observed.text.trim() || typeof observed.text_truncated !== 'boolean' || observed.text_completeness_known !== true) throw new Error('ONECREW_XIAOHONGSHU_CONTENT_INVALID');
  return { note_id: id, author_id: author, author_label: observed.author_label, author_url: observed.author_url, source_url: source,
    title: observed.title, text: observed.text, tags: [...new Set(observed.tags)].slice(0, 30), text_truncated: observed.text_truncated, text_completeness_known: true };
}
