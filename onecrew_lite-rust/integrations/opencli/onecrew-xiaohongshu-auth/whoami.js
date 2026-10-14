import { cli, Strategy } from '@jackwener/opencli/registry';
import { AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readXhsLogin } from './session.mjs';

cli({
  site: 'onecrew-xiaohongshu-auth', name: 'whoami', access: 'read',
  description: 'OneCrew 1.0.0: verify Xiaohongshu search-site login without returning identity',
  domain: 'www.xiaohongshu.com', browser: true, strategy: Strategy.COOKIE,
  navigateBefore: false, args: [], columns: ['verified'],
  func: async (page) => {
    const status = await readXhsLogin(page);
    if (status === 'not_logged_in') throw new AuthRequiredError('www.xiaohongshu.com', 'Search-site sign-in is required');
    if (status !== 'logged_in') throw new CommandExecutionError('Search-site identity could not be verified');
    return [{ verified: true }];
  },
});
