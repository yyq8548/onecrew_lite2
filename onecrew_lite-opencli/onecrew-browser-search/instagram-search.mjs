export function projectInstagramVisibleSearch(rows, limit) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || !Array.isArray(rows) || rows.length > 100) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  const seen = new Set();
  return rows.flatMap(row => {
    if (typeof row?.username !== 'string' || !/^[a-z0-9_.]{1,30}$/.test(row.username)
      || row.url !== `https://www.instagram.com/${row.username}/` || typeof row.name !== 'string'
      || row.name.includes('\0') || row.author_id !== undefined) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    if (seen.has(row.username)) return [];
    seen.add(row.username);
    return [{provider:'onecrew-instagram-visible-v1',execution_mode:'opencli_dom',username:row.username,
      name:Array.from(row.name || row.username).slice(0,200).join(''),url:row.url}];
  }).slice(0,limit);
}

export function observeInstagramVisibleSearch(query) {
  if (location.origin !== 'https://www.instagram.com') return {stop:'read_redirect'};
  if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
  const visible=node=>node.getClientRects().length>0;
  const input=[...document.querySelectorAll('input[aria-label="Search input"],input[aria-label="搜索输入"]')].filter(visible);
  if (input.length!==1) {
    if (input.length>1) return {stop:'layout'};
    const entry=[...document.querySelectorAll('a[href="/explore/"]')].filter(visible);
    return {ready:false,...(query===null&&entry.length===1?{entry:'a[href="/explore/"]'}:{})};
  }
  if(query===null) return {ready:true};
  if(input[0].value!==query) return {ready:false};
  const scope=input[0].closest('main');
  if(!scope) throw new Error('ONECREW_INSTAGRAM_SEARCH_LAYOUT');
  const body=scope.innerText;
  if(/Try again later|请求太频繁|稍后重试|操作过于频繁/i.test(body)) return {stop:'risk'};
  const busy=[...scope.querySelectorAll('[role="status"],[role="progressbar"],[aria-busy="true"]')].some(node=>visible(node)&&(/Loading|正在加载|加载中/i.test(node.innerText)||node.getAttribute('aria-busy')==='true'));
  if(busy) return {ready:false};
  const keyword=[...scope.querySelectorAll('a[href]')].some(anchor=>{
    const url=new URL(anchor.href,location.href);
    return visible(anchor)&&url.origin===location.origin&&url.pathname==='/explore/search/keyword/'&&url.searchParams.get('q')===query;
  });
  const empty=/No results found|No results|未找到结果|没有找到结果/i.test(body);
  if(!keyword&&!empty) return {ready:false};
  const profiles=[];
  for(const anchor of scope.querySelectorAll('a[href]')) {
    if(!visible(anchor)) continue;
    const url=new URL(anchor.href,location.href),match=/^\/([a-z0-9_.]{1,30})\/$/i.exec(url.pathname);
    if(url.origin!==location.origin||url.search||url.hash||!match||['explore','accounts','reels','direct','stories'].includes(match[1].toLowerCase())) continue;
    const username=match[1].toLowerCase(),lines=anchor.innerText.split('\n').map(line=>line.trim()).filter(Boolean);
    if(!lines.some(line=>line.toLowerCase()===username)) continue;
    profiles.push({username,name:lines.filter(line=>line.toLowerCase()!==username).join(' '),url:`https://www.instagram.com/${username}/`});
  }
  return {ready:profiles.length>0||empty,profiles};
}

async function searchState(owned,query) {
  let opened=false;
  for(let poll=0;poll<40;poll++) {
    const raw=await owned.evaluate(`(${observeInstagramVisibleSearch.toString()})(${JSON.stringify(query)})`);
    const state=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    if(state?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_'+state.stop.toUpperCase());
    if(state?.ready) return state;
    if(query===null&&state?.entry&&!opened) { await owned.click(state.entry); opened=true; continue; }
    if(poll<39) await owned.sleep(0.25);
  }
  throw new Error('ONECREW_INSTAGRAM_SEARCH_TIMEOUT');
}

export async function observeInstagramKeywordPosts(query) {
  for(let poll=0;poll<32;poll++) {
    if(/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
    if(/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
    if(location.origin!=='https://www.instagram.com'||location.pathname!=='/explore/search/keyword/'||new URL(location.href).searchParams.get('q')!==query) return {stop:'source'};
    const main=document.querySelector('main'),body=main?.innerText||'';
    if(/Try again later|请求太频繁|稍后重试|操作过于频繁/i.test(body)) return {stop:'risk'};
    const urls=[...new Set([...main?.querySelectorAll('a[href]')||[]].filter(node=>node.getClientRects().length).map(node=>node.href).filter(href=>/^https:\/\/www\.instagram\.com\/(?:p|reel)\/[A-Za-z0-9_-]{1,32}\/$/.test(href)))].slice(0,20);
    if(urls.length||/No results found|No results|未找到结果|没有找到结果/i.test(body)) return {urls};
    if(poll<31) await new Promise(resolve=>setTimeout(resolve,250));
  }
  return {stop:'timeout'};
}

export async function observeInstagramSearchPostAuthor(source) {
  for(let poll=0;poll<32;poll++) {
    if(/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
    if(/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
    if(location.origin+location.pathname!==source) return {stop:'source'};
    const main=document.querySelector('main'),body=main?.innerText||'';
    if(/Try again later|请求太频繁|稍后重试|操作过于频繁/i.test(body)) return {stop:'risk'};
    if(/Sorry, this page isn't available|页面无法访问|此内容不可用/i.test(body)) return {unavailable:true};
    const marker=[...main?.querySelectorAll('time')||[]].find(node=>node.getClientRects().length&&!node.closest('a[href]'));
    // 正文的作者行与时间相邻；正文中的提及、评论作者和推荐账号不作为发布者。
    for(let node=marker?.parentElement,depth=0;node&&node!==main&&depth<6;node=node.parentElement,depth++) {
      const parts=[...node.children];
      if(parts.length!==2||!parts[0].contains(marker)||parts[1].contains(marker)||!parts[1].innerText.trim()) continue;
      const authors=[...parts[0].querySelectorAll('a[href]')].flatMap(anchor=>{
        const url=new URL(anchor.href),match=/^\/([a-z0-9_.]{1,30})\/$/i.exec(url.pathname);
        return anchor.getClientRects().length&&url.origin===location.origin&&!url.search&&!url.hash&&match&&anchor.innerText.trim().toLowerCase()===match[1].toLowerCase()?[match[1].toLowerCase()]:[];
      });
      const unique=[...new Set(authors)];
      if(unique.length>1) return {stop:'identity'};
      if(unique.length===1) return {username:unique[0],name:unique[0],url:`https://www.instagram.com/${unique[0]}/`};
    }
    if(poll<31) await new Promise(resolve=>setTimeout(resolve,250));
  }
  return {stop:'timeout'};
}

export async function readInstagramKeywordSearch(owned,query,limit) {
  const unwrap=raw=>raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
  await owned.goto('https://www.instagram.com/explore/search/keyword/?q='+encodeURIComponent(query),{settleMs:0});
  const state=unwrap(await owned.evaluate(`(${observeInstagramKeywordPosts.toString()})(${JSON.stringify(query)})`));
  if(state?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_'+state.stop.toUpperCase());
  if(!Array.isArray(state?.urls)) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  const rows=[];
  for(const source of state.urls.slice(0,5)) {
    await owned.goto(source,{settleMs:0});
    const author=unwrap(await owned.evaluate(`(${observeInstagramSearchPostAuthor.toString()})(${JSON.stringify(source)})`));
    if(author?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_'+author.stop.toUpperCase());
    if(!author?.unavailable) rows.push(author);
    if(new Set(rows.map(row=>row.username)).size>=limit) break;
  }
  return projectInstagramVisibleSearch(rows,limit);
}

export async function readInstagramSearch(page,args,ownPage) {
  const query=args.query,limit=args.limit??20;
  if(typeof query!=='string'||!query.trim()||query.length>1000||query.includes('\0')||!Number.isInteger(limit)||limit<1||limit>20) throw new Error('ONECREW_INSTAGRAM_SEARCH_INPUT');
  return ownPage(page,async owned=>{
    // 中文词组在账号建议区可能被拼音模糊匹配成熟人；使用正式内容结果再核实发布者。
    if(/\p{Script=Han}/u.test(query)) return readInstagramKeywordSearch(owned,query,limit);
    await owned.goto('https://www.instagram.com/explore/',{settleMs:0});
    await searchState(owned,null);
    const input='input[aria-label="Search input"],input[aria-label="搜索输入"]';
    await owned.fillText(input,query);
    const state=await searchState(owned,query);
    return projectInstagramVisibleSearch(state.profiles,limit);
  });
}
