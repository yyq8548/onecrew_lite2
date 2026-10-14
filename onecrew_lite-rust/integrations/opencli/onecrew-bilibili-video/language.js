import { readFile } from 'node:fs/promises';
import { cli, Strategy } from '@jackwener/opencli/registry';
import { AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readBilibiliLanguage, readBilibiliApi } from './language.mjs';
import { readBilibiliPageLanguage } from './language-page.mjs';
import { withOwnedSearchPage } from '../onecrew-browser-search/owned-page.mjs';
const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
if (JSON.parse(await readFile(new URL('package.json', root), 'utf8')).version !== '1.8.8')
  throw new Error('OneCrew language requires OpenCLI 1.8.8');
const { wbiSign } = await import(new URL('clis/bilibili/utils.js', root));
const apiGet = (page, path, options) => readBilibiliApi(page, path, options, wbiSign);
function requireData(payload) {
  if (payload?.code === -101) throw new AuthRequiredError('www.bilibili.com', 'Sign-in required');
  if ([-352, -412, -509].includes(payload?.code))
    throw new CommandExecutionError('Bilibili CAPTCHA or rate limit requires human review');
  if (payload?.code !== 0 || !payload.data || typeof payload.data !== 'object')
    throw new CommandExecutionError('Bilibili original language API unavailable');
  return payload.data;
}
cli({ site: 'onecrew-bilibili-video', name: 'language', access: 'read',
  domain: 'www.bilibili.com', strategy: Strategy.COOKIE, navigateBefore: false,
  description: 'Author-bound original video text and up to 5 comments, or three recent locators',
  args: [{ name: 'mid', required: true, positional: true }, { name: 'video', required: true, positional: true }],
  columns: ['source_url'],
  func: async (page, args) => {
    try { return await readBilibiliLanguage(page, args.mid, args.video, apiGet, withOwnedSearchPage, requireData); }
    catch (error) {
      // Only the observed non-JSON API response permits a public-page read.
      // Auth, risk controls, invalid identity and actual provider errors remain failures.
      if (args.video === 'recent' || !String(error?.message).includes('is not valid JSON')) throw error;
      return withOwnedSearchPage(page, owned => readBilibiliPageLanguage(owned, args.mid, args.video));
    }
  },
});
