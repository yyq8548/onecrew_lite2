import { cli, Strategy } from '@jackwener/opencli/registry';
import { AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readSiteLogin } from './probe.mjs';

const domains = { bilibili: 'www.bilibili.com', tiktok: 'www.tiktok.com', instagram: 'www.instagram.com', facebook: 'www.facebook.com', douyin: 'www.douyin.com', linkedin: 'www.linkedin.com', weibo: 'weibo.com', tieba: 'tieba.baidu.com' };
domains.twitter = 'x.com';
domains.youtube = 'www.youtube.com';
domains.zhihu = 'www.zhihu.com';
domains.xiaohongshu = 'www.xiaohongshu.com';
for (const provider of Object.keys(domains)) {
  cli({
    site: `onecrew-${provider}-auth`, name: 'whoami', access: 'read',
    description: `OneCrew: verify current ${provider} account without returning identity`,
    domain: domains[provider], browser: true, strategy: Strategy.COOKIE,
    navigateBefore: false, args: [], columns: ['verified'],
    func: async page => {
      const status = await readSiteLogin(page, provider);
      if (status === 'not_logged_in') throw new AuthRequiredError(domains[provider], 'Current-account sign-in is required');
      if (status === 'rate_limited') throw new CommandExecutionError('HTTP 429 authentication check rate limited');
      if (status !== 'logged_in') throw new CommandExecutionError('Current-account identity could not be verified');
      return [{ verified: true }];
    },
  });
}
