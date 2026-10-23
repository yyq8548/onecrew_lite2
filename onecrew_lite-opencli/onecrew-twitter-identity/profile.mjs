// A profile's native ID is an observation, not proof of ownership of an older post.
export function projectIdentity(result, expectedHandle, evidenceSource = 'x_user_by_screen_name') {
  const fail = () => { throw new Error('ONECREW_X_IDENTITY_INVALID'); };
  if (result?.__typename !== 'User' || typeof result.rest_id !== 'string'
      || !/^[1-9][0-9]{0,19}$/.test(result.rest_id)) fail();
  const oneValue = values => {
    const present = values.filter(value => value !== undefined && value !== null);
    if (present.some(value => value !== present[0])) fail();
    return present[0];
  };
  const handles = [result.core?.screen_name, result.legacy?.screen_name].filter(value => value != null);
  if (!handles.length || handles.some(value => typeof value !== 'string'
      || !/^[a-zA-Z0-9_]{1,15}$/.test(value) || value.toLowerCase() !== expectedHandle)) fail();
  const name = oneValue([result.core?.name, result.legacy?.name]);
  const bio = oneValue([result.profile_bio?.description, result.legacy?.description]) ?? '';
  const location = oneValue([result.location?.location, result.legacy?.location]) ?? '';
  const count = oneValue([result.relationship_counts?.followers, result.legacy?.followers_count]);
  const text = (value, maximum, multiline = false) => typeof value === 'string'
    && [...value].length <= maximum
    && !(multiline ? /[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/ : /[\u0000-\u001f\u007f-\u009f]/).test(value);
  if (!text(name, 200) || !name.trim() || !text(bio, 2000, true) || !text(location, 200)
      || (count != null && (!Number.isSafeInteger(count) || count < 0))) fail();
  return { adapter_version: '1.0.0', account_id: result.rest_id, handle: expectedHandle,
    profile_url: 'https://x.com/' + expectedHandle, name, bio,
    self_declared_location: location, follower_count: count ?? null,
    identity_kind: 'native_id', evidence_source: evidenceSource };
}

// Runs inside the selected Chrome tab. Cookies never leave this function.
export async function readIdentityPage(handle, operation, bearer, project) {
  if (location.origin !== 'https://x.com' || location.pathname !== '/' + handle)
    throw new Error('ONECREW_X_IDENTITY_ORIGIN');
  const csrf = document.cookie.split(';').map(item => item.trim()).find(item => item.startsWith('ct0='))?.slice(4);
  if (!csrf) throw new Error('ONECREW_X_IDENTITY_AUTH');
  const url = new URL('/i/api/graphql/' + operation.queryId + '/UserByScreenName', location.origin);
  url.searchParams.set('variables', JSON.stringify({ screen_name: handle, withSafetyModeUserFields: true }));
  url.searchParams.set('features', JSON.stringify(operation.features));
  const response = await fetch(url.href, { method: 'GET', credentials: 'include', redirect: 'error',
    signal: AbortSignal.timeout(20000), headers: {
      Authorization: 'Bearer ' + decodeURIComponent(bearer), 'X-Csrf-Token': csrf,
      'X-Twitter-Auth-Type': 'OAuth2Session', 'X-Twitter-Active-User': 'yes',
    } });
  if (response.status === 401 || response.status === 403) throw new Error('ONECREW_X_IDENTITY_AUTH');
  if (response.status === 429) throw new Error('ONECREW_X_IDENTITY_RISK');
  if (!response.ok || response.redirected || new URL(response.url).origin !== location.origin
      || !response.headers.get('content-type')?.includes('application/json') || !response.body)
    throw new Error('ONECREW_X_IDENTITY_INVALID');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let size = 0, body = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 1048576) throw new Error('ONECREW_X_IDENTITY_TOO_LARGE');
      body += decoder.decode(value, { stream: true });
    }
    body += decoder.decode();
  } finally { await reader.cancel(); }
  const data = JSON.parse(body);
  if (data.errors != null && (!Array.isArray(data.errors) || data.errors.length))
    throw new Error('ONECREW_X_IDENTITY_INVALID');
  return [project(data.data?.user?.result, handle)];
}

export async function readTwitterIdentity(page, handle, resolveOperation, bearer) {
  if (typeof handle !== 'string' || !/^[a-z0-9_]{1,15}$/.test(handle))
    throw new Error('ONECREW_X_IDENTITY_INPUT');
  const tab = await page.newTab('https://x.com/' + handle);
  if (!tab) throw new Error('ONECREW_X_IDENTITY_ORIGIN');
  await page.setActivePage(tab);
  await page.wait({ selector: '[data-testid="primaryColumn"]' });
  await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
  const operation = await resolveOperation(page);
  if (!operation || typeof operation.queryId !== 'string' || !/^[A-Za-z0-9_-]{1,100}$/.test(operation.queryId)
      || !operation.features || typeof operation.features !== 'object' || Array.isArray(operation.features)
      || Object.values(operation.features).some(value => typeof value !== 'boolean'))
    throw new Error('ONECREW_X_IDENTITY_INVALID');
  const result = await page.evaluate(`(${readIdentityPage.toString()})(${JSON.stringify(handle)}, ${JSON.stringify({ queryId: operation.queryId, features: operation.features })}, ${JSON.stringify(bearer)}, ${projectIdentity.toString()})`);
  const rows = result && 'session' in result && 'data' in result ? result.data : result;
  if (!Array.isArray(rows) || rows.length !== 1) throw new Error('ONECREW_X_IDENTITY_INVALID');
  return rows;
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
