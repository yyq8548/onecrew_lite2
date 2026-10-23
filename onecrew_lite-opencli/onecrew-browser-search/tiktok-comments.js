import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readTikTokComments} from './tiktok-comments.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';

cli({site: 'onecrew-comments-tiktok', name: 'comments', access: 'read', browser: true,
  domain: 'www.tiktok.com', strategy: Strategy.COOKIE, navigateBefore: false,
  description: 'Read original video description and bounded public comments',
  args: [{name: 'id', positional: true, required: true}, {name: 'limit', type: 'int', default: 20}],
  columns: ['type', 'id', 'native_author_id', 'author', 'source_url', 'text'],
  func: async (page, args) => {
    try { return await readTikTokComments(page, args, withOwnedSearchPage); }
    catch (error) {
      if (error?.message === 'ONECREW_COMMENTS_INPUT') throw new ArgumentError('Invalid TikTok comment source');
      if (error?.message === 'ONECREW_COMMENTS_AUTH') throw new AuthRequiredError('www.tiktok.com', 'Comments require sign-in');
      if (error?.message === 'ONECREW_COMMENTS_RISK') throw new CommandExecutionError('TikTok verification or rate limit requires human review');
      throw error;
    }
  },
});
