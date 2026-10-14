// Only the focal post's native author is evidence. Replies, quoted posts and
// today's handle-to-profile lookup cannot establish ownership of an older post.
export function projectPostAuthor(data, handle, postId) {
  const fail = () => { throw new Error('ONECREW_X_POST_IDENTITY_INVALID'); };
  if (data?.errors != null && (!Array.isArray(data.errors) || data.errors.length)) fail();
  const instructions = data?.data?.threaded_conversation_with_injections_v2?.instructions;
  if (!Array.isArray(instructions)) fail();
  const found = [];
  const inspect = result => {
    const tweet = result?.__typename === 'TweetWithVisibilityResults' ? result.tweet : result;
    if (tweet?.rest_id === postId) found.push(tweet);
  };
  for (const instruction of instructions) {
    for (const entry of instruction.entries ?? []) {
      inspect(entry.content?.itemContent?.tweet_results?.result);
      for (const item of entry.content?.items ?? [])
        inspect(item.item?.itemContent?.tweet_results?.result);
    }
  }
  if (found.length !== 1) fail();
  const tweet = found[0], user = tweet.core?.user_results?.result;
  const numericId = value => typeof value === 'string' && /^[1-9][0-9]{0,19}$/.test(value);
  if (tweet.__typename !== 'Tweet' || tweet.legacy?.id_str !== postId
      || tweet.legacy?.retweeted_status_result != null || tweet.retweeted_status_result != null
      || user?.__typename !== 'User' || !numericId(user.rest_id)
      || tweet.legacy?.user_id_str !== user.rest_id) fail();
  const handles = [user.core?.screen_name, user.legacy?.screen_name].filter(value => value != null);
  if (!handles.length || handles.some(value => typeof value !== 'string'
      || !/^[A-Za-z0-9_]{1,15}$/.test(value) || value.toLowerCase() !== handle)) fail();
  return { adapter_version: '1.0.0', post_id: postId, account_id: user.rest_id, handle,
    profile_url: 'https://x.com/' + handle, source_url: 'https://x.com/' + handle + '/status/' + postId,
    identity_kind: 'native_post_author_id', evidence_source: 'x_tweet_detail' };
}

// Cookies and unrelated thread content stay inside the selected browser tab.
export async function readPostAuthorPage(handle, postId, operation, bearer, project) {
  if (location.origin !== 'https://x.com' || location.pathname !== '/' + handle + '/status/' + postId)
    throw new Error('ONECREW_X_POST_IDENTITY_ORIGIN');
  const csrf = document.cookie.split(';').map(item => item.trim()).find(item => item.startsWith('ct0='))?.slice(4);
  if (!csrf) throw new Error('ONECREW_X_POST_IDENTITY_AUTH');
  const url = new URL('/i/api/graphql/' + operation.queryId + '/TweetDetail', location.origin);
  url.searchParams.set('variables', JSON.stringify({ focalTweetId: postId, referrer: 'tweet',
    with_rux_injections: false, includePromotedContent: false, rankingMode: 'Recency',
    withCommunity: true, withQuickPromoteEligibilityTweetFields: true, withBirdwatchNotes: true, withVoice: true }));
  url.searchParams.set('features', JSON.stringify(operation.features));
  url.searchParams.set('fieldToggles', JSON.stringify(operation.fieldToggles));
  const response = await fetch(url.href, { method: 'GET', credentials: 'include', redirect: 'error',
    signal: AbortSignal.timeout(20000), headers: {
      Authorization: 'Bearer ' + decodeURIComponent(bearer), 'X-Csrf-Token': csrf,
      'X-Twitter-Auth-Type': 'OAuth2Session', 'X-Twitter-Active-User': 'yes',
    } });
  if (response.status === 401 || response.status === 403) throw new Error('ONECREW_X_POST_IDENTITY_AUTH');
  if (response.status === 429) throw new Error('ONECREW_X_POST_IDENTITY_RISK');
  if (!response.ok || response.redirected || new URL(response.url).origin !== location.origin
      || !response.headers.get('content-type')?.includes('application/json') || !response.body)
    throw new Error('ONECREW_X_POST_IDENTITY_INVALID');
  const reader = response.body.getReader(), decoder = new TextDecoder();
  let size = 0, body = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 1048576) throw new Error('ONECREW_X_POST_IDENTITY_TOO_LARGE');
      body += decoder.decode(value, { stream: true });
    }
    body += decoder.decode();
  } finally { await reader.cancel(); }
  return [project(JSON.parse(body), handle, postId)];
}

export async function readTwitterPostAuthor(page, handle, postId, resolveOperation, bearer) {
  if (typeof handle !== 'string' || !/^[a-z0-9_]{1,15}$/.test(handle)
      || typeof postId !== 'string' || !/^[1-9][0-9]{0,19}$/.test(postId))
    throw new Error('ONECREW_X_POST_IDENTITY_INPUT');
  const tab = await page.newTab('https://x.com/' + handle + '/status/' + postId);
  if (!tab) throw new Error('ONECREW_X_POST_IDENTITY_ORIGIN');
  await page.setActivePage(tab);
  await page.wait({ selector: '[data-testid="primaryColumn"]' });
  await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
  const operation = await resolveOperation(page);
  const flags = value => value && typeof value === 'object' && !Array.isArray(value)
    && Object.values(value).every(flag => typeof flag === 'boolean');
  if (!operation || typeof operation.queryId !== 'string' || !/^[A-Za-z0-9_-]{1,100}$/.test(operation.queryId)
      || !flags(operation.features) || !flags(operation.fieldToggles))
    throw new Error('ONECREW_X_POST_IDENTITY_INVALID');
  const metadata = { queryId: operation.queryId, features: operation.features, fieldToggles: operation.fieldToggles };
  const code = '(' + readPostAuthorPage.toString() + ')(' + [JSON.stringify(handle),
    JSON.stringify(postId), JSON.stringify(metadata), JSON.stringify(bearer), projectPostAuthor.toString()].join(',') + ')';
  const result = await page.evaluate(code);
  const rows = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(rows) || rows.length !== 1) throw new Error('ONECREW_X_POST_IDENTITY_INVALID');
  return rows;
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
