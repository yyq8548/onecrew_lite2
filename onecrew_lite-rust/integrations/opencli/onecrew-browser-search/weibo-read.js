import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readWeiboEvidence } from './weibo-read.mjs';
import { withOwnedSearchPage } from './owned-page.mjs';

for (const name of ['profile', 'recent']) {
  cli({site: 'onecrew-weibo-reader', name, access: 'read', strategy: Strategy.COOKIE,
    domain: 'weibo.com', navigateBefore: false,
    description: `OneCrew: bounded native Weibo ${name} evidence`,
    args: [{name: 'id', required: true, positional: true, help: 'Observed native UID'}],
    columns: name === 'profile' ? ['id', 'name', 'follower_count', 'profile_url'] : ['author_id', 'source_urls'],
    func: async (page, args) => {
      try { return await readWeiboEvidence(page, args, name, withOwnedSearchPage); }
      catch (error) {
        if (error?.message === 'ONECREW_WEIBO_INPUT') throw new ArgumentError('Invalid native Weibo UID');
        if (error?.message === 'ONECREW_WEIBO_AUTH') throw new AuthRequiredError('weibo.com', 'Weibo requires sign-in');
        if (error?.message === 'ONECREW_WEIBO_RISK') throw new CommandExecutionError('Weibo verification or rate limit requires human review');
        throw error;
      }
    },
  });
}
