import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readVideoEvidence } from './evidence.mjs';

cli({
  site: 'onecrew-youtube', name: 'video-evidence', access: 'read',
  description: 'OneCrew 1.0.0: bounded original YouTube description and source-bound links',
  domain: 'www.youtube.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [{ name: 'video', required: true, positional: true, help: 'Exact 11-character video ID' }],
  columns: ['videoId', 'channelId', 'title', 'subscribers'],
  func: async (page, args) => {
    if (typeof args.video !== 'string' || !/^[A-Za-z0-9_-]{11}$/.test(args.video)) {
      throw new ArgumentError('Expected an exact YouTube video ID');
    }
    try { return await readVideoEvidence(page, args.video); }
    catch (error) {
      // 原始错误可能含页面状态，仅返回已知诊断代码。
      const markers = ['AUTH_REQUIRED', 'ONECREW_YOUTUBE_VIDEO_UNAVAILABLE',
        'ONECREW_YOUTUBE_LAYOUT_UNSUPPORTED', 'ONECREW_YOUTUBE_IDENTITY_INVALID',
        'ONECREW_YOUTUBE_DESCRIPTION_UNAVAILABLE'];
      const marker = markers.find(code => typeof error?.message === 'string' && error.message.includes(code));
      throw new CommandExecutionError(marker ?? 'OneCrew video evidence could not be verified');
    }
  },
});
