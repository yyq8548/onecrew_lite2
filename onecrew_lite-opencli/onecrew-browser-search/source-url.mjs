// 此函数会连同观察器注入页面；只合并已知官网及无身份意义的格式差异。
export function equivalentSourceUrl(actual, expected) {
  const key = raw => {
    if (typeof raw !== 'string' || raw.length > 4096) return null;
    const input = raw.startsWith('www.') ? `https://${raw}` : raw;
    if (!URL.canParse(input)) return null;
    const url = new URL(input);
    if (url.protocol !== 'https:' || url.username || url.password || url.port) return null;
    const domains = ['linkedin.com','facebook.com','instagram.com','youtube.com','twitter.com','x.com','tiktok.com','bilibili.com','douyin.com','weibo.com','zhihu.com','pixiv.net','bsky.app'];
    let host = url.hostname;
    const apex = host.startsWith('www.') ? host.slice(4) : host;
    const known = domains.includes(apex) || ['space.bilibili.com','news.ycombinator.com'].includes(host);
    if (domains.includes(apex)) host = apex;
    if (host === 'twitter.com') host = 'x.com';
    let path = url.pathname;
    if (host === 'instagram.com') {
      const parts = path.split('/');
      if (parts.length > 1 && !['p','reel','reels','tv','explore','accounts'].includes(parts[1].toLowerCase())) parts[1] = parts[1].toLowerCase();
      path = parts.join('/');
    }
    if (known && path.length > 1 && path.endsWith('/') && !path.endsWith('//')) path = path.slice(0,-1);
    for (const name of [...url.searchParams.keys()]) {
      if (/^utm_/i.test(name) || ['fbclid','gclid','igshid'].includes(name)
          || host === 'instagram.com' && name === 'igsh'
          || host === 'linkedin.com' && (['trk','lipi'].includes(name) || name === 'isSelfProfile' && url.searchParams.getAll(name).length === 1 && url.searchParams.get(name) === 'false')) url.searchParams.delete(name);
    }
    url.searchParams.sort();
    return `${host}${path}?${url.searchParams.toString()}${url.hash}`;
  };
  const expectedKey = key(expected);
  return expectedKey !== null && key(actual) === expectedKey;
}

export function sourceUrlScript(expression) {
  return `(()=>{const equivalentSourceUrl=(${equivalentSourceUrl.toString()});return (${expression});})()`;
}
