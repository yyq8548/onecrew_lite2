// Douyin's public sec_uid identifies an author. Display names, the editable
// Douyin number and recommendation-link author_id parameters do not.
export function douyinAuthor(value) {
  if (typeof value !== 'string' || !/^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(value)) throw new Error('ONECREW_DOUYIN_INPUT');
  return value;
}
export function douyinVideo(value) {
  if (typeof value !== 'string' || !/^[1-9]\d{0,19}$/.test(value)) throw new Error('ONECREW_DOUYIN_INPUT');
  return value;
}
const invalid = () => { throw new Error('ONECREW_DOUYIN_INVALID'); };
const text = (value, maximum) => typeof value === 'string' && !value.includes('\0') && new TextEncoder().encode(value).length <= maximum;
export function douyinExactFollowers(value) {
  const match = /^(0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+)$/.exec(value);
  const count = match ? Number(match[1].replaceAll(',', '')) : null;
  return Number.isSafeInteger(count) ? count : null;
}
function links(values) {
  if (!Array.isArray(values) || values.length > 10) invalid();
  for (const value of values) {
    if (!text(value, 2048) || /\s|[\u0000-\u001f\u007f]/.test(value)) invalid();
    let url; try { url = new URL(value); } catch { invalid(); }
    if (url.protocol !== 'https:' || url.username || url.password || url.port
        || !url.hostname.includes('.') || url.hostname.endsWith('.')
        || /(?:^|\.)(?:localhost|local|internal|test|invalid|onion|home\.arpa)$/.test(url.hostname)
        || /^\d+\.\d+\.\d+\.\d+$/.test(url.hostname) || url.hostname.startsWith('[')) invalid();
  }
  return [...new Set(values)];
}
export function projectDouyinProfile(data, author) {
  douyinAuthor(author);
  if (data?.author_id !== author || data.profile_url !== `https://www.douyin.com/user/${author}`
      || !text(data.author_label, 512) || !data.author_label.trim() || /[\u0000-\u001f\u007f]/.test(data.author_label)
      || !text(data.bio, 8000) || !text(data.follower_text, 100) || typeof data.bio_truncated !== 'boolean') invalid();
  return {adapter_version:'1.0.0', author_id:author, author_label:data.author_label.trim(),
    profile_url:data.profile_url, follower_count:douyinExactFollowers(data.follower_text),
    follower_text:data.follower_text, bio:data.bio, bio_truncated:data.bio_truncated,
    public_links:links(data.public_links), evidence_source:'visible_profile_header'};
}
export function projectDouyinContent(data, video, expectedAuthor) {
  douyinVideo(video);
  if (expectedAuthor !== undefined) douyinAuthor(expectedAuthor);
  if (!data || data.post_id !== video || data.source_url !== `https://www.douyin.com/video/${video}`
      || (expectedAuthor !== undefined && data.author_id !== expectedAuthor)
      || !text(data.author_label,512) || !data.author_label.trim() || /[\u0000-\u001f\u007f]/.test(data.author_label)
      || !text(data.text,12000) || !data.text.trim() || typeof data.text_truncated !== 'boolean') invalid();
  douyinAuthor(data.author_id);
  if (data.profile_url !== `https://www.douyin.com/user/${data.author_id}`) invalid();
  return {post_id:video, author_id:data.author_id, author_label:data.author_label,
    profile_url:data.profile_url, source_url:data.source_url, text:data.text,
    text_truncated:data.text_truncated, evidence_source:'visible_native_video_caption', ...(data.observations?{observations:data.observations}:{})};
}

// Serialized into one owned target. Never return the feed, chapter summaries,
// comments, location, age, operator state, media URLs or platform footer contacts.
export function observeDouyinEvidence(kind, id, source, expandBio = false) {
  if (location.origin !== 'https://www.douyin.com' || location.href !== source) throw new Error('ONECREW_DOUYIN_INVALID');
  const visible = node => node.getClientRects().length && !node.closest('[aria-hidden="true"],nav,aside,footer');
  if ([...document.querySelectorAll('iframe[src*="captcha"],.captcha-container,.verify-dialog')].some(visible)) throw new Error('ONECREW_DOUYIN_RISK');
  if ([...document.querySelectorAll('.login-container,.login-modal,.login-main')].some(node => visible(node) && /登录|扫码/.test(node.innerText))) throw new Error('ONECREW_DOUYIN_AUTH');
  if (document.documentElement.classList.contains('translated-ltr') || document.documentElement.classList.contains('translated-rtl')) throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
  const headers = [...document.querySelectorAll('[data-e2e="user-info"]')].filter(visible);
  if (!headers.length || kind==='profile' && headers.length!==1) return {pending:'author_header'};
  const header = headers[0];
  const bounded = (value, maximum) => {
    let output='', bytes=0, truncated=false;
    for (const character of String(value || '').trim()) {
      const size=new TextEncoder().encode(character).length;
      if (bytes+size>maximum) {truncated=true;break;} output+=character;bytes+=size;
    }
    return {text:output,truncated};
  };
  const plain = root => {
    const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT); let output='';
    while(walker.nextNode()) {
      const node=walker.currentNode;
      if (node.parentElement && !node.parentElement.closest('button,[role="button"],[aria-hidden="true"],[contenteditable="true"]')) output+=node.textContent;
    }
    return output.trim();
  };
  if (kind === 'content') {
    if (!/^[1-9]\d{0,19}$/.test(id)) throw new Error('ONECREW_DOUYIN_INVALID');
    const roots=[...document.querySelectorAll('[data-e2e="detail-video-info"]')].filter(visible);
    if(roots.length!==1) return {pending:'content_root'};
    const titles=[...roots[0].querySelectorAll('h1')].filter(visible);
    const authors=headers.flatMap(node=>[...node.querySelectorAll('a[href]')]).filter(visible);
    const paths=[...new Set(authors.map(a=>{const url=new URL(a.href);return url.origin+url.pathname.replace(/\/$/,'');}).filter(url=>/^https:\/\/www\.douyin\.com\/user\/MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(url)))];
    if (titles.length !== 1 || paths.length !== 1) return {pending:titles.length!==1?'caption_title':'author_link'};
    const author=paths[0].match(/^https:\/\/www\.douyin\.com\/user\/(MS4wLjABAAAA[A-Za-z0-9_-]{8,88})$/)?.[1];
    if (!author) throw new Error('ONECREW_DOUYIN_INVALID');
    const names=[...new Set(authors.map(plain).filter(Boolean))];
    if (names.length !== 1) return {pending:'author_label'};
    const caption=bounded(plain(titles[0]),12000);
    if (!caption.text) return {pending:'caption_text'};
    const published=roots[0].querySelector('[data-e2e="detail-video-publish-time"]');
    const share=roots[0].querySelector('[data-e2e="video-share-icon-container"]');
    const shareText=share&&visible(share)?share.innerText.trim():'';
    return {post_id:id,author_id:author,author_label:bounded(names[0],512).text,
      profile_url:paths[0],source_url:source,text:caption.text,text_truncated:caption.truncated,
      observations:{published_at:published&&visible(published)?published.innerText.replace(/^发布时间[：:]\s*/,'').trim()||null:null,
        share_count:/^\d+$/.test(shareText)&&Number.isSafeInteger(Number(shareText))?Number(shareText):null,text_completeness:caption.truncated?'truncated':'unknown'}};
  }
  if (kind !== 'profile' || !/^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/.test(id)) throw new Error('ONECREW_DOUYIN_INVALID');
  const titles=[...header.querySelectorAll('h1')].filter(visible);
  const fans=[...header.querySelectorAll('[data-e2e="user-info-fans"]')].filter(visible);
  if (titles.length !== 1 || fans.length !== 1) return null;
  const followerText=fans[0].innerText.replace(/^粉丝\s*/, '').trim();
  if(!followerText) return null;
  // The observed biography block follows the metadata paragraph in this header.
  // Require that exact structure; never read a broader ancestor or the works grid.
  const metadata=[...header.children].filter(node=>node.tagName==='P' && /^抖音号：/.test(plain(node)));
  if (metadata.length !== 1) return null;
  const bioRoot=metadata[0].nextElementSibling;
  if (!bioRoot || bioRoot.tagName !== 'DIV') return null;
  // 简介限定在元信息后的相邻区块，后续群聊等区域不参与读取。
  const expanded=[...bioRoot.querySelectorAll('p')].filter(visible);
  const more=[...bioRoot.querySelectorAll('*')].filter(node=>visible(node) && !node.children.length && node.textContent.trim()==='更多');
  if (expanded.length > 1 || more.length > 1) throw new Error('ONECREW_DOUYIN_INVALID');
  if (!expanded.length && more.length && expandBio) {
    more[0].setAttribute('data-onecrew-douyin-bio-more','true');
    more[0].click();return {expand_requested:true};
  }
  const bio=bounded(expanded.length ? plain(expanded[0]) : plain(bioRoot).replace(/更多$/, '').trim(),8000);
  const urls=[];
  for (const anchor of bioRoot.querySelectorAll('a[href]')) {
    if (!visible(anchor)) continue;
    let url;try{url=new URL(anchor.href);}catch{continue;}
    if (url.protocol==='mailto:') {
      let email;try{email=decodeURIComponent(url.pathname);}catch{continue;}
      if (email.length<=320&&!/[\u0000-\u0020\u007f]/.test(email)) {
        const extra=bounded(`${bio.text}\n${email}`,8000);bio.text=extra.text;bio.truncated||=extra.truncated;
      }
    } else if (url.protocol==='https:' && url.origin!==location.origin && !url.username&&!url.password&&!url.port
        && url.href.length<=2048&&!urls.includes(url.href)&&urls.length<10) urls.push(url.href);
  }
  return {author_id:id,author_label:bounded(plain(titles[0]),512).text,profile_url:source,
    expansion_pending:!expanded.length&&more.length>0,
    follower_text:bounded(followerText,100).text,bio:bio.text,bio_truncated:bio.truncated||(!expanded.length&&more.length>0),public_links:urls};
}

export async function readDouyinEvidence(page, kind, id, expectedAuthor, ownedPage) {
  if (kind==='profile') douyinAuthor(id); else if (kind==='content') douyinVideo(id); else throw new Error('ONECREW_DOUYIN_INPUT');
  if (expectedAuthor !== undefined) douyinAuthor(expectedAuthor);
  const source=`https://www.douyin.com/${kind==='profile'?'user':'video'}/${id}`;
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:1500});
    let expanded=false,pending=null;
    for(let attempt=0;attempt<12;attempt++) {
      const raw=await owned.evaluate(`(${observeDouyinEvidence.toString()})(${JSON.stringify(kind)},${JSON.stringify(id)},${JSON.stringify(source)},${!expanded})`);
      const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
      if(data?.pending) pending=data.pending;
      else if(data?.expand_requested===true) {
        expanded=true;
        // The native biography popup opens on hover. A DOM click does not
        // dispatch the pointer event used by this profile's disclosure.
        // The preceding observation validated the unique metadata/bio boundary.
        await owned.hover('[data-e2e="user-info"] [data-onecrew-douyin-bio-more="true"]');
      }
      else if(data && !(kind==='profile' && expanded && data.expansion_pending===true && attempt<11))
        return kind==='profile'?projectDouyinProfile(data,id):projectDouyinContent(data,id,expectedAuthor);
      if(attempt<11)await owned.sleep(0.5);
    }
    throw new Error('ONECREW_DOUYIN_INVALID'+(pending?':'+pending:''));
  });
}
