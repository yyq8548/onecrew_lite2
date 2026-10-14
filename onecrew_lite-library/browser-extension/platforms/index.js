// Explicit platform registration; the bridge dispatches without platform-specific logic.
(function (root) {
  const adapters = [root.OneCrewYoutubePlatform];
  const adapterFor = (url) => adapters.find((adapter) => adapter.supportsUrl(url));
  root.OneCrewPlatforms = {
    supportsUrl: (url) => Boolean(adapterFor(url)),
    operations: Object.assign({}, ...adapters.map((adapter) => adapter.operations)),
    async execute(tabId, func, args) {
      const tab = await chrome.tabs.get(tabId);
      const adapter = adapterFor(tab.url);
      if (!adapter) throw new Error(chrome.i18n.getMessage('bridgeUnsupportedUrl'));
      return adapter.execute(tabId, func, args);
    },
  };
})(globalThis);
