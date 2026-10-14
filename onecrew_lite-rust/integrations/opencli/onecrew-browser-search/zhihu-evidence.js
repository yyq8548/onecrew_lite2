import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readEvidence} from './zhihu-evidence.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';

for (const name of ['profile', 'answer', 'article', 'search', 'answer-comments', 'article-comments']) {
  cli({site: 'onecrew-zhihu-reader', name, access: 'read', strategy: Strategy.COOKIE,
    domain: 'www.zhihu.com', navigateBefore: false,
    description: `OneCrew bounded original Zhihu ${name} evidence`,
    args: name === 'search'
      ? [{name: 'query', required: true, positional: true}, {name: 'limit', type: 'int', default: 10}]
      : [{name: 'id', required: true, positional: true}],
    columns: name === 'profile' ? ['author_id', 'url_token', 'follower_count', 'author_url']
      : ['author_id', 'answer_id', 'source_url'],
    func: async (page, args) => {
      try { return await readEvidence(page, args, name, withOwnedSearchPage); }
      catch (error) {
        if (error?.message === 'ONECREW_ZHIHU_INPUT') throw new ArgumentError('Invalid native Zhihu target');
        if (error?.message === 'ONECREW_ZHIHU_AUTH') throw new AuthRequiredError('www.zhihu.com', 'Zhihu sign-in required');
        if (error?.message === 'ONECREW_ZHIHU_RISK') {
          const diagnostic = [403, 429].includes(error.httpStatus) ? ` (HTTP ${error.httpStatus})` : '';
          throw new CommandExecutionError(`Zhihu verification or rate limit${diagnostic}; stop this source`);
        }
        throw error;
      }
    },
  });
}
