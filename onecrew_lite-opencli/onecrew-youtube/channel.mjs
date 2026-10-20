import { enrichYoutubeHiddenEmail } from './hidden-email.mjs';

// 只提取作者的频道资料，不将评论或推荐内容用作联系方式证据。
export function projectChannelData(data, channelId) {
  const fail = () => { throw new Error('ONECREW_CHANNEL_EVIDENCE_INVALID'); };
  if (!/^UC[A-Za-z0-9_-]{22}$/.test(channelId) || !data || typeof data!=='object' || Array.isArray(data) || new TextEncoder().encode(JSON.stringify(data)).length>8388608) fail();
  const meta = data?.metadata?.channelMetadataRenderer;
  if (meta?.externalId !== channelId || typeof meta.title !== 'string' || (meta.description != null && typeof meta.description !== 'string')) fail();
  const bounded = (s, max) => { if (typeof s!=='string' || s.includes('\0') || new TextEncoder().encode(s).length>max) fail(); return s; };
  const text = v => typeof v==='string' ? v : v?.simpleText ?? v?.content ?? (v?.runs ?? []).map(r=>r.text??'').join('');
  const details = [], links = []; let hiddenEmailAvailable = false, aboutDataAvailable = false, linksTruncated = false;
  function link(raw) {
    if (typeof raw!=='string' || raw.length>2048 || raw.includes('...') || raw.includes('…')) return;
    try { let u=new URL(raw,'https://www.youtube.com');
      if (u.origin==='https://www.youtube.com' && u.pathname==='/redirect') u=new URL(u.searchParams.get('q'));
      if(['https:','http:'].includes(u.protocol) && !u.username && !u.password && !u.port && !links.includes(u.href)) {
        if(links.length<50) links.push(u.href); else linksTruncated=true;
      }
    } catch {}
  }
  // Recursion only locates known about renderers; contents of other renderers aren't evidence.
  let visited=0;
  function walk(v) {
    if(!v || typeof v!=='object') return; if(++visited>100000) fail();
    const about=v.channelAboutFullMetadataRenderer ?? v.aboutChannelRenderer ?? v.aboutChannelViewModel;
    if(about) {
      if(about.channelId != null && about.channelId !== channelId) fail();
      aboutDataAvailable = true;
      // Native email-specific fields establish an entry, independent of localized labels.
      const label=text(about.businessEmailLabel);
      hiddenEmailAvailable ||= Boolean(label.trim() || about.businessEmailButton || about.businessEmailRevealButton);
      const description=text(about.description); if(description) details.push(bounded(description,16000));
      const email=text(about.businessEmail); if(email) details.push(bounded(email,2048));
      function aboutLinks(node) { if(!node || typeof node!=='object') return;
        link(node.urlEndpoint?.url); link(node.channelExternalLinkViewModel?.link?.commandRuns?.[0]?.onTap?.innertubeCommand?.urlEndpoint?.url);
        for(const value of Object.values(node)) if(typeof value==='object') aboutLinks(value);
      }
      aboutLinks(about);
    }
    for(const value of Object.values(v)) if(typeof value==='object') walk(value);
  }
  walk(data.contents); walk(data.onResponseReceivedEndpoints); walk(data.onResponseReceivedActions); walk(data.engagementPanels);
  const description=bounded([...new Set([...details,meta.description ?? ''])].filter(Boolean).join('\n'),64000);
  return {adapterVersion:'1.0.0', channelId, sourceUrl:'https://www.youtube.com/channel/'+channelId,
    title:bounded(meta.title,500), description, publicLinks:links, linksTruncated,
    descriptionStatus:description?aboutDataAvailable?'complete':'unknown':'missing', hiddenEmailAvailable, aboutDataAvailable};
}
export async function readChannelPage(channelId, project) {
  if(location.origin!=='https://www.youtube.com') throw new Error('ONECREW_ORIGIN_MISMATCH');
  // /about opens the native description panel. Let YouTube own its authenticated
  // continuation; do not copy credentials or use an unauthenticated fetch fallback.
  // 此处仅提取原生资料，隐藏邮箱交由后续受限流程处理。
  for(let attempt=0;attempt<20;attempt++) {
    const meta=window.ytInitialData?.metadata?.channelMetadataRenderer;
    const panels=[...document.querySelectorAll('ytd-about-channel-renderer')]
      .map(el=>el.data?.metadata?.aboutChannelViewModel).filter(Boolean);
    if(meta && meta.externalId!==channelId) throw new Error('ONECREW_CHANNEL_ID_MISMATCH');
    if(panels.some(panel=>panel.channelId!==channelId)) throw new Error('ONECREW_CHANNEL_ID_MISMATCH');
    if(meta && panels.length===1) {
      return project({
        metadata:{channelMetadataRenderer:meta},contents:{aboutChannelViewModel:panels[0]},
      },channelId);
    }
    await new Promise(resolve=>setTimeout(resolve,400));
  }
  throw new Error('ONECREW_CHANNEL_ABOUT_UNAVAILABLE');
}
export async function readChannelEvidence(page, channelId, task) {
  if(!/^UC[A-Za-z0-9_-]{22}$/.test(channelId)) throw new Error('ONECREW_CHANNEL_ID_INVALID');
  await page.goto('https://www.youtube.com/channel/'+channelId+'/about',{waitUntil:'none'}); await page.wait(2);
  const result=await page.evaluate(`(${readChannelPage.toString()})(${JSON.stringify(channelId)}, ${projectChannelData.toString()})`);
  const data=result && 'session' in result && 'data' in result ? result.data : result;
  if(!data || data.channelId!==channelId || data.adapterVersion!=='1.0.0') throw new Error('ONECREW_CHANNEL_EVIDENCE_INVALID');
  return [await enrichYoutubeHiddenEmail(page, channelId, data, task)];
}
