export function projectInstagramVisibleSearch(rows, limit) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 20 || !Array.isArray(rows) || rows.length > 100) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
  const seen = new Set();
  return rows.flatMap(row => {
    if (typeof row?.username !== 'string' || !/^[a-z0-9_.]{1,30}$/.test(row.username)
      || row.url !== `https://www.instagram.com/${row.username}/` || typeof row.name !== 'string'
      || row.name.includes('\0') || row.author_id !== undefined) throw new Error('ONECREW_INSTAGRAM_SEARCH_INVALID');
    if (seen.has(row.username)) return [];
    seen.add(row.username);
    return [{provider:'onecrew-instagram-visible-v1',execution_mode:'computer_use',username:row.username,
      name:Array.from(row.name || row.username).slice(0,200).join(''),url:row.url}];
  }).slice(0,limit);
}

export function observeInstagramVisibleSearch(query) {
  if (location.origin !== 'https://www.instagram.com') return {stop:'read_redirect'};
  if (/^\/accounts\/login/.test(location.pathname)) return {stop:'auth'};
  if (/^\/(challenge|checkpoint)\//.test(location.pathname)) return {stop:'risk'};
  const visible=node=>node.getClientRects().length>0;
  const input=[...document.querySelectorAll('input[aria-label="Search input"],input[aria-label="搜索输入"]')].filter(visible);
  if (input.length!==1) return {ready:false};
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
  for(let poll=0;poll<40;poll++) {
    const raw=await owned.evaluate(`(${observeInstagramVisibleSearch.toString()})(${JSON.stringify(query)})`);
    const state=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
    if(state?.stop) throw new Error('ONECREW_INSTAGRAM_SEARCH_'+state.stop.toUpperCase());
    if(state?.ready) return state;
    if(poll<39) await owned.sleep(0.25);
  }
  throw new Error('ONECREW_INSTAGRAM_SEARCH_TIMEOUT');
}

export async function readInstagramSearch(page,args,ownPage) {
  const query=args.query,limit=args.limit??20;
  if(typeof query!=='string'||!query.trim()||query.length>1000||query.includes('\0')||!Number.isInteger(limit)||limit<1||limit>20) throw new Error('ONECREW_INSTAGRAM_SEARCH_INPUT');
  return ownPage(page,async owned=>{
    await owned.goto('https://www.instagram.com/explore/');
    await owned.snapshot({source:'dom',interactive:true,compact:true});
    await searchState(owned,null);
    const input='input[aria-label="Search input"],input[aria-label="搜索输入"]';
    await owned.click(input);
    await owned.fillText(input,query);
    const state=await searchState(owned,query);
    await owned.snapshot({source:'dom',interactive:true,compact:true});
    return projectInstagramVisibleSearch(state.profiles,limit);
  });
}
