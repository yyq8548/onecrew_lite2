const status = document.querySelector('#status');
let language;
let catalog;
const languageButton = document.querySelector('#language');
const message = (name, substitutions) => {
  const entry = catalog?.[name];
  if (!entry) return chrome.i18n.getMessage(name, substitutions);
  let text = entry.message;
  const values = Array.isArray(substitutions) ? substitutions : [substitutions ?? ''];
  for (const [key, placeholder] of Object.entries(entry.placeholders || {})) {
    const value = placeholder.content.replace(
      /\$(\d+)/g,
      (_, index) => values[Number(index) - 1] ?? '',
    );
    text = text.replaceAll(`$${key}$`, value);
  }
  return text;
};
async function applyLanguage(next) {
  const response = await fetch(chrome.runtime.getURL(`_locales/${next}/messages.json`));
  catalog = await response.json();
  language = next;
  document.documentElement.lang = next === 'en' ? 'en' : 'zh-CN';
  for (const element of document.querySelectorAll('[data-i18n]')) {
    const text = message(element.dataset.i18n);
    if (element.dataset.i18nAttr) element.setAttribute(element.dataset.i18nAttr, text);
    else element.textContent = text;
  }
  const label = next === 'en' ? '切换为中文' : 'Switch to English';
  languageButton.setAttribute('aria-label', label);
  languageButton.title = label;
  await refresh();
}
languageButton.addEventListener('click', async () => {
  languageButton.disabled = true;
  try {
    const next = language === 'en' ? 'zh_CN' : 'en';
    await chrome.storage.local.set({ language: next });
    await applyLanguage(next);
  } finally {
    languageButton.disabled = false;
  }
});
const serviceAddress = document.querySelector('[data-i18n="serviceAddress"]');
function showServiceAddress(url) {
  serviceAddress.textContent = message(
    url ? 'serviceAddress' : 'serviceAddressDiscovering',
    url || undefined,
  );
}
showServiceAddress();
function show(m) {
  status.textContent = m;
}
function send(type, extra) {
  return new Promise((resolve) => chrome.runtime.sendMessage({ type, ...extra }, resolve));
}
async function refresh() {
  const r = await send('status');
  showServiceAddress(r?.service_url);
  show(
    r?.connected
      ? message('statusConnected')
      : r?.disabled
        ? message('statusDisconnectedReconnect')
        : message('statusConnectingUnavailable'),
  );
}
document.querySelector('#connect').addEventListener('click', async () => {
  show(message('statusConnecting'));
  const r = await send('retry');
  show(r && r.ok ? message('statusConnected') : (r && r.error) || message('connectFailed'));
});
document.querySelector('#disconnect').addEventListener('click', async () => {
  await send('disconnect');
  show(message('statusDisconnected'));
});
chrome.storage.local
  .get('language')
  .then(({ language: saved }) =>
    applyLanguage(
      saved === 'en' || saved === 'zh_CN'
        ? saved
        : chrome.i18n.getUILanguage().startsWith('zh')
          ? 'zh_CN'
          : 'en',
    ),
  );
