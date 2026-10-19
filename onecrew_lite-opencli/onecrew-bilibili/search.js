import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { searchBilibili } from './search.mjs';

cli({
  site: 'onecrew-bilibili', name: 'search', access: 'read',
  description: 'OneCrew 1.0.0: bounded Bilibili search cards with video and author sources',
  domain: 'search.bilibili.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'query', required: true, positional: true, help: 'Search query' },
    { name: 'limit', type: 'int', default: 5, help: 'Maximum videos, 1 to 50' },
  ],
  columns: ['bvid', 'title', 'author', 'author_id', 'url', 'author_url'],
  func: async (page, args) => {
    try { return await searchBilibili(page, args.query, args.limit); }
    catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_BILIBILI_INPUT')) throw new ArgumentError('Invalid OneCrew Bilibili search input');
      if (message.includes('ONECREW_BILIBILI_AUTH')) throw new AuthRequiredError('bilibili.com', 'OneCrew Bilibili requires sign-in');
      if (message.includes('ONECREW_BILIBILI_RISK')) throw new CommandExecutionError('OneCrew Bilibili CAPTCHA or rate limit requires human review');
      // Never publish raw page content, transient URLs or an unverified empty result.
      throw new CommandExecutionError('OneCrew Bilibili search could not be verified');
    }
  },
});
