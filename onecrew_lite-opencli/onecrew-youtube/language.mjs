import { waitForYoutubePage } from './evidence.mjs';

// Native response text only. Translated DOM, recommendations and replies from
// unrelated threads must never become an author's language evidence.
export function projectLanguageComments(data, videoId) {
  const mutations = data?.frameworkUpdates?.entityBatchUpdate?.mutations;
  if (!Array.isArray(mutations)) throw new Error('ONECREW_COMMENTS_INVALID');
  const rows = [], seen = new Set();
  let inspected = 0;
  for (const mutation of mutations) {
    const comment = mutation.payload?.commentEntityPayload;
    if (!comment) continue;
    const id = comment.properties?.commentId;
    const text = comment.properties?.content?.content;
    if (seen.has(id)) continue;
    if (inspected === 5) break;
    inspected += 1;
    seen.add(id);
    if (typeof id !== 'string' || !/^[A-Za-z0-9_.-]{1,200}$/.test(id)
        || typeof text !== 'string' || !text.trim() || text.includes('\0')) continue;
    let bounded = '';
    for (const char of text) {
      if (new TextEncoder().encode(bounded + char).length > 2000) break;
      bounded += char;
    }
    rows.push({ source_url: `https://www.youtube.com/watch?v=${videoId}&lc=${id}`, text: bounded });
  }
  return rows;
}

export function projectRecentVideos(data, channelId) {
  if (data?.metadata?.channelMetadataRenderer?.externalId !== channelId) throw new Error('ONECREW_AUTHOR_MISMATCH');
  const tabs = data.contents?.twoColumnBrowseResultsRenderer?.tabs;
  const contents = tabs?.find(t => t.tabRenderer?.selected)?.tabRenderer?.content?.richGridRenderer?.contents;
  if (!Array.isArray(contents)) throw new Error('ONECREW_RECENT_CONTENT_INVALID');
  const urls = [];
  for (const item of contents) {
    const content = item.richItemRenderer?.content;
    const id = content?.videoRenderer?.videoId
      ?? (content?.lockupViewModel?.contentType === 'LOCKUP_CONTENT_TYPE_VIDEO' ? content.lockupViewModel.contentId : null);
    if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{11}$/.test(id)) continue;
    const url = `https://www.youtube.com/watch?v=${id}`;
    if (!urls.includes(url)) urls.push(url);
    if (urls.length === 3) break;
  }
  return urls;
}

export async function readLanguagePage(videoId, channelId, project) {
  if (location.origin !== 'https://www.youtube.com') throw new Error('ONECREW_ORIGIN_MISMATCH');
  const player = window.ytInitialPlayerResponse;
  const details = player?.videoDetails;
  if (details?.videoId !== videoId || details.channelId !== channelId || player.playabilityStatus?.status !== 'OK') {
    throw new Error('ONECREW_AUTHOR_MISMATCH');
  }
  const description = details.shortDescription;
  if (typeof description !== 'string' || description.includes('\0')) throw new Error('ONECREW_LANGUAGE_TEXT_INVALID');
  // Use the author description; a localized title alone is not language proof.
  let text = '';
  for (const char of description) {
    if (new TextEncoder().encode(text + char).length > 8000) break;
    text += char;
  }
  const items = window.ytInitialData?.contents?.twoColumnWatchNextResults?.results?.results?.contents;
  const section = items?.find(item => item.itemSectionRenderer?.targetId === 'comments-section')?.itemSectionRenderer;
  const token = section?.contents?.find(item => item.continuationItemRenderer)?.continuationItemRenderer?.continuationEndpoint?.continuationCommand?.token;
  let comments = [];
  if (token) {
    const context = window.ytcfg?.data_?.INNERTUBE_CONTEXT;
    if (!context) throw new Error('ONECREW_COMMENTS_INVALID');
    const response = await fetch('/youtubei/v1/next?prettyPrint=false', {
      method: 'POST', credentials: 'include', redirect: 'error', signal: AbortSignal.timeout(15000),
      headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ context, continuation: token }),
    });
    if (!response.ok) throw new Error('ONECREW_COMMENTS_UNAVAILABLE');
    const body = await response.text();
    if (body.length > 2_000_000) throw new Error('ONECREW_COMMENTS_INVALID');
    comments = project(JSON.parse(body), videoId);
  } else if (!Array.isArray(items)) throw new Error('ONECREW_COMMENTS_INVALID');
  return { author_id: channelId, source_url: `https://www.youtube.com/watch?v=${videoId}`, text, comments };
}

export async function readYoutubeLanguage(page, videoId, channelId, restoreOriginal) {
  await page.goto(`https://www.youtube.com/watch?v=${videoId}`, { waitUntil: 'none' });
  await page.evaluate(`(${waitForYoutubePage.toString()})('watch')`);
  await page.evaluate(`(${restoreOriginal.toString()})()`);
  const result = await page.evaluate(`(${readLanguagePage.toString()})(${JSON.stringify(videoId)}, ${JSON.stringify(channelId)}, ${projectLanguageComments.toString()})`);
  const value = result && 'session' in result && 'data' in result ? result.data : result;
  return [value];
}

export async function readYoutubeRecent(page, channelId, restoreOriginal) {
  await page.goto(`https://www.youtube.com/channel/${channelId}/videos`, { waitUntil: 'none' });
  await page.evaluate(`(${waitForYoutubePage.toString()})('channel')`);
  await page.evaluate(`(${restoreOriginal.toString()})()`);
  const result = await page.evaluate(`(${projectRecentVideos.toString()})(window.ytInitialData, ${JSON.stringify(channelId)})`);
  const value = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(value)) throw new Error('ONECREW_RECENT_CONTENT_INVALID');
  return value.map(source_url => ({ source_url }));
}
