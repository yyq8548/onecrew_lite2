// Generic browser tab ownership and snapshot operations.
(function (root) {
  'use strict';
  const OWNED_KEY = 'ownedBrowserTabs';
  const get = function (key) {
    return root.chrome.i18n.getMessage(key) || key;
  };
  function tabInfo(tab) {
    return { tab_id: tab.id, url: tab.url || '', title: tab.title || '' };
  }
  async function ownedTabs(platform, state) {
    const saved = await state();
    const ids = Array.isArray(saved[OWNED_KEY]) ? saved[OWNED_KEY] : [];
    const tabs = [],
      keep = [];
    for (const id of ids) {
      try {
        const tab = await root.chrome.tabs.get(id);
        if (platform.supportsUrl(tab.url)) {
          tabs.push(tab);
          keep.push(id);
        }
      } catch (_) {
        /* Closed tabs are removed from the owned set below. */
      }
    }
    if (keep.length !== ids.length) await root.chrome.storage.local.set({ [OWNED_KEY]: keep });
    return tabs;
  }
  async function ownTab(id, state) {
    const saved = await state();
    const ids = Array.isArray(saved[OWNED_KEY]) ? saved[OWNED_KEY] : [];
    if (!ids.includes(id)) await root.chrome.storage.local.set({ [OWNED_KEY]: ids.concat(id) });
  }
  async function createTab(url, platform, state) {
    if (!platform.supportsUrl(url)) throw new Error(get('bridgeUnsupportedUrl'));
    const tab = await root.chrome.tabs.create({ url: url, active: false });
    await ownTab(tab.id, state);
    return tabInfo(tab);
  }
  async function snapshot(id, platform, state) {
    if (!(await ownedTabs(platform, state)).some((tab) => tab.id === id))
      throw new Error(get('bridgeTabNotOwned'));
    return platform.execute(
      id,
      (tabId) => ({
        tab_id: tabId,
        url: location.href,
        title: document.title,
        text: document.body?.innerText.slice(0, 100000) || '',
      }),
      [id],
    );
  }
  root.OneCrewBrowser = {
    ownedKey: OWNED_KEY,
    tabInfo: tabInfo,
    ownedTabs: ownedTabs,
    ownTab: ownTab,
    createTab: createTab,
    snapshot: snapshot,
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
