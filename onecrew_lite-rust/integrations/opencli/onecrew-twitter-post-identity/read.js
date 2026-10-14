import { readFile } from 'node:fs/promises';
import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readTwitterPostAuthor } from './post.mjs';

const packageRoot = new URL('../../', import.meta.resolve('@jackwener/opencli/registry'));
// OpenCLI 1.8.8 twitter/thread fallback, resolved against current public metadata.
const operation = { queryId: 'nBS-WpgA6ZG0CyNHD517JQ', features: {
  responsive_web_graphql_exclude_directive_enabled: true, verified_phone_label_enabled: false,
  creator_subscriptions_tweet_preview_api_enabled: true,
  responsive_web_graphql_timeline_navigation_enabled: true,
  responsive_web_graphql_skip_user_profile_image_extensions_enabled: false,
  longform_notetweets_consumption_enabled: true, longform_notetweets_rich_text_read_enabled: true,
  longform_notetweets_inline_media_enabled: true, freedom_of_speech_not_reach_fetch_enabled: true,
}, fieldToggles: { withArticleRichContentState: true, withArticlePlainText: false } };

cli({
  site: 'onecrew-twitter-post-identity', name: 'read', access: 'read', browser: true,
  description: 'OneCrew 1.0.0: bind one observed X post to its native author; no replies or qualification',
  domain: 'x.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'handle', type: 'string', required: true, positional: true, help: 'Observed lowercase X author handle' },
    { name: 'post-id', type: 'string', required: true, positional: true, help: 'Observed numeric post ID' },
  ],
  columns: ['post_id', 'account_id', 'handle', 'source_url'],
  func: async (page, args) => {
    try {
      const metadata = JSON.parse(await readFile(new URL('package.json', packageRoot), 'utf8'));
      if (metadata.version !== '1.8.8') throw new Error('ONECREW_X_POST_IDENTITY_VERSION');
      const { resolveTwitterOperationMetadata } = await import(new URL('clis/twitter/shared.js', packageRoot).href);
      const { TWITTER_BEARER_TOKEN } = await import(new URL('clis/twitter/utils.js', packageRoot).href);
      return await readTwitterPostAuthor(page, args.handle, args['post-id'],
        page => resolveTwitterOperationMetadata(page, 'TweetDetail', operation), TWITTER_BEARER_TOKEN);
    } catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_X_POST_IDENTITY_INPUT')) throw new ArgumentError('Invalid OneCrew X post identity input');
      if (message.includes('ONECREW_X_POST_IDENTITY_AUTH')) throw new AuthRequiredError('x.com', 'OneCrew X post identity requires sign-in');
      if (message.includes('ONECREW_X_POST_IDENTITY_RISK')) throw new CommandExecutionError('OneCrew X rate limit requires human review');
      throw new CommandExecutionError('OneCrew X post author could not be verified');
    }
  },
});
