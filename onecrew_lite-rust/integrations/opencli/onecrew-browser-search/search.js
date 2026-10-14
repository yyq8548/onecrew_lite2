import {readFile} from 'node:fs/promises';
import {cli, getRegistry} from '@jackwener/opencli/registry';
import {AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {withOwnedSearchPage, readSearchStop, normalizeBrowserRows, readFacebookCreatorResults} from './owned-page.mjs';
import {observeLinkedinPeople} from './linkedin-search.mjs';

// Exact reviewed OpenCLI release and entrypoints; keep native parsing and errors.
const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
const metadata = JSON.parse(await readFile(new URL('package.json', root), 'utf8'));
if (metadata.version !== '1.8.8') throw new Error('OneCrew search requires OpenCLI 1.8.8');
for (const [site, name] of [
  ['twitter','search'],
  ['linkedin','people-search'], ['facebook','search'], ['zhihu','search'],
  ['douyin','search'], ['weibo','search'], ['tieba','search'],
  ['linkedin-learning','search'],
  ['dribbble','designer'], ['douban','search'],
  ['dribbble','shot'], ['google','search'], ['duckduckgo','search'],
  ['hupu','search'], ['weixin','search'], ['pinterest','search-boards'],
]) {
  await import(new URL(`clis/${site}/${name}.js`, root));
  const native = getRegistry().get(`${site}/${name}`);
  if (native?.access !== 'read' || !native.browser || typeof native.func !== 'function')
    throw new Error('Reviewed native search unavailable');
  cli({...native, site:`onecrew-search-${site}`, navigateBefore:false,
    func:(page,args) => withOwnedSearchPage(page, async owned => {
      try {
        if (site === 'facebook') return await readFacebookCreatorResults(owned, args, native.func);
        if (site === 'linkedin') {
          if (typeof args.keywords !== 'string' || !args.keywords.trim()
            || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 10)
            throw new Error('ONECREW_LINKEDIN_SEARCH_INPUT');
          await owned.goto('https://www.linkedin.com/search/results/people/?keywords=' + encodeURIComponent(args.keywords));
          await owned.sleep(6);
          const raw = await owned.evaluate(`(${observeLinkedinPeople.toString()})(${JSON.stringify(args.keywords)},${args.limit})`);
          const rows = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
          return normalizeBrowserRows(site, rows, args.limit);
        }
        return normalizeBrowserRows(site, await native.func(owned,args),args.limit);
      }
      catch(error) {
        if (error?.code === 'AUTH_REQUIRED' || error?.code === 'TIMEOUT') throw error;
        // Inspect only this owned page after a failed read. No retry or auth bypass.
        let state;
        try {
          for (let poll = 0; poll < 7; poll++) {
            const raw = await owned.evaluate(`(${readSearchStop.toString()})()`);
            state = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
            if (state !== 'loading_auth') break;
            if (poll < 6) await owned.sleep(1);
          }
        } catch { throw error; }
        if (state === 'risk') throw new CommandExecutionError('Search CAPTCHA or rate limit requires human review');
        if (state === 'auth') throw new AuthRequiredError(native.domain,'Search requires sign-in');
        throw error;
      }
    })});
}
