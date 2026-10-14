// Runs in the page. Translation is presentation state, never language evidence.
export async function restoreOriginalLanguage() {
  // Navigation can resolve before the new document has its root. Do not
  // certify that transient empty document as untranslated or read from it.
  let root = document.documentElement;
  for (let attempt = 0; !root && attempt < 20; attempt++) {
    await new Promise(resolve => setTimeout(resolve, 100));
    root = document.documentElement;
  }
  if (!root) throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
  root.setAttribute('translate', 'no');
  root.classList.add('notranslate');
  const translated = () => root.classList.contains('translated-ltr') || root.classList.contains('translated-rtl');
  if (translated() && typeof globalThis.cr?.googleTranslate?.revert === 'function') {
    globalThis.cr.googleTranslate.revert();
  }
  const visible = node => node.getClientRects().length > 0;
  const originalControls = () => [...document.querySelectorAll('button,[role="button"]')].filter(button =>
    /^(Show original|See original|View original|显示原文|查看原文|顯示原文|查看原始内容)$/i.test(button.textContent.trim()) && visible(button));
  for (const button of originalControls()) button.click();
  // A click alone does not prove restoration. Wait for translated-state markers
  // and original-text toggles to disappear before any search data is extracted.
  for (let attempt = 0; attempt < 10; attempt++) {
    if (!translated() && originalControls().length === 0) return true;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
}
