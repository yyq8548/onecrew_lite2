import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { withOwnedSearchPage } from '../onecrew-browser-search/owned-page.mjs';
import { fetchZhihuPage, zhihuSearchRows, zhihuProfile, zhihuContent } from './read.mjs';

for (const name of ['search', 'profile', 'content']) cli({
  site: 'onecrew-zhihu', name, access: 'read', domain: 'www.zhihu.com', strategy: Strategy.COOKIE, navigateBefore: false,
  description: `OneCrew Zhihu ${name} with native author evidence`,
  args: name === 'search' ? [{ name: 'query', required: true, positional: true }, { name: 'limit', type: 'int', default: 20 }]
    : [{ name: 'source', required: true, positional: true }, { name: 'author', required: true }],
  columns: name === 'search' ? ['author', 'title', 'url', 'author_id', 'author_url'] : ['author_id', 'author_label', 'author_url'],
  func: (page, args) => withOwnedSearchPage(page, async owned => {
    if (name === 'search' ? typeof args.query !== 'string' || !args.query.trim() || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 20
      : !/^[a-f0-9]{32}$/.test(args.author)) throw new ArgumentError('Invalid Zhihu read input');
    let endpoint, destination = 'https://www.zhihu.com/', token;
    if (name === 'search') endpoint = `https://www.zhihu.com/api/v4/search_v3?q=${encodeURIComponent(args.query)}&t=general&offset=0&limit=20`;
    else if (name === 'profile') {
      token = /^https:\/\/www\.zhihu\.com\/people\/([A-Za-z0-9_-]{1,128})$/.exec(args.source)?.[1];
      if (!token) throw new ArgumentError('Invalid Zhihu profile URL');
      endpoint = `https://www.zhihu.com/api/v4/members/${token}?include=follower_count,headline,description`;
    } else {
      const answer = /^https:\/\/www\.zhihu\.com\/question\/([1-9][0-9]{0,19})\/answer\/([1-9][0-9]{0,19})$/.exec(args.source);
      const article = /^https:\/\/zhuanlan\.zhihu\.com\/p\/([1-9][0-9]{0,19})$/.exec(args.source);
      if (!answer && !article) throw new ArgumentError('Invalid Zhihu content URL');
      destination = answer ? args.source : destination;
      endpoint = answer ? `https://www.zhihu.com/api/v4/answers/${answer[2]}?include=content,author,question` : `https://www.zhihu.com/api/v4/articles/${article[1]}?include=content,author,title`;
    }
    await owned.goto(destination);
    const raw = await owned.evaluate(`(${fetchZhihuPage.toString()})(${JSON.stringify(endpoint)})`);
    const result = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
    if (result?.failure) {
      const { status, code, message } = result.failure;
      const detail = `ONECREW_ZHIHU_HTTP stage=${name} HTTP ${status} code=${code ?? 'unknown'} message=${message}`;
      if (status === 401) throw new AuthRequiredError('www.zhihu.com', detail);
      throw new CommandExecutionError(detail);
    }
    if (!result?.data) throw new CommandExecutionError(`ONECREW_ZHIHU_INVALID_RESPONSE stage=${name}`);
    return name === 'search' ? zhihuSearchRows(result.data, args.limit) : [name === 'profile' ? zhihuProfile(result.data, args.author, token) : zhihuContent(result.data, args.source, args.author)];
  }),
});
