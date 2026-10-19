// Read native fields for an already observed Pixiv author or illustration.
// Never reinterpret following/likes as followers, or search text as profile data.
export function pixivId(value) {
  if(typeof value!=='string'||!/^[1-9][0-9]{0,19}$/.test(value)) throw new Error('ONECREW_PIXIV_INPUT');
  return value;
}
const invalid=()=>{throw new Error('ONECREW_PIXIV_INVALID');};
const text=(value,max)=>{
  if(typeof value!=='string'||value.includes('\0')||Buffer.byteLength(value)>max) invalid();
  return value;
};
export function projectPixivProfile(raw,id) {
  pixivId(id);
  if(raw?.userId!==id||typeof raw.name!=='string'||!raw.name.trim()) invalid();
  const bio=text(raw.comment,8000), name=text(raw.name,512);
  if(!Array.isArray(raw.links)||raw.links.length>10) invalid();
  const links=[];
  for(const link of raw.links) {
    text(link,2048);
    let url;try{url=new URL(link);}catch{invalid();}
    if(url.protocol!=='https:'||url.username||url.password||url.port||/[\u0000-\u0020\u007f]/.test(link)) invalid();
    if(!links.includes(url.href)) links.push(url.href);
  }
  return {adapter_version:'1.0.0',author_id:id,author_label:name,profile_url:`https://www.pixiv.net/users/${id}`,
    follower_count:null,bio,public_links:links,evidence_source:'native_ajax_response'};
}
export function projectPixivContent(raw,id,authorId) {
  pixivId(id);pixivId(authorId);
  if(raw?.illustId!==id||raw.userId!==authorId||typeof raw.title!=='string'||!raw.title.trim()||typeof raw.truncated!=='boolean') invalid();
  const title=text(raw.title,512), description=text(raw.description,15000);
  return {post_id:id,author_id:authorId,source_url:`https://www.pixiv.net/artworks/${id}`,
    text:`${title}\n${description}`,text_truncated:raw.truncated};
}

// The function is serialized into the owned browser target. No raw response or
// session material leaves it, and the request can only read the selected entity.
export async function fetchPixivEvidence(kind,id,source) {
  if(location.origin!=='https://www.pixiv.net'||location.href!==source) return {stop:'invalid'};
  if(document.querySelector('form[action*="login"]')) return {stop:'auth'};
  if(document.querySelector('iframe[src*="captcha"],[id*="captcha"]')) return {stop:'risk'};
  if(!['profile','content'].includes(kind)||!/^[1-9][0-9]{0,19}$/.test(id)) return {stop:'invalid'};
  const response=await fetch(kind==='profile'?`/ajax/user/${id}?full=1`:`/ajax/illust/${id}`,{
    credentials:'include',redirect:'error',signal:AbortSignal.timeout(15000)});
  if(response.status===401)return {stop:'auth'};
  if(response.status===403||response.status===429)return {stop:'risk'};
  if(!response.ok||!response.body)return {stop:'read'};
  const reader=response.body.getReader(),chunks=[];let size=0;
  while(true){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;
    if(size>200000){await reader.cancel();return {stop:'read'};}chunks.push(value);}
  const bytes=new Uint8Array(size);let offset=0;
  for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let data;try{data=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{return {stop:'read'};}
  if(data?.error!==false||!data.body||typeof data.body!=='object'||Array.isArray(data.body))return {stop:'read'};
  const body=data.body;
  const plain=html=>{
    if(typeof html!=='string')return null;
    const template=document.createElement('template');template.innerHTML=html;
    for(const node of template.content.querySelectorAll('script,style,iframe'))node.remove();
    return {value:template.content.textContent||'',links:[...template.content.querySelectorAll('a[href]')].map(a=>a.getAttribute('href'))};
  };
  if(kind==='profile'){
    const comment=plain(body.comment);if(!comment)return {stop:'read'};
    const candidates=comment.links;
    if(typeof body.webpage==='string'&&body.webpage)candidates.push(body.webpage);
    const links=[];let biography=comment.value;
    for(const link of candidates){
      if(typeof link!=='string'||link.length>2048||/[\u0000-\u0020\u007f]/.test(link))continue;
      try{const url=new URL(link);
        if(url.protocol==='mailto:'){
          const email=decodeURIComponent(url.pathname);
          if(email.length<=320&&!/[\u0000-\u0020\u007f]/.test(email))biography+=`\n${email}`;
        }else if(url.protocol==='https:'&&!url.username&&!url.password&&!url.port&&!links.includes(url.href)&&links.length<10)links.push(url.href);
      }catch{}
    }
    return {userId:body.userId,name:body.name,comment:biography,links};
  }
  const description=plain(body.description);if(!description)return {stop:'read'};
  // UTF-8 bound prevents multilingual receipts overflowing the owner contract.
  let value='',truncated=false,used=0;
  for(const character of description.value){const count=new TextEncoder().encode(character).length;
    if(used+count>15000){truncated=true;break;}value+=character;used+=count;}
  return {illustId:body.illustId,userId:body.userId,title:body.illustTitle,description:value,truncated};
}

export async function readPixivEvidence(page,kind,id,authorId,ownedPage) {
  pixivId(id);if(kind==='content')pixivId(authorId);
  if(!['profile','content'].includes(kind))throw new Error('ONECREW_PIXIV_INPUT');
  const source=`https://www.pixiv.net/${kind==='profile'?'users':'artworks'}/${id}`;
  return ownedPage(page,async owned=>{
    await owned.goto(source,{settleMs:1500});
    const raw=await owned.evaluate(`(${fetchPixivEvidence.toString()})(${JSON.stringify(kind)},${JSON.stringify(id)},${JSON.stringify(source)})`);
    const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    if(data?.stop)throw new Error(`ONECREW_PIXIV_${String(data.stop).toUpperCase()}`);
    return kind==='profile'?projectPixivProfile(data,id):projectPixivContent(data,id,authorId);
  });
}
