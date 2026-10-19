import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { searchTikTok } from './search.mjs';

cli({
  site: 'onecrew-tiktok', name: 'search', access: 'read',
  description: 'OneCrew 1.0.0: bounded TikTok video search with author identity and source',
  domain: 'www.tiktok.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'query', required: true, positional: true, help: 'Search query' },
    { name: 'limit', type: 'int', default: 5, help: 'Maximum videos, 1 to 50' },
  ],
  columns: ['id', 'author_id', 'author', 'desc', 'url'],
  func: async (page, args) => {
    try { return await searchTikTok(page, args.query, args.limit); }
    catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_TIKTOK_INPUT')) throw new ArgumentError('Invalid OneCrew TikTok search input');
      if (message.includes('ONECREW_TIKTOK_AUTH')) throw new AuthRequiredError('www.tiktok.com', 'OneCrew TikTok requires sign-in');
      if (message.includes('ONECREW_TIKTOK_RISK')) throw new CommandExecutionError('OneCrew TikTok CAPTCHA or rate limit requires human review');
      // Never expose raw page state, endpoint response bodies, or transient URLs.
      throw new CommandExecutionError('OneCrew TikTok search could not be verified');
    }
  },
});
