import {douyinAuthor, douyinVideo, observeDouyinEvidence} from './douyin-evidence.mjs';

export function projectDouyinComments(data, video, author) {
  douyinVideo(video); douyinAuthor(author);
  const source=`https://www.douyin.com/video/${video}`;
  if(data?.post_id!==video || data.author_id!==author || data.source_url!==source
      || typeof data.text!=='string' || !data.text.trim() || data.text.includes('\0')
      || new TextEncoder().encode(data.text).length>12000 || typeof data.text_truncated!=='boolean'
      || !Array.isArray(data.comments) || data.comments.length>5) throw new Error('ONECREW_DOUYIN_INVALID');
  const seen=new Set();
  const comments=data.comments.map(row=>{
    if(typeof row?.id!=='string' || !/^[1-9]\d{0,19}$/.test(row.id) || row.parent_post_id!==video
        || seen.has(row.id) || typeof row.text!=='string' || row.text.includes('\0')
        || new TextEncoder().encode(row.text).length>2000) throw new Error('ONECREW_DOUYIN_INVALID');
    seen.add(row.id);
    return {id:row.id,parent_post_id:video,text:row.text};
  });
  return {post_id:video,author_id:author,source_url:source,text:data.text,text_truncated:data.text_truncated,
    comments,evidence_source:'visible_same_video_comments'};
}

// The native comment panel contains a related-search link with both aweme_id
// and gid. Require that parent binding and native tooltip comment IDs; never
// manufacture IDs from array positions or read replies/another video's panel.
export function observeDouyinComments(source, video, author, caption) {
  if(location.href!==source || caption?.post_id!==video || caption.author_id!==author
      || caption.source_url!==source) throw new Error('ONECREW_DOUYIN_INVALID');
  const visible=node=>node.getClientRects().length && !node.closest('[aria-hidden="true"],nav,aside,footer');
  const lists=[...document.querySelectorAll('[data-e2e="comment-list"]')].filter(visible);
  if(lists.length!==1) return null;
  const list=lists[0], panel=list.parentElement;
  if(!panel || panel.querySelectorAll('[data-e2e="comment-list"]').length!==1) return null;
  const roots=[...panel.querySelectorAll('a[href]')].filter(visible).map(a=>{
    try {const u=new URL(a.href);return u.origin===location.origin && u.pathname.startsWith('/search/')
      && u.searchParams.get('source')==='comment_related_search'?u:null;} catch{return null;}
  }).filter(Boolean);
  if(!roots.length || roots.some(u=>u.searchParams.getAll('aweme_id').length!==1
      || u.searchParams.getAll('gid').length!==1 || u.searchParams.get('aweme_id')!==video
      || u.searchParams.get('gid')!==video)) throw new Error('ONECREW_DOUYIN_INVALID');
  const rows=[...list.querySelectorAll('[data-e2e="comment-item"]')].filter(visible)
    .filter(row=>!row.parentElement?.closest('[data-e2e="comment-item"]')).slice(0,5);
  if(!rows.length) return null;
  const comments=[];
  for(const row of rows) {
    if([...row.querySelectorAll('button,[role="button"]')].some(node=>visible(node)
        && /^(Show original|See original|View original|显示原文|查看原文|顯示原文)$/i.test(node.textContent.trim())))
      throw new Error('ONECREW_ORIGINAL_TEXT_UNCONFIRMED');
    const more=[...row.querySelectorAll('[data-e2e="video-comment-more"]')];
    if(more.length!==1 || more[0].children.length!==1) return null;
    const id=more[0].children[0].id.match(/^tooltip_([1-9]\d{0,19})$/)?.[1];
    const body=more[0].parentElement?.nextElementSibling;
    // The observed text span precedes image attachments and stats. Its empty
    // text remains an empty one of the first five, never replaced by a sixth.
    if(!id || body?.tagName!=='DIV' || body.firstElementChild?.tagName!=='SPAN') return null;
    const walker=document.createTreeWalker(body.firstElementChild,NodeFilter.SHOW_TEXT);
    let text='',bytes=0;
    original: while(walker.nextNode()) {
      const node=walker.currentNode;
      if(node.parentElement?.closest('button,[role="button"],[aria-hidden="true"],[contenteditable="true"]')) continue;
      for(const char of node.textContent||'') {const size=new TextEncoder().encode(char).length;
        if(bytes+size>2000) break original; text+=char; bytes+=size;}
      if(bytes>=2000) break;
    }
    comments.push({id,parent_post_id:video,text:text.trim()});
  }
  return {post_id:video,author_id:author,source_url:source,text:caption.text,
    text_truncated:caption.text_truncated,comments};
}

export async function readDouyinComments(page, video, author, ownedPage) {
  douyinVideo(video);douyinAuthor(author);
  const source=`https://www.douyin.com/video/${video}`;
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:1500});
    for(let attempt=0;attempt<12;attempt++) {
      const script=`(()=>{const caption=(${observeDouyinEvidence.toString()})('content',${JSON.stringify(video)},${JSON.stringify(source)},false);return caption?(${observeDouyinComments.toString()})(${JSON.stringify(source)},${JSON.stringify(video)},${JSON.stringify(author)},caption):null;})()`;
      const raw=await owned.evaluate(script);
      const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
      if(data) return projectDouyinComments(data,video,author);
      if(attempt<11)await owned.sleep(0.5);
    }
    throw new Error('ONECREW_DOUYIN_INVALID');
  });
}
