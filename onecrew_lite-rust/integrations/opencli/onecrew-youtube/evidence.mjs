// This module is deliberately dependency-free. Only its bounded projection crosses
// the official OpenCLI browser bridge; full HTML/player/network state never does.
export function projectWatchHtml(html, videoId) {
  const fail = () => { throw new Error('ONECREW_VIDEO_EVIDENCE_INVALID'); };
  if (!/^[A-Za-z0-9_-]{11}$/.test(videoId) || typeof html !== 'string'
      || html.length > 8 * 1024 * 1024) fail();
  function bootstrap(name) {
    const pattern = new RegExp('(?:var\\s+' + name + '|window\\["' + name
      + '"\\]|window\\.' + name + ')\\s*=\\s*', 'g');
    const match = pattern.exec(html);
    if (!match || html[pattern.lastIndex] !== '{') fail();
    const start = pattern.lastIndex;
    let depth = 0, quoted = false, escaped = false;
    for (let i = start; i < html.length; i++) {
      const c = html[i];
      if (quoted) {
        if (escaped) escaped = false;
        else if (c === '\\') escaped = true;
        else if (c === '"') quoted = false;
      } else if (c === '"') quoted = true;
      else if (c === '{') depth++;
      else if (c === '}' && --depth === 0) {
        try { return JSON.parse(html.slice(start, i + 1)); } catch { fail(); }
      }
    }
    fail();
  }
  function text(value, maxBytes, required = false) {
    if (typeof value !== 'string' || value.includes('\0')
        || new TextEncoder().encode(value).length > maxBytes
        || (required && !value.trim())) fail();
    return value;
  }
  const player = bootstrap('ytInitialPlayerResponse');
  const initial = bootstrap('ytInitialData');
  const details = player.videoDetails;
  if (!details || details.videoId !== videoId
      || !/^UC[A-Za-z0-9_-]{22}$/.test(details.channelId)
      || player.playabilityStatus?.status !== 'OK') fail();
  const contents = initial.contents?.twoColumnWatchNextResults?.results?.results?.contents;
  if (!Array.isArray(contents)) fail();
  const secondary = contents.filter(c => c.videoSecondaryInfoRenderer)
    .map(c => c.videoSecondaryInfoRenderer);
  if (secondary.length !== 1) fail();
  const section = secondary[0];
  const owner = section.owner?.videoOwnerRenderer;
  if (owner?.navigationEndpoint?.browseEndpoint?.browseId !== details.channelId) fail();
  const description = text(details.shortDescription, 16000, true);
  const displayDescription = text(section.attributedDescription?.content ?? '', 16000);
  const links = [];
  function observe(rawUrl, label, source) {
    if (typeof rawUrl !== 'string' || rawUrl.length > 2048
        || rawUrl.includes('...') || rawUrl.includes('…')) return;
    let url;
    try {
      url = new URL(rawUrl, 'https://www.youtube.com');
      if (url.origin === 'https://www.youtube.com' && url.pathname === '/redirect') {
        const targets = url.searchParams.getAll('q');
        if (targets.length !== 1) return;
        url = new URL(targets[0]);
      }
    } catch { return; }
    if (url.protocol !== 'https:' || url.username || url.password || url.port
        || !url.hostname.includes('.') || /[\s…]/.test(url.href)
        || url.href.includes('...')) return;
    if ([...url.searchParams.keys()].some(key =>
      /^(access_token|refresh_token|id_token|api_key|password|authorization)$/i.test(key))) return;
    if (links.some(link => link.url === url.href && link.source === source)) return;
    if (links.length >= 100) fail();
    links.push({ url: url.href, label: text(label, 2048), source });
  }
  for (const token of description.split(/\s+/)) {
    if (!token.includes('...') && !token.includes('…')) {
      const clean = token.replace(/^[([<{"']+/, '').replace(/[)\]}>"',.;]+$/, '');
      if (clean.startsWith('https://')) observe(clean, clean, 'shortDescription');
    }
  }
  // Only description runs, never a recursive walk through recommendations/comments.
  for (const run of section.description?.runs ?? []) {
    observe(run.navigationEndpoint?.urlEndpoint?.url, run.text ?? '', 'description.runs');
  }
  for (const run of section.attributedDescription?.commandRuns ?? []) {
    const start = run.startIndex, length = run.length;
    if (!Number.isInteger(start) || !Number.isInteger(length) || start < 0 || length < 1
        || start + length > displayDescription.length) fail();
    observe(run.onTap?.innertubeCommand?.urlEndpoint?.url,
      displayDescription.slice(start, start + length), 'attributedDescription.commandRuns');
  }
  return {
    adapterVersion: '1.0.0', videoId, channelId: details.channelId,
    sourceUrl: 'https://www.youtube.com/watch?v=' + videoId,
    title: text(details.title, 2000, true), author: text(details.author, 800, true),
    subscribers: text(owner.subscriberCountText?.simpleText ?? '', 200),
    description, descriptionSource: 'ytInitialPlayerResponse.videoDetails.shortDescription',
    displayDescription, links, playabilityStatus: 'OK',
  };
}

export async function readWatchPage(videoId, project) {
  if (location.origin !== 'https://www.youtube.com') throw new Error('ONECREW_ORIGIN_MISMATCH');
  const response = await fetch('/watch?v=' + encodeURIComponent(videoId), {
    credentials: 'include', redirect: 'error', signal: AbortSignal.timeout(20000),
  });
  if (!response.ok || !response.body) throw new Error('ONECREW_WATCH_UNAVAILABLE');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let bytes = 0, html = '';
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      bytes += chunk.value.byteLength;
      if (bytes > 8 * 1024 * 1024) throw new Error('ONECREW_WATCH_TOO_LARGE');
      html += decoder.decode(chunk.value, { stream: true });
    }
    return project(html + decoder.decode(), videoId);
  } finally { await reader.cancel(); }
}

export async function readVideoEvidence(page, videoId) {
  if (typeof videoId !== 'string' || !/^[A-Za-z0-9_-]{11}$/.test(videoId)) {
    throw new Error('ONECREW_VIDEO_ID_INVALID');
  }
  await page.goto('https://www.youtube.com', { waitUntil: 'none' });
  await page.wait(2);
  await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
  const result = await page.evaluate(
    `(${readWatchPage.toString()})(${JSON.stringify(videoId)}, ${projectWatchHtml.toString()})`,
  );
  const data = result && 'session' in result && 'data' in result ? result.data : result;
  if (!data || data.videoId !== videoId || data.adapterVersion !== '1.0.0') {
    throw new Error('ONECREW_VIDEO_EVIDENCE_INVALID');
  }
  return [data];
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
