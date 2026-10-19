import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readEvidence} from './evidence.mjs';

for (const name of ['profile', 'post', 'comments', 'search']) {
  cli({site: 'onecrew-bluesky', name, access: 'read', browser: false,
    strategy: Strategy.PUBLIC, domain: 'public.api.bsky.app',
    description: `OneCrew bounded public native Bluesky ${name}`,
    args: name === 'search'
      ? [{name: 'query', positional: true, required: true}, {name: 'limit', type: 'int', default: 10}]
      : [{name: name === 'profile' ? 'did' : 'uri', positional: true, required: true}],
    columns: name === 'profile' ? ['did', 'handle', 'follower_count', 'profile_url']
      : ['author_id', 'post_uri', 'source_url', 'text'],
    func: async (_page, args) => {
      try { return await readEvidence(name, args); }
      catch (error) {
        if (error?.message === 'ONECREW_BLUESKY_INPUT') throw new ArgumentError('Invalid native Bluesky target');
        if (error?.message === 'ONECREW_BLUESKY_AUTH') throw new AuthRequiredError('public.api.bsky.app', 'Public read requires authentication');
        if (error?.message === 'ONECREW_BLUESKY_RISK') throw new CommandExecutionError('Bluesky verification or rate limit; stop this source');
        throw error;
      }
    },
  });
}
