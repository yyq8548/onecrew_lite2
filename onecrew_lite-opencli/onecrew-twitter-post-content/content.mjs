import { projectPostAuthor, readPostAuthorPage } from '../onecrew-twitter-post-identity/post.mjs';
import { projectLanguageSample } from './language.mjs';
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
import { projectIdentity } from '../onecrew-twitter-identity/profile.mjs';

// Only fixed local categories may leave the adapter, never exception payloads.
export function postContentDiagnostic(message) {
  const marker = message.match(/\bONECREW_X_POST_CONTENT_INVALID_([A-Z_]+)\b/)?.[1];
  for (const code of ['AUTHOR', 'NOTE', 'TEXT', 'TIME', 'ENTITIES', 'NOTE_URLS_MISSING', 'URLS_TYPE', 'URL', 'MEDIA', 'QUOTE', 'OPERATION', 'RESPONSE']) {
    if (marker === code) return code.toLowerCase();
  }
  if (message.includes('ONECREW_X_POST_IDENTITY_INVALID')) return 'identity';
  if (message.includes('ONECREW_X_POST_IDENTITY_ORIGIN')) return 'origin';
  if (message.includes('ONECREW_X_POST_CONTENT_VERSION')) return 'version';
  if (message.includes('ONECREW_X_POST_IDENTITY_TOO_LARGE')) return 'output_size';
  return 'unclassified';
}

// This projection runs inside the browser. Only the exact focal author's text
// and link entities leave the tab; these are observations, not qualification.
export function projectPostContent(data, handle, postId, accountId, projectIdentity, projectProfile) {
  const fail = code => { throw new Error('ONECREW_X_POST_CONTENT_INVALID_' + code); };
  const identity = projectIdentity(data, handle, postId);
  if (accountId !== null && identity.account_id !== accountId) fail('AUTHOR');
  let focal;
  const inspect = result => {
    const tweet = result?.__typename === 'TweetWithVisibilityResults' ? result.tweet : result;
    if (tweet?.rest_id === postId) focal = tweet;
  };
  // The identity projector has already rejected missing or duplicate focal posts.
  for (const instruction of data.data.threaded_conversation_with_injections_v2.instructions) {
    for (const entry of instruction.entries ?? []) {
      inspect(entry.content?.itemContent?.tweet_results?.result);
      for (const item of entry.content?.items ?? []) inspect(item.item?.itemContent?.tweet_results?.result);
    }
  }
  const note = focal.note_tweet?.note_tweet_results?.result;
  if (focal.note_tweet != null && (note == null || typeof note !== 'object')) fail('NOTE');
  const text = note != null ? note.text : focal.legacy.full_text;
  const entities = note != null ? note.entity_set : focal.legacy.entities;
  const created = focal.legacy.created_at;
  if (typeof text !== 'string' || !text.trim() || text.includes('\0')) fail('TEXT');
  if (typeof created !== 'string' || !created.trim() || created.length > 80 || /[\u0000-\u001f\u007f]/.test(created)) fail('TIME');
  if (!entities || typeof entities !== 'object' || Array.isArray(entities)) fail('ENTITIES');
  if (note != null && !Object.hasOwn(entities, 'urls')) fail('NOTE_URLS_MISSING');
  // TweetDetail can omit legacy URLs. Record no returned links, never infer them from text.
  const urlEntities = note == null && !Object.hasOwn(entities, 'urls') ? [] : entities.urls;
  if (!Array.isArray(urlEntities)) fail('URLS_TYPE');
  // Clip on a Unicode boundary. A long note never falls back to a shorter legacy preview.
  const encoder = new TextEncoder();
  let excerpt = '', bytes = 0;
  for (const character of text) {
    const length = encoder.encode(character).length;
    if (bytes + length > 8000) break;
    excerpt += character; bytes += length;
  }
  const urls = [];
  for (const entity of urlEntities) {
    const value = entity?.expanded_url;
    if (typeof value !== 'string' || value.length > 2048 || /\s|[\u0000-\u001f\u007f]/.test(value)) fail('URL');
    let parsed;
    try { parsed = new URL(value); } catch { fail('URL'); }
    if (!['https:', 'http:'].includes(parsed.protocol) || !parsed.hostname || parsed.username || parsed.password) fail('URL');
    if (!urls.includes(value)) urls.push(value);
    if (urls.length > 20) fail('URL');
  }
  const media = focal.legacy.extended_entities?.media ?? focal.legacy.entities?.media ?? [];
  if (!Array.isArray(media) || media.some(item => !['photo', 'video', 'animated_gif'].includes(item?.type))) fail('MEDIA');
  if (focal.legacy.is_quote_status != null && typeof focal.legacy.is_quote_status !== 'boolean') fail('QUOTE');
  const content = { adapter_version: '1.0.0', identity, text: excerpt, text_truncated: excerpt !== text,
    text_source: note != null ? 'note_tweet' : 'legacy_full_text', created_at: created,
    has_media: media.length > 0,
    quotes_another_post: focal.legacy.is_quote_status === true || focal.quoted_status_result != null,
    expanded_urls: urls,
    observations: {
      published_at:created,
      like_count:Number.isSafeInteger(focal.legacy.favorite_count)&&focal.legacy.favorite_count>=0?focal.legacy.favorite_count:null,
      comment_count:Number.isSafeInteger(focal.legacy.reply_count)&&focal.legacy.reply_count>=0?focal.legacy.reply_count:null,
      share_count:Number.isSafeInteger(focal.legacy.retweet_count)&&focal.legacy.retweet_count>=0?focal.legacy.retweet_count:null,
      view_count:/^\d+$/.test(focal.views?.count??'')&&Number.isSafeInteger(Number(focal.views.count))?Number(focal.views.count):null,
      text_completeness:excerpt!==text?'truncated':'complete',
    } };
  if (!projectProfile) return content;
  const user = focal.core.user_results.result;
  const name = user.core?.name ?? user.legacy?.name;
  const bio = user.profile_bio?.description ?? user.legacy?.description;
  const count = user.relationship_counts?.followers ?? user.legacy?.followers_count;
  // 只有同次响应确实带回全部必要字段，才省去单独的主页请求。
  const profile = typeof name === 'string' && typeof bio === 'string' && Number.isSafeInteger(count) && count >= 0
    ? projectProfile(user, handle, 'x_tweet_detail') : null;
  return { content, profile };
}

export async function readTwitterPostContent(page, handle, postId, accountId, resolveOperation, bearer, languageSample = false, includeAuthorProfile = false) {
  const numeric = value => typeof value === 'string' && /^[1-9][0-9]{0,19}$/.test(value);
  if (typeof handle !== 'string' || !/^[a-z0-9_]{1,15}$/.test(handle) || !numeric(postId) || !(numeric(accountId) || includeAuthorProfile && accountId === null) || includeAuthorProfile && languageSample)
    throw new Error('ONECREW_X_POST_IDENTITY_INPUT');
  const tab = await page.newTab('https://x.com/' + handle + '/status/' + postId);
  if (!tab) throw new Error('ONECREW_X_POST_IDENTITY_ORIGIN');
  await page.setActivePage(tab);
  await page.wait({ selector: '[data-testid="primaryColumn"]' });
  await page.evaluate('(' + restoreOriginalLanguage.toString() + ')()');
  const operation = await resolveOperation(page);
  const flags = value => value && typeof value === 'object' && !Array.isArray(value)
    && Object.values(value).every(flag => typeof flag === 'boolean');
  if (!operation || typeof operation.queryId !== 'string' || !/^[A-Za-z0-9_-]{1,100}$/.test(operation.queryId)
      || !flags(operation.features) || !flags(operation.fieldToggles))
    throw new Error('ONECREW_X_POST_CONTENT_INVALID_OPERATION');
  const metadata = { queryId: operation.queryId, features: operation.features, fieldToggles: operation.fieldToggles };
  const project = languageSample
    ? '(data, handle, postId) => (' + projectLanguageSample.toString() + ')(data, handle, postId, '
      + JSON.stringify(accountId) + ', ' + projectPostContent.toString() + ', ' + projectPostAuthor.toString() + ')'
    : '(data, handle, postId) => (' + projectPostContent.toString() + ')(data, handle, postId, '
      + JSON.stringify(accountId) + ', ' + projectPostAuthor.toString() + (includeAuthorProfile ? ', ' + projectIdentity.toString() : '') + ')';
  const code = '(' + readPostAuthorPage.toString() + ')(' + [JSON.stringify(handle), JSON.stringify(postId),
    JSON.stringify(metadata), JSON.stringify(bearer), project].join(',') + ')';
  const result = await page.evaluate(code);
  const rows = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(rows) || rows.length !== 1) throw new Error('ONECREW_X_POST_CONTENT_INVALID_RESPONSE');
  return rows;
}
