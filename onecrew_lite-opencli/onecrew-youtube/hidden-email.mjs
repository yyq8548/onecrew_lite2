import { homedir } from 'node:os';
import { join } from 'node:path';
import { YoutubeHiddenEmailStore } from './hidden-email-store.mjs';

export function youtubePublicEmails(text) {
  return [...new Set((text.match(/[a-z0-9][a-z0-9.!#$%&'*+/=?^_`{|}~\-]*@[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+/giu) ?? []).filter(email => {
    const local = email.slice(0, email.lastIndexOf('@'));
    return email.length <= 254 && local.length <= 64 && !local.endsWith('.') && !local.includes('..');
  }))];
}
export function youtubeDailyEmailLimit(text) {
  return /(?:达到|達到|超出|超過).{0,20}(?:今日|今天).{0,30}(?:限制|上限)|(?:daily|today).{0,60}(?:limit|maximum)|(?:limit|maximum).{0,60}(?:daily|today)/iu.test(text);
}
// 只读取已绑定作者的原生面板及可见控件。
export function hiddenEmailPageState(channel) {
  if (location.origin !== 'https://www.youtube.com' || window.ytInitialData?.metadata?.channelMetadataRenderer?.externalId !== channel) throw new Error('ONECREW_CHANNEL_ID_MISMATCH');
  const visible = element => element != null && element.getClientRects().length > 0;
  const panels = [...document.querySelectorAll('ytd-about-channel-renderer')].filter(visible);
  if (panels.length !== 1 || panels[0].data?.metadata?.aboutChannelViewModel?.channelId !== channel) throw new Error('ONECREW_CHANNEL_ABOUT_UNAVAILABLE');
  const panel = panels[0], link = panel.querySelector('a#email'), frame = panel.querySelector('#recaptcha iframe');
  const rect = visible(frame) ? frame.getBoundingClientRect() : null;
  return {
    email: visible(link) && link.getAttribute('href')?.startsWith('mailto:') ? link.getAttribute('href').slice(7).split('?')[0] : null,
    text: panel.querySelector('#view-email-button-container')?.closest('tr')?.innerText.slice(0,20000) ?? '',
    viewAvailable: visible(panel.querySelector('#view-email-button-container button')),
    submitAvailable: visible(panel.querySelector('#submit-btn')),
    frame: rect ? { url: frame.src, left: rect.left, top: rect.top } : null,
  };
}
const unwrap = result => result && typeof result === 'object' && 'session' in result && 'data' in result ? result.data : result;
export async function enrichYoutubeHiddenEmail(page, channel, result, task, directory = process.env.OPENCLI_CONFIG_DIR ?? join(homedir(), '.opencli')) {
  if (typeof task !== 'string' || !task.trim() || task.length > 200) throw new Error('youtube_email_task_invalid');
  const store = new YoutubeHiddenEmailStore(directory);
  let lease = null;
  const complete = (status, email = null, reason = null) => {
    const capturedAt = Date.now();
    store.record(channel, status, email, reason, capturedAt);
    return { ...result, description: email && !youtubePublicEmails(result.description).includes(email) ? `${result.description}\n${email}` : result.description,
      hiddenEmailStatus: status, hiddenEmailTaskId: task, hiddenEmailCapturedAt: capturedAt };
  };
  try {
    if (youtubePublicEmails(result.description).length) return complete('public_email');
    const state = () => page.evaluate(`(${hiddenEmailPageState.toString()})(${JSON.stringify(channel)})`).then(unwrap);
    let observed = await state();
    result = { ...result, hiddenEmailAvailable: result.hiddenEmailAvailable || observed.viewAvailable || observed.email !== null };
    if (observed.email !== null) {
      if (!youtubePublicEmails(observed.email).includes(observed.email)) throw new Error('ONECREW_CHANNEL_EMAIL_INVALID');
      return complete('revealed', observed.email);
    }
    const saved = store.observation(channel);
    if (saved?.email) {
      if (!youtubePublicEmails(saved.email).includes(saved.email)) throw new Error('ONECREW_CHANNEL_EMAIL_INVALID');
      return complete('cached', saved.email);
    }
    if (!observed.viewAvailable) return complete('not_available');
    const gate = store.claim(task);
    if (gate.status !== 'ready') return complete(gate.status, null, gate.reason);
    lease = gate.lease;
    await page.click('ytd-about-channel-renderer #view-email-button-container button');
    const deadline = Date.now() + 20000;
    let checkboxClicked = false, submitted = false;
    while (Date.now() < deadline) {
      observed = await state();
      if (youtubeDailyEmailLimit(observed.text)) {
        const reason = observed.text.split('\n').find(youtubeDailyEmailLimit);
        store.noteDailyLimit(task, Date.now(), reason);
        return complete('daily_limit', null, reason);
      }
      if (observed.email !== null) {
        if (!youtubePublicEmails(observed.email).includes(observed.email)) throw new Error('ONECREW_CHANNEL_EMAIL_INVALID');
        return complete('revealed', observed.email);
      }
      if (observed.frame && !submitted) {
        const frames = await page.frames();
        const frame = frames.find(frame => frame.url === observed.frame.url);
        if (frame) {
          const url = new URL(frame.url);
          if (!['www.google.com','www.recaptcha.net'].includes(url.hostname) || !url.pathname.endsWith('/anchor')) throw new Error('ONECREW_CAPTCHA_FRAME_INVALID');
          const box = unwrap(await page.evaluateInFrame(`(() => { const box=document.querySelector('#recaptcha-anchor'); if(!box) return null; const rect=box.getBoundingClientRect(); return {checked:box.getAttribute('aria-checked')==='true',left:rect.left,top:rect.top,width:rect.width,height:rect.height}; })()`, frame.index));
          if (box?.checked && observed.submitAvailable) {
            await page.click('ytd-about-channel-renderer #submit-btn'); submitted = true;
          } else if (box && !checkboxClicked) {
            const point = { x: observed.frame.left + box.left + box.width / 2, y: observed.frame.top + box.top + box.height / 2 };
            await page.cdp('Input.dispatchMouseEvent', { type:'mousePressed', ...point, button:'left', clickCount:1 });
            await page.cdp('Input.dispatchMouseEvent', { type:'mouseReleased', ...point, button:'left', clickCount:1 });
            checkboxClicked = true;
          }
        }
      }
      await page.wait(0.4);
    }
    return complete('verification_required', null, submitted ? 'email_reveal_not_completed' : 'captcha_requires_user');
  } finally {
    if (lease !== null) store.release(lease);
    store.close();
  }
}
