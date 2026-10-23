import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readTikTokProfile } from './profile.mjs';

for (const operation of ['read','identity']) cli({
  site: 'onecrew-tiktok-profile', name: operation, access: 'read',
  description: 'OneCrew 1.0.0: public TikTok profile bound to an observed author ID',
  domain: 'www.tiktok.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'username', required: true, positional: true, help: 'Observed author username' },
    ...(operation==='read'?[{ name: 'author-id', required: true, help: 'Expected native author ID from search' }]:[]),
  ],
  columns: ['id', 'username', 'follower_count', 'profile_url'],
  func: async (page, args) => {
    try { return await readTikTokProfile(page, args.username, args['author-id'], operation==='identity'); }
    catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_TIKTOK_PROFILE_INPUT')) throw new ArgumentError('Invalid OneCrew TikTok profile input');
      if (message.includes('ONECREW_TIKTOK_PROFILE_AUTH')) throw new AuthRequiredError('www.tiktok.com', 'OneCrew TikTok profile requires sign-in');
      if (message.includes('ONECREW_TIKTOK_PROFILE_RISK')) throw new CommandExecutionError('OneCrew TikTok CAPTCHA or rate limit requires human review');
      throw new CommandExecutionError('OneCrew TikTok public profile could not be verified');
    }
  },
});
