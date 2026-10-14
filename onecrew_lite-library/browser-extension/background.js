// YouTube logic adapted from jackwener/OpenCLI (Apache-2.0).
// Modified for OneCrew MV3 execution and normalized channel results.
// See ../THIRD_PARTY_NOTICES.md and licenses/APACHE-2.0.txt.
importScripts('platforms/youtube/index.js', 'platforms/index.js', 'browser.js', 'discovery.js');

let serviceUrl;
let rediscover = true;
const POLL_ALARM = 'onecrew-command-poll';
const OWNED_KEY = OneCrewBrowser.ownedKey;
const SESSION_KEY = 'bridgeSession';
const RESULT_KEY = 'pendingResults';
const DISABLED_KEY = 'bridgeDisabled';
const LAST_SUCCESS_KEY = 'lastBridgeSuccess';
let pollAbort = null;
let executing = false;
let heartbeatTimer = null;
let connectPromise = null;
const seenCommands = new Set();

function i18n(key, substitutions) {
  return chrome.i18n.getMessage(key, substitutions) || key;
}

function bridgeHeaders(token) {
  const headers = {
    'Content-Type': 'application/json',
    'X-OneCrew-Extension-Id': chrome.runtime.id,
  };
  if (token) headers.Authorization = 'Bearer ' + token;
  return headers;
}

function timeoutSignal(ms) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ms);
  return { signal: controller.signal, clear: () => clearTimeout(timer) };
}

async function state() {
  return chrome.storage.local.get([
    'token',
    'serviceUrl',
    SESSION_KEY,
    OWNED_KEY,
    RESULT_KEY,
    DISABLED_KEY,
    LAST_SUCCESS_KEY,
  ]);
}

async function connect() {
  if (connectPromise) return connectPromise;
  connectPromise = (async () => {
    const current = await state();
    if (current[DISABLED_KEY]) return { ok: false, disabled: true };
    let request;
    try {
      const discovered = await OneCrewDiscovery.find(current.serviceUrl);
      if (!discovered) throw new Error(i18n('bridgeLocalServiceUnavailable'));
      request = timeoutSignal(10000);
      const response = await fetch(`${discovered}/api/v1/browser/connect`, {
        method: 'POST',
        headers: bridgeHeaders(),
        signal: request.signal,
        body: JSON.stringify({ extension_id: chrome.runtime.id }),
      });
      if (!response.ok) return { ok: false, error: i18n('bridgeConnectHttp', response.status) };
      const body = await response.json();
      if (!body.token) return { ok: false, error: i18n('bridgeMissingToken') };
      if ((await state())[DISABLED_KEY]) return { ok: false, disabled: true };
      serviceUrl = discovered;
      rediscover = false;
      if (
        current.token === body.token &&
        current[SESSION_KEY] &&
        current.serviceUrl === discovered
      ) {
        await chrome.storage.local.set({ [LAST_SUCCESS_KEY]: Date.now() });
        return { ok: true };
      }
      const session = crypto.randomUUID();
      await chrome.storage.local.set({
        token: body.token,
        serviceUrl: discovered,
        [SESSION_KEY]: session,
        [RESULT_KEY]: { session, items: {} },
        [LAST_SUCCESS_KEY]: Date.now(),
      });
      return { ok: true };
    } catch (error) {
      return {
        ok: false,
        error: error?.message || i18n('bridgeLocalServiceUnavailable'),
      };
    } finally {
      request?.clear();
    }
  })().finally(() => {
    connectPromise = null;
  });
  return connectPromise;
}

async function forgetSession() {
  await chrome.storage.local.remove(['token', SESSION_KEY, RESULT_KEY, LAST_SUCCESS_KEY]);
}

async function command(commandValue) {
  const input = commandValue.input || {};
  const handler = OneCrewPlatforms.operations[commandValue.operation];
  if (handler) return handler(input);
  if (commandValue.operation === 'browser.open')
    return OneCrewBrowser.createTab(input.url, OneCrewPlatforms, state);
  if (commandValue.operation === 'browser.tabs')
    return {
      tabs: (await OneCrewBrowser.ownedTabs(OneCrewPlatforms, state)).map(OneCrewBrowser.tabInfo),
    };
  if (commandValue.operation === 'browser.snapshot')
    return OneCrewBrowser.snapshot(Number(input.tab_id), OneCrewPlatforms, state);
  throw new Error(i18n('bridgeUnsupportedOperation'));
}

async function flushResults(token, session) {
  const saved = await state();
  const pending = saved[RESULT_KEY];
  if (!pending || pending.session !== session) return;
  for (const [id, payload] of Object.entries(pending.items || {})) {
    const request = timeoutSignal(10000);
    try {
      const response = await fetch(`${serviceUrl}/api/v1/browser/results`, {
        method: 'POST',
        headers: bridgeHeaders(token),
        signal: request.signal,
        body: JSON.stringify({ id, ...payload }),
      });
      if (response.status === 401 || response.status === 403) {
        await forgetSession();
        return;
      }
      if (response.ok) {
        await chrome.storage.local.set({ [LAST_SUCCESS_KEY]: Date.now() });
        delete pending.items[id];
        await chrome.storage.local.set({ [RESULT_KEY]: pending });
      }
    } finally {
      request?.clear();
    }
  }
}

async function saveResult(id, payload, token, session) {
  const saved = await state();
  if (saved.token !== token || saved[SESSION_KEY] !== session) return;
  const pending =
    saved[RESULT_KEY]?.session === session ? saved[RESULT_KEY] : { session, items: {} };
  pending.items[id] = payload;
  await chrome.storage.local.set({ [RESULT_KEY]: pending });
  await flushResults(token, session);
}

function startHeartbeat(token, session) {
  if (heartbeatTimer) return;
  heartbeatTimer = setInterval(async () => {
    try {
      const current = await state();
      if (!executing || current.token !== token || current[SESSION_KEY] !== session || !serviceUrl)
        return;
      const request = timeoutSignal(10000);
      try {
        const response = await fetch(serviceUrl + '/api/v1/browser/heartbeat', {
          method: 'POST',
          headers: bridgeHeaders(token),
          signal: request.signal,
        });
        if (response.ok) {
          const latest = await state();
          if (latest.token === token && latest[SESSION_KEY] === session)
            await chrome.storage.local.set({ [LAST_SUCCESS_KEY]: Date.now() });
        }
      } finally {
        request.clear();
      }
    } catch (_) {
      // A heartbeat is advisory. Result submission still checks token/session.
    }
  }, 20000);
}

function stopHeartbeat() {
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  heartbeatTimer = null;
}

async function poll() {
  if (pollAbort || executing) return;
  let saved = await state();
  if (saved[DISABLED_KEY]) return;
  serviceUrl = saved.serviceUrl;
  if (rediscover || !saved.token || !saved[SESSION_KEY] || !serviceUrl) {
    if (!(await connect()).ok) return;
    saved = await state();
  }
  if (pollAbort || executing || saved[DISABLED_KEY]) return;
  pollAbort = new AbortController();
  const token = saved.token;
  const session = saved[SESSION_KEY];
  let continuePolling = false;
  try {
    await flushResults(token, session);
    const request = timeoutSignal(21000);
    const response = await fetch(`${serviceUrl}/api/v1/browser/commands`, {
      headers: bridgeHeaders(token),
      signal: AbortSignal.any([pollAbort.signal, request.signal]),
    });
    request?.clear();
    if (response.status === 401 || response.status === 403) {
      await forgetSession();
      await connect();
      return;
    }
    if (!response.ok) return;
    continuePolling = true;
    await chrome.storage.local.set({ [LAST_SUCCESS_KEY]: Date.now() });
    const commandValue = (await response.json()).command;
    if (!commandValue?.id || seenCommands.has(commandValue.id)) return;
    seenCommands.add(commandValue.id);
    if (seenCommands.size > 500) seenCommands.delete(seenCommands.values().next().value);
    if ((await state()).token !== token) return;
    executing = true;
    startHeartbeat(token, session);
    try {
      await saveResult(commandValue.id, { result: await command(commandValue) }, token, session);
    } catch (error) {
      await saveResult(
        commandValue.id,
        { error: { message: error?.message || String(error) } },
        token,
        session,
      );
    } finally {
      executing = false;
      stopHeartbeat();
    }
  } catch (_) {
    continuePolling = false;
    rediscover = true;
    await chrome.storage.local.remove(LAST_SUCCESS_KEY);
  } finally {
    pollAbort = null;
    if (continuePolling) setTimeout(() => void poll(), 0);
  }
}

chrome.alarms.create(POLL_ALARM, { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === POLL_ALARM) poll();
});
chrome.runtime.onStartup.addListener(() => poll());
chrome.runtime.onInstalled.addListener(() => poll());
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  (async () => {
    try {
      if (message.type === 'connect' || message.type === 'retry') {
        await chrome.storage.local.set({ [DISABLED_KEY]: false });
        const result = await connect();
        if (result.ok) poll();
        sendResponse(result);
        return;
      }
      if (message.type === 'disconnect') {
        pollAbort?.abort();
        await forgetSession();
        await chrome.storage.local.set({ [DISABLED_KEY]: true });
        sendResponse({ ok: true });
        return;
      }
      if (message.type === 'status') {
        const current = await state();
        sendResponse({
          ok: true,
          paired: Boolean(current.token),
          connected: Boolean(
            current[LAST_SUCCESS_KEY] && Date.now() - current[LAST_SUCCESS_KEY] < 70000,
          ),
          disabled: Boolean(current[DISABLED_KEY]),
          extensionId: chrome.runtime.id,
          service_url: current.serviceUrl ?? null,
        });
      }
    } catch (error) {
      sendResponse({ ok: false, error: error.message });
    }
  })();
  return true;
});

void poll();
