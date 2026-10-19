// Read-only public endpoints. Retry transient transport failures, never a
// challenge, authorization failure, changed data shape or business write.
export async function readPublic(url, { fetcher = fetch, sleep = ms => new Promise(resolve => setTimeout(resolve, ms)) } = {}) {
  for (let attempt = 0; attempt < 3; attempt++) {
    let response;
    try { response = await fetcher(url, { signal: AbortSignal.timeout(15_000), redirect: 'error' }); }
    catch { if (attempt < 2) { await sleep(250 * (attempt + 1)); continue; } throw Error('PUBLIC_TRANSPORT'); }
    if ([403, 429].includes(response.status)) throw Error('PUBLIC_RISK');
    if (response.status === 401) throw Error('PUBLIC_AUTH');
    if (response.status === 404) throw Error('PUBLIC_SCOPE');
    if ([502, 503, 504].includes(response.status) && attempt < 2) { await sleep(250 * (attempt + 1)); continue; }
    if (!response.ok) throw Error('PUBLIC_HTTP');
    const type = response.headers.get('content-type') || '';
    if (!/json|xml|atom/i.test(type)) throw Error('PUBLIC_SHAPE');
    const reader = response.body?.getReader();
    if (!reader) throw Error('PUBLIC_SHAPE');
    const chunks = []; let bytes = 0;
    try {
      while (true) {
        const { done, value } = await reader.read(); if (done) break;
        bytes += value.byteLength; if (bytes > 2_000_000) throw Error('PUBLIC_SIZE'); chunks.push(value);
      }
    } finally { await reader.cancel(); }
    const buffer = new Uint8Array(bytes); let offset = 0;
    for (const chunk of chunks) { buffer.set(chunk, offset); offset += chunk.byteLength; }
    return new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  }
}

export function lobstersRows(text, limit) {
  const rows = JSON.parse(text);
  if (!Array.isArray(rows) || rows.length > 1000) throw Error('PUBLIC_SHAPE');
  return rows.slice(0, limit).map((row, index) => ({ rank: index + 1, id: row.short_id,
    title: row.title, author: row.submitter_user, url: row.comments_url }));
}
