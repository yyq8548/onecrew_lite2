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
    catch {
      // Raw provider failures can contain page state or transient network URLs.
      throw new CommandExecutionError('OneCrew video evidence could not be verified');
    }
  },
});
