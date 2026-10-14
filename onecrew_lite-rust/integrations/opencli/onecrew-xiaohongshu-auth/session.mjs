// Search runs on www.xiaohongshu.com, not the creator dashboard.
// Read only the current-user store. Never return IDs, cookies or profile data.
export function observeXhsLogin() {
  if (location.origin !== 'https://www.xiaohongshu.com') return 'unknown';
  if (document.querySelector('iframe[src*="captcha"], #captcha, .captcha-container')) return 'unknown';
  const store = window.__INITIAL_STATE__?.user;
  const loggedIn = store?.loggedIn?._value ?? store?.loggedIn;
  const info = store?.userInfo?._value ?? store?.userInfo;
  const id = info?.user_id ?? info?.userId ?? info?.userID;
  if (loggedIn === true && typeof id === 'string' && id.trim() && id.length <= 128) return 'logged_in';
  const login = document.querySelector('#login-btn');
  if (loggedIn === false && login && login.getClientRects().length > 0) return 'not_logged_in';
  return 'unknown';
}

export async function readXhsLogin(page) {
  const target = await page.newTab('https://www.xiaohongshu.com/explore');
  if (!target) throw new Error('Authentication probe page was not created');
  page.setActivePage(target);
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await page.wait(2);
    let result = await page.evaluate(`(${observeXhsLogin.toString()})()`);
    if (result && typeof result === 'object' && 'session' in result && 'data' in result) result = result.data;
    if (result === 'logged_in' || result === 'not_logged_in') return result;
  }
  return 'unknown';
}
