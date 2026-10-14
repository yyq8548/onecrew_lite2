import { readFile } from 'node:fs/promises';
import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readTwitterPostContent, postContentDiagnostic } from './content.mjs';

const packageRoot = new URL('../../', import.meta.resolve('@jackwener/opencli/registry'));
// Same pinned TweetDetail operation as the independently versioned identity reader.
const operation = { queryId: 'nBS-WpgA6ZG0CyNHD517JQ', features: {
  responsive_web_graphql_exclude_directive_enabled: true, verified_phone_label_enabled: false,
  creator_subscriptions_tweet_preview_api_enabled: true,
  responsive_web_graphql_timeline_navigation_enabled: true,
  responsive_web_graphql_skip_user_profile_image_extensions_enabled: false,
  longform_notetweets_consumption_enabled: true, longform_notetweets_rich_text_read_enabled: true,
  longform_notetweets_inline_media_enabled: true, freedom_of_speech_not_reach_fetch_enabled: true,
}, fieldToggles: { withArticleRichContentState: true, withArticlePlainText: false } };

cli({
  site: 'onecrew-twitter-post-content', name: 'read', access: 'read', browser: true,
  description: 'OneCrew 1.0.0: bounded focal post text and URL entities bound to an expected native author; not qualification',
  domain: 'x.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [
    { name: 'handle', type: 'string', required: true, positional: true, help: 'Observed lowercase author handle' },
    { name: 'post-id', type: 'string', required: true, positional: true, help: 'Observed numeric post ID' },
    { name: 'account-id', type: 'string', required: true, positional: true, help: 'Previously verified native author ID' },
    { name: 'language-sample', type: 'boolean', default: false, help: 'Read original focal text and first five direct replies' },
  ],
  columns: ['text', 'text_source', 'has_media'],
  func: async (page, args) => {
    try {
      const metadata = JSON.parse(await readFile(new URL('package.json', packageRoot), 'utf8'));
      if (metadata.version !== '1.8.8') throw new Error('ONECREW_X_POST_CONTENT_VERSION');
      const { resolveTwitterOperationMetadata } = await import(new URL('clis/twitter/shared.js', packageRoot).href);
      const { TWITTER_BEARER_TOKEN } = await import(new URL('clis/twitter/utils.js', packageRoot).href);
      return await readTwitterPostContent(page, args.handle, args['post-id'], args['account-id'],
        page => resolveTwitterOperationMetadata(page, 'TweetDetail', operation), TWITTER_BEARER_TOKEN, args['language-sample'] === true);
    } catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_X_POST_IDENTITY_INPUT')) throw new ArgumentError('Invalid OneCrew X post content input');
      if (message.includes('ONECREW_X_POST_IDENTITY_AUTH')) throw new AuthRequiredError('x.com', 'OneCrew X post content requires sign-in');
      if (message.includes('ONECREW_X_POST_IDENTITY_RISK')) throw new CommandExecutionError('OneCrew X rate limit requires human review');
      throw new CommandExecutionError('OneCrew X post content could not be verified: ' + postContentDiagnostic(message));
    }
  },
});
