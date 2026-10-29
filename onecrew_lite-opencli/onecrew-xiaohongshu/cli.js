import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CliError, CommandExecutionError } from '@jackwener/opencli/errors';
import { withOwnedSearchPage } from '../onecrew-browser-search/owned-page.mjs';
import { observeXiaohongshuPage } from './page.mjs';
import { xiaohongshuSource, xiaohongshuSearchRows, xiaohongshuProfile, xiaohongshuContent } from './read.mjs';

for (const name of ['search', 'profile', 'content']) cli({
  site: 'onecrew-xiaohongshu', name, access: 'read', domain: 'www.xiaohongshu.com', strategy: Strategy.COOKIE, navigateBefore: false,
  description: `OneCrew Xiaohongshu ${name} with native author evidence`,
  args: name === 'search' ? [{ name: 'query', required: true, positional: true }, { name: 'limit', type: 'int', default: 20 }]
    : [{ name: 'source', required: true, positional: true }, { name: 'author', required: true }],
  columns: name === 'search' ? ['author', 'title', 'url', 'author_id', 'author_url'] : ['author_id', 'author_label'],
  func: (page, args) => withOwnedSearchPage(page, async owned => {
    if (name === 'search' ? typeof args.query !== 'string' || !args.query.trim() || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 20
      : !/^[a-f0-9]{24}$/.test(args.author) || name === 'profile' && xiaohongshuSource(args.source, name) !== args.author) throw new ArgumentError('Invalid Xiaohongshu read input');
    if (name === 'content') xiaohongshuSource(args.source, name);
    // 新建搜索页默认即为综合搜索，不重复点击默认筛选项。
    const destination = name === 'search' ? `https://www.xiaohongshu.com/search_result?keyword=${encodeURIComponent(args.query)}&source=web_search_result_notes` : args.source;
    await owned.goto(destination, { settleMs: 0 });
    const result = await owned.evaluate(`(${observeXiaohongshuPage.toString()})(${JSON.stringify(name)},${JSON.stringify(destination)},${JSON.stringify(args.limit || 1)})`)
      .then(raw => raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw, error => {
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes('ONECREW_XIAOHONGSHU_AUTH')) throw new AuthRequiredError('www.xiaohongshu.com', message);
        if (message.includes('ONECREW_XIAOHONGSHU_RISK')) throw new CliError('SECURITY_BLOCK', message);
        throw new CommandExecutionError(message);
      });
    return name === 'search' ? xiaohongshuSearchRows(result, args.limit)
      : [name === 'profile' ? xiaohongshuProfile(result, args.author, args.source) : xiaohongshuContent(result, args.author, args.source)];
  }),
});
