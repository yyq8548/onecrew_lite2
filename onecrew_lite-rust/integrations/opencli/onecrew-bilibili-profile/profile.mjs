// Exact public follower evidence, not a conversion from an abbreviated display.
export function projectProfile(observation, expectedMid) {
  const fail = () => { throw new Error('ONECREW_BILIBILI_PROFILE_INVALID'); };
  const validText = (value, maximum) => typeof value === 'string' && value.trim().length > 0
    && [...value].length <= maximum && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
  const raw = observation?.follower_count_text;
  if (observation?.mid !== expectedMid || observation.label !== '粉丝数'
      || !validText(observation.name, 200) || !validText(observation.follower_display, 32)
      || typeof raw !== 'string' || raw.length > 32
      || !/^(?:0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+)$/.test(raw)) fail();
  const count = Number(raw.replace(/,/g, ''));
  if (!Number.isSafeInteger(count) || count < 0) fail();
  const sign = observation.sign;
  if (sign !== undefined && (typeof sign !== 'string' || [...sign].length > 1000
      || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/.test(sign))) fail();
  return { adapter_version: '1.0.0', mid: observation.mid, name: observation.name.trim(),
    follower_count: count, follower_count_text: raw, follower_display: observation.follower_display,
    profile_url: 'https://space.bilibili.com/' + observation.mid, evidence_source: 'profile_tooltip',
    ...(sign === undefined ? {} : {sign:sign.trim()}) };
}

export function readProfilePage(expectedMid, project) {
  const current = new URL(location.href);
  const match = current.pathname.match(/^\/([1-9][0-9]{0,19})\/?$/);
  if (current.origin !== 'https://space.bilibili.com' || current.username || current.password
      || !match || match[1] !== expectedMid) throw new Error('ONECREW_BILIBILI_PROFILE_ORIGIN');
  const text = (document.body?.innerText ?? '').slice(0, 100000);
  if (/验证码|安全验证|操作频繁|访问受限|captcha|too many requests/i.test(text)) throw new Error('ONECREW_BILIBILI_PROFILE_RISK');
  if (/请先登录|登录后查看/.test(text)) throw new Error('ONECREW_BILIBILI_PROFILE_AUTH');
  const names = document.querySelectorAll('.upinfo-detail__top .nickname');
  const metrics = [...document.querySelectorAll('.nav-statistics__item')];
  if (names.length !== 1 || metrics.length > 20) throw new Error('ONECREW_BILIBILI_PROFILE_INVALID');
  const followers = metrics.filter(item => {
    const labels = item.querySelectorAll('.nav-statistics__item-text');
    return labels.length === 1 && labels[0].textContent?.trim() === '粉丝数';
  });
  if (followers.length !== 1) throw new Error('ONECREW_BILIBILI_PROFILE_INVALID');
  const values = followers[0].querySelectorAll('.nav-statistics__item-num');
  if (values.length !== 1) throw new Error('ONECREW_BILIBILI_PROFILE_INVALID');
  // Observed native header biography; never use the entire page, video cards,
  // comments, or a title tooltip that disagrees with the original text.
  const signs = document.querySelectorAll('.upinfo-detail__bottom .sign.header-sign .pure-text');
  const original = signs.length === 1 ? signs[0].textContent?.trim() : undefined;
  const tooltip = signs.length === 1 ? signs[0].getAttribute('title') : null;
  const bio = typeof original === 'string' && [...original].length <= 1000
    && (!tooltip || tooltip.trim() === original) ? original : undefined;
  // Observed author-owned announcement, isolated from video descriptions/comments.
  const announcements = document.querySelectorAll('.ann-section');
  let announcement;
  if (announcements.length === 1
      && announcements[0].querySelector('.ann-section__header')?.textContent?.trim() === '公告') {
    const content = announcements[0].querySelectorAll('.ann-section__content .show-text');
    if (content.length === 1) {
      const paragraphs = [...content[0].querySelectorAll('p')];
      announcement = paragraphs.length
        ? paragraphs.map(p => p.textContent?.trim() ?? '').join('\n')
        : content[0].textContent?.trim();
    }
  }
  const parts = [bio, announcement].filter(value => typeof value === 'string' && value.length > 0);
  const sign = parts.length ? [...new Set(parts)].join('\n') : undefined;
  return [project({ mid: match[1], name: names[0].textContent, label: '粉丝数',
    follower_count_text: values[0].getAttribute('title'), follower_display: values[0].textContent?.trim(), sign }, expectedMid)];
}

export async function readBilibiliProfile(page, mid) {
  if (typeof mid !== 'string' || !/^[1-9][0-9]{0,19}$/.test(mid)) throw new Error('ONECREW_BILIBILI_PROFILE_INPUT');
  const identity = await page.newTab('https://space.bilibili.com/' + mid);
  if (!identity) throw new Error('ONECREW_BILIBILI_PROFILE_IDENTITY');
  let readFailed = false;
  try {
    await page.setActivePage(identity);
    await page.wait({ time: 3 });
    await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
    const result = await page.evaluate(`(${readProfilePage.toString()})(${JSON.stringify(mid)}, ${projectProfile.toString()})`);
    const rows = result && 'session' in result && 'data' in result ? result.data : result;
    if (!Array.isArray(rows) || rows.length !== 1) throw new Error('ONECREW_BILIBILI_PROFILE_INVALID');
    return rows;
  } catch (error) {
    readFailed = true;
    throw error;
  } finally {
    try { await page.closeTab(identity); }
    catch (error) { if (!readFailed) throw error; }
  }
}
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';
