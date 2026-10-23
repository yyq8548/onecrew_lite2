import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readBilibiliVideo } from './video.mjs';

cli({
  site: 'onecrew-bilibili-video', name: 'read', access: 'read',
  description: 'OneCrew 1.0.0: source-bound public Bilibili video-page evidence',
  domain: 'www.bilibili.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'bvid', required: true, positional: true, help: 'Observed exact BVID' },
    { name: 'mid', required: true, positional: true, help: 'Observed native author MID' },
  ],
  columns: ['field', 'value'],
  func: async (page, args) => {
    try { return await readBilibiliVideo(page, args.bvid, args.mid); }
    catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_BILIBILI_VIDEO_INPUT')) throw new ArgumentError('Invalid OneCrew Bilibili video input');
      if (message.includes('ONECREW_BILIBILI_VIDEO_AUTH')) throw new AuthRequiredError('www.bilibili.com', 'OneCrew Bilibili video requires sign-in');
      if (message.includes('ONECREW_BILIBILI_VIDEO_RISK')) throw new CommandExecutionError('OneCrew Bilibili verification or rate limit requires human review');
      throw new CommandExecutionError('OneCrew Bilibili video page could not be verified');
    }
  },
});
