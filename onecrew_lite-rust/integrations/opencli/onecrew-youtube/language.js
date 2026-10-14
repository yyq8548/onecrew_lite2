import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readYoutubeLanguage, readYoutubeRecent } from './language.mjs';
import { restoreOriginalLanguage } from '../onecrew-browser-search/original-language.mjs';

cli({
  site: 'onecrew-youtube', name: 'language', access: 'read', domain: 'www.youtube.com',
  description: 'Bounded native YouTube description/comments or three recent video locators',
  strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'channel', required: true, positional: true },
    { name: 'video', required: true, positional: true, help: 'Exact video ID or recent' },
  ],
  columns: ['source_url'],
  func: async (page, args) => {
    if (!/^UC[A-Za-z0-9_-]{22}$/.test(args.channel)
        || (args.video !== 'recent' && !/^[A-Za-z0-9_-]{11}$/.test(args.video))) throw new ArgumentError('Invalid native YouTube identity');
    try {
      return args.video === 'recent'
        ? await readYoutubeRecent(page, args.channel, restoreOriginalLanguage)
        : await readYoutubeLanguage(page, args.video, args.channel, restoreOriginalLanguage);
    } catch { throw new CommandExecutionError('OneCrew original YouTube language evidence unavailable'); }
  },
});
