// Public Substack search endpoint; internal API, bounded and validated per post.
// Custom publication domains are observed canonical sources, never creator proof.
export function projectPosts(data, limit) {
  if (!data || !Array.isArray(data.results)) throw new Error('SUBSTACK_SHAPE');
  return data.results.slice(0, limit).map(post => {
    const id = value => Number.isSafeInteger(value) && value > 0 ? String(value) : null;
    const post_id = id(post.id), publication_id = id(post.publication_id);
    const slug = post.slug;
    let url;
    try { url = new URL(post.canonical_url); } catch { throw new Error('SUBSTACK_SOURCE'); }
    if (!post_id || !publication_id || typeof slug !== 'string' || !/^[a-zA-Z0-9_-]+$/.test(slug)
      || url.protocol !== 'https:' || url.username || url.password || url.port || url.hash
      || url.pathname !== `/p/${slug}` || !url.hostname.includes('.')
      || /(?:^|\.)(?:localhost|local|internal|test)$/.test(url.hostname)
      || /^\d+(?:\.\d+){3}$/.test(url.hostname)
      || typeof post.title !== 'string' || !post.title.trim()) throw new Error('SUBSTACK_SOURCE');
    return {provider:'onecrew-substack-v1', post_id, publication_id, slug,
      title:post.title.replace(/\s+/g,' ').trim().slice(0,500),
      author:typeof post.publishedBylines?.[0]?.name === 'string' ? post.publishedBylines[0].name.slice(0,200) : '',
      url:url.href};
  });
}

export async function searchPosts(query, limit, request = fetch) {
  if (typeof query !== 'string' || !query.trim() || [...query].length > 160
    || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error('SUBSTACK_INPUT');
  const url = new URL('https://substack.com/api/v1/post/search');
  url.searchParams.set('query',query); url.searchParams.set('page','0');
  url.searchParams.set('includePlatformResults','true');
  const response = await request(url,{signal:AbortSignal.timeout(15000),redirect:'error',headers:{Accept:'application/json'}});
  if (!response.ok) throw new Error(response.status === 429 ? 'SUBSTACK_RISK' : 'SUBSTACK_HTTP');
  // Bound the body before decoding; never fetch result URLs or arbitrary hosts.
  const reader = response.body.getReader(); const chunks = []; let size = 0;
  try {
    for (;;) { const {done,value} = await reader.read(); if (done) break;
      size += value.length; if (size > 4*1024*1024) throw new Error('SUBSTACK_SIZE'); chunks.push(value); }
  } finally { await reader.cancel(); }
  const bytes = new Uint8Array(size); let offset = 0;
  for (const chunk of chunks) {bytes.set(chunk,offset); offset += chunk.length;}
  return projectPosts(JSON.parse(new TextDecoder().decode(bytes)),limit);
}
