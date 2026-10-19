import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readBilibiliProfile } from './profile.mjs';

cli({
  site: 'onecrew-bilibili-profile', name: 'read', access: 'read',
  description: 'OneCrew 1.0.0: exact public follower evidence on an observed Bilibili author profile',
  domain: 'space.bilibili.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [{ name: 'mid', required: true, positional: true, help: 'Observed native author MID' }],
  columns: ['mid', 'name', 'follower_count', 'follower_display', 'profile_url'],
  func: async (page, args) => {
    try { return await readBilibiliProfile(page, args.mid); }
    catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_BILIBILI_PROFILE_INPUT')) throw new ArgumentError('Invalid OneCrew Bilibili profile input');
      if (message.includes('ONECREW_BILIBILI_PROFILE_AUTH')) throw new AuthRequiredError('space.bilibili.com', 'OneCrew Bilibili profile requires sign-in');
      if (message.includes('ONECREW_BILIBILI_PROFILE_RISK')) throw new CommandExecutionError('OneCrew Bilibili CAPTCHA or rate limit requires human review');
      throw new CommandExecutionError('OneCrew Bilibili public profile could not be verified');
    }
  },
});
