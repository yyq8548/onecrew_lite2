// Public video-page fallback when Bilibili's view API does not return JSON.
// The page's VideoObject and visible title/author must agree with the saved hit.
export function projectVideo(observation, expectedBvid, expectedMid) {
  const fail = () => { throw new Error('ONECREW_BILIBILI_VIDEO_INVALID'); };
  const validText = (value, max) => typeof value === 'string' && value.trim().length > 0
    && [...value].length <= max && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
  if (!/^BV[0-9A-Za-z]{10}$/.test(expectedBvid) || !/^[1-9][0-9]{0,19}$/.test(expectedMid)) fail();
  if (observation?.url !== `https://www.bilibili.com/video/${expectedBvid}/`
      || observation?.author_url !== `https://space.bilibili.com/${expectedMid}`
      || !validText(observation.title, 500) || !validText(observation.author, 200)
      || observation.title !== observation.visible_title
      || observation.author !== observation.visible_author
      || typeof observation.description !== 'string'
      || [...observation.description].length > 16000
      || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/.test(observation.description)
      || observation.payment_marker) fail();
  // Empty metrics and payment fields mean the page did not verify them. They
  // must never be used as proof of reach or free playback downstream.
  const values = {
    bvid: expectedBvid, aid: '', title: observation.title, author: `${observation.author} (mid: ${expectedMid})`,
    category: '', publish_time: observation.publish_time ?? '', duration: '', view: observation.view ?? '', danmaku: '', reply: observation.reply ?? '', like: observation.like ?? '',
    coin: '', favorite: '', share: '', parts: '', thumbnail: '', description: observation.description,
    requires_payment: 'false', payment_type: '', pay_preview: 'false', redirect_url: '',
  };
  return Object.entries(values).map(([field, value]) => ({ field, value }));
}

export function readVideoPage(expectedBvid, expectedMid, project) {
  const current = new URL(location.href);
  if (current.origin !== 'https://www.bilibili.com'
      || current.pathname !== `/video/${expectedBvid}/`
      || current.username || current.password || current.port) throw new Error('ONECREW_BILIBILI_VIDEO_ORIGIN');
  const text = (document.body?.innerText ?? '').slice(0, 100000);
  if (/验证码|安全验证|操作频繁|访问受限|captcha|too many requests/i.test(text)) throw new Error('ONECREW_BILIBILI_VIDEO_RISK');
  const metadata = [...document.querySelectorAll('script[type="application/ld+json"]')]
    .map(script => { try { return JSON.parse(script.textContent); } catch { return null; } })
    .filter(item => item?.['@type'] === 'VideoObject');
  const titles = document.querySelectorAll('h1.video-title');
  const authors = document.querySelectorAll('a.up-name[href]');
  if (metadata.length !== 1 || titles.length !== 1 || authors.length !== 1) {
    // A public video can contain a generic "please log in" prompt for likes
    // while its content remains visible. Only classify a wall when the actual
    // video evidence is missing.
    if (/请先登录|登录后查看/.test(text)) throw new Error('ONECREW_BILIBILI_VIDEO_AUTH');
    throw new Error('ONECREW_BILIBILI_VIDEO_INVALID');
  }
  const data = metadata[0];
  const authorUrl = new URL(data.author?.url ?? '', location.href);
  const visibleAuthorUrl = new URL(authors[0].href);
  if (authorUrl.origin !== 'https://space.bilibili.com' || visibleAuthorUrl.origin !== authorUrl.origin
      || authorUrl.pathname !== `/${expectedMid}` || visibleAuthorUrl.pathname !== `/${expectedMid}/`)
    throw new Error('ONECREW_BILIBILI_VIDEO_INVALID');
  const metric = action => {
    const rows=Array.isArray(data.interactionStatistic)?data.interactionStatistic:[];
    const matches=rows.filter(row=>row.interactionType===`https://schema.org/${action}` || row.interactionType===`http://schema.org/${action}` || row.interactionType?.['@type']===action);
    const value=matches.length===1?String(matches[0].userInteractionCount):'';
    return /^\d+$/.test(value)&&Number.isSafeInteger(Number(value))?value:'';
  };
  return project({ url: data.url, author_url: authorUrl.origin + authorUrl.pathname,
    title: data.name, visible_title: titles[0].textContent?.trim(), author: data.author?.name,
    visible_author: authors[0].textContent?.trim(), description: data.description,
    publish_time:typeof data.uploadDate==='string'?data.uploadDate:'',view:metric('WatchAction'),like:metric('LikeAction'),reply:metric('CommentAction'),
    payment_marker: Boolean(document.querySelector('[class*="paywall"], [class*="ugc-pay"], [class*="upower-exclusive"]')),
  }, expectedBvid, expectedMid);
}

export async function readBilibiliVideo(page, bvid, mid) {
  if (typeof bvid !== 'string' || !/^BV[0-9A-Za-z]{10}$/.test(bvid)
      || typeof mid !== 'string' || !/^[1-9][0-9]{0,19}$/.test(mid)) throw new Error('ONECREW_BILIBILI_VIDEO_INPUT');
  const identity = await page.newTab(`https://www.bilibili.com/video/${bvid}/`);
  if (!identity) throw new Error('ONECREW_BILIBILI_VIDEO_IDENTITY');
  let readFailed = false;
  try {
    await page.setActivePage(identity);
    await page.sleep(2);
    await page.wait({ selector: 'h1.video-title', timeout: 10 });
    await page.evaluate(`(${restoreOriginalLanguage.toString()})()`);
    const output = await page.evaluate(`(${readVideoPage.toString()})(${JSON.stringify(bvid)}, ${JSON.stringify(mid)}, ${projectVideo.toString()})`);
    const rows = output && 'session' in output && 'data' in output ? output.data : output;
    if (!Array.isArray(rows) || rows.length !== 21) throw new Error('ONECREW_BILIBILI_VIDEO_INVALID');
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
