// Public API only. Identity comes from a native DID, never a display name.
const api = 'https://public.api.bsky.app/xrpc/';
const invalid = () => { throw new Error('ONECREW_BLUESKY_EVIDENCE_INVALID'); };
export function validDid(value) {
  return typeof value === 'string' && value.length <= 256
    && (/^did:plc:[a-z2-7]{24}$/.test(value)
      || /^did:web:[a-zA-Z0-9.-]+(?::[a-zA-Z0-9._-]+)*$/.test(value));
}
export function validHandle(value) {
  return typeof value === 'string' && value.length <= 253 && value.includes('.')
    && value === value.toLowerCase() && value.split('.').every(label =>
      /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label));
}
export function postTarget(uri) {
  if (typeof uri !== 'string') invalid();
  const match = /^at:\/\/([^/]+)\/app\.bsky\.feed\.post\/([a-zA-Z0-9_-]{1,64})$/.exec(uri);
  if (!match || !validDid(match[1])) invalid();
  return {did: match[1], key: match[2], source: `https://bsky.app/profile/${match[1]}/post/${match[2]}`};
}
function text(value, max, required = false) {
  if (typeof value !== 'string' || Buffer.byteLength(value, 'utf8') > max
      || value.includes('\0') || (required && !value.trim())) invalid();
  return value;
}
export function projectProfile(payload, did) {
  if (!validDid(did) || payload?.did !== did || !validHandle(payload.handle)) invalid();
  const count = payload.followersCount;
  if (count !== undefined && (!Number.isSafeInteger(count) || count < 0)) invalid();
  return {did, handle: payload.handle, display_name: text(payload.displayName ?? payload.handle, 800, true),
    biography: text(payload.description ?? '', 8000), follower_count: count ?? null,
    profile_url: `https://bsky.app/profile/${did}`};
}
function nativePost(post) {
  const target = postTarget(post?.uri);
  const record = post?.record;
  if (post?.author?.did !== target.did || !validHandle(post.author.handle)
      || record?.$type !== 'app.bsky.feed.post') invalid();
  const original = text(record.text, 16000, true);
  return {author_id: target.did, author_handle: post.author.handle, post_uri: post.uri,
    source_url: target.source, text: original, public_links: originalLinks(record, original)};
}
function originalLinks(record, original) {
  const links = new Set();
  const add = raw => {
    if (typeof raw !== 'string' || raw.length > 2048 || /[\x00-\x20]/.test(raw)) return;
    let url; try {url = new URL(raw);} catch {return;}
    if (url.username || url.password || url.port
        || [...url.searchParams.keys()].some(key => /^(token|access_token|api_key|authorization|cookie|session|password|secret|auth)$/i.test(key))) return;
    if (url.protocol === 'https:' && url.hostname.includes('.') && !url.hostname.endsWith('.local')
        && !url.hostname.endsWith('.internal') && !/^[0-9.]+$/.test(url.hostname)) links.add(raw);
    if (url.protocol === 'mailto:' && !url.search && !url.hash && /^[^/%?\s]+@[^/%?\s]+\.[^/%?\s]+$/.test(url.pathname)) links.add(raw);
  };
  const facets = record.facets ?? [];
  if (!Array.isArray(facets) || facets.length > 100) invalid();
  const bytes = Buffer.from(original, 'utf8');
  for (const facet of facets) {
    const start = facet?.index?.byteStart, end = facet?.index?.byteEnd;
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end <= start || end > bytes.length
        || !Array.isArray(facet.features) || facet.features.length > 10) invalid();
    // Offsets are UTF-8 bytes, not JS character indices. Partial codepoints
    // cannot attribute a link to an author-written span.
    new TextDecoder('utf-8', {fatal: true}).decode(bytes.subarray(0, start));
    new TextDecoder('utf-8', {fatal: true}).decode(bytes.subarray(start, end));
    for (const feature of facet.features)
      if (feature?.$type === 'app.bsky.richtext.facet#link') add(feature.uri);
  }
  if (record.embed?.$type === 'app.bsky.embed.external') add(record.embed.external?.uri);
  // Never inherit contact links from an embedded quoted author's record.
  if (links.size > 10) invalid();
  return [...links];
}
export function projectThread(payload, expectedUri, includeComments) {
  postTarget(expectedUri);
  const thread = payload?.thread;
  if (thread?.$type !== 'app.bsky.feed.defs#threadViewPost'
      || thread?.post?.uri !== expectedUri) invalid();
  const parent = nativePost(thread.post);
  const comments = [];
  if (includeComments) {
    const replies = thread.replies === undefined ? [] : thread.replies;
    if (!Array.isArray(replies)) invalid();
    const ids = new Set();
    // First five entries in the observed native thread order. No refill after
    // duplicates, blocked entries, or invalid evidence; no nested reply votes.
    for (const reply of replies.slice(0, 5)) {
      if (reply?.$type !== 'app.bsky.feed.defs#threadViewPost') invalid();
      const comment = nativePost(reply.post);
      if (reply.post.record?.reply?.parent?.uri !== expectedUri
          || ids.has(comment.post_uri) || comment.post_uri === expectedUri) invalid();
      ids.add(comment.post_uri);
      comments.push({post_uri: comment.post_uri, source_url: comment.source_url,
        text: text(comment.text, 2000, true)});
    }
  }
  return {...parent, comments};
}
export function projectSearch(payload, limit) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 50
      || !Array.isArray(payload?.posts) || payload.posts.length > limit) invalid();
  const seen = new Set();
  return payload.posts.map((post, index) => {
    const original = nativePost(post);
    if (seen.has(original.post_uri)) invalid();
    seen.add(original.post_uri);
    return {...original, rank: index + 1, title: original.text,
      author: text(post.author.displayName ?? post.author.handle, 800, true),
      author_url: `https://bsky.app/profile/${original.author_id}`,
      provider: 'onecrew-bluesky-native-v1'};
  });
}
async function publicRead(method, parameters, fetchImpl) {
  const url = api + method + '?' + new URLSearchParams(parameters);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 15000);
  let reader;
  try {
    const response = await fetchImpl(url, {redirect: 'manual', credentials: 'omit', signal: controller.signal});
    if (response.status === 401) throw new Error('ONECREW_BLUESKY_AUTH');
    if ([403, 429].includes(response.status)) throw new Error('ONECREW_BLUESKY_RISK');
    if (!response.ok || response.url !== url || response.headers.get('content-type')?.split(';')[0] !== 'application/json') invalid();
    reader = response.body?.getReader();
    if (!reader) invalid();
    const chunks = []; let size = 0;
    while (true) {
      const {done, value} = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 400000) invalid();
      chunks.push(value);
    }
    return JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(Buffer.concat(chunks)));
  } finally {
    clearTimeout(timer);
    if (reader) await reader.cancel().catch(() => {});
  }
}
export async function readEvidence(kind, args, fetchImpl = fetch) {
  if (kind === 'profile') {
    if (!validDid(args.did)) throw new Error('ONECREW_BLUESKY_INPUT');
    return [projectProfile(await publicRead('app.bsky.actor.getProfile', {actor: args.did}, fetchImpl), args.did)];
  }
  if (kind === 'post' || kind === 'comments') {
    postTarget(args.uri);
    const payload = await publicRead('app.bsky.feed.getPostThread',
      {uri: args.uri, depth: kind === 'comments' ? '1' : '0', parentHeight: '0'}, fetchImpl);
    return [projectThread(payload, args.uri, kind === 'comments')];
  }
  if (kind === 'search') {
    if (typeof args.query !== 'string' || !args.query.trim()
        || Buffer.byteLength(args.query) > 500 || /[\x00-\x1f]/.test(args.query)
        || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 50)
      throw new Error('ONECREW_BLUESKY_INPUT');
    return projectSearch(await publicRead('app.bsky.feed.searchPosts',
      {q: args.query, limit: String(args.limit), sort: 'top'}, fetchImpl), args.limit);
  }
  throw new Error('ONECREW_BLUESKY_INPUT');
}
