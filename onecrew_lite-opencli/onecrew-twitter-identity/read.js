import { readFile } from 'node:fs/promises';
import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readTwitterIdentity } from './profile.mjs';

// Resolve the installed package rather than assuming a global npm directory.
const packageRoot = new URL('../../', import.meta.resolve('@jackwener/opencli/registry'));
const operation = { queryId: 'IGgvgiOx4QZndDHuD3x9TQ', features: {
  hidden_profile_subscriptions_enabled: true, rweb_tipjar_consumption_enabled: true,
  responsive_web_graphql_exclude_directive_enabled: true, verified_phone_label_enabled: false,
  subscriptions_verification_info_is_identity_verified_enabled: true,
  subscriptions_verification_info_verified_since_enabled: true, highlights_tweets_tab_ui_enabled: true,
  responsive_web_twitter_article_notes_tab_enabled: true, subscriptions_feature_can_gift_premium: true,
  creator_subscriptions_tweet_preview_api_enabled: true,
  responsive_web_graphql_skip_user_profile_image_extensions_enabled: false,
  responsive_web_graphql_timeline_navigation_enabled: true,
} };

cli({
  site: 'onecrew-twitter-identity', name: 'read', access: 'read', browser: true,
  description: 'OneCrew 1.0.0: bounded native X account identity for an observed handle; not KOL qualification',
  domain: 'x.com', strategy: Strategy.COOKIE, navigateBefore: false,
  args: [{ name: 'handle', type: 'string', required: true, positional: true, help: 'Observed lowercase X handle' }],
  columns: ['account_id', 'handle', 'name', 'follower_count', 'profile_url'],
  func: async (page, args) => {
    try {
      const metadata = JSON.parse(await readFile(new URL('package.json', packageRoot), 'utf8'));
      if (metadata.version !== '1.8.8') throw new Error('ONECREW_X_IDENTITY_VERSION');
      const { resolveTwitterOperationMetadata } = await import(new URL('clis/twitter/shared.js', packageRoot).href);
      const { TWITTER_BEARER_TOKEN } = await import(new URL('clis/twitter/utils.js', packageRoot).href);
      return await readTwitterIdentity(page, args.handle,
        page => resolveTwitterOperationMetadata(page, 'UserByScreenName', operation), TWITTER_BEARER_TOKEN);
    } catch (error) {
      const message = String(error?.message ?? '');
      if (message.includes('ONECREW_X_IDENTITY_INPUT')) throw new ArgumentError('Invalid OneCrew X identity input');
      if (message.includes('ONECREW_X_IDENTITY_AUTH')) throw new AuthRequiredError('x.com', 'OneCrew X identity requires sign-in');
      if (message.includes('ONECREW_X_IDENTITY_RISK')) throw new CommandExecutionError('OneCrew X rate limit requires human review');
      throw new CommandExecutionError('OneCrew X native identity could not be verified');
    }
  },
});
