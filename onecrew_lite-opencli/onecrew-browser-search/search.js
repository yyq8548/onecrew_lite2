import {readFile} from 'node:fs/promises';
import {cli, getRegistry} from '@jackwener/opencli/registry';
import {AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {withOwnedSearchPage, readSearchStop, readYahooEmptyResult, normalizeBrowserRows, readFacebookCreatorResults} from './owned-page.mjs';
import {observeLinkedinPeople,readLinkedinPeopleWhenReady} from './linkedin-search.mjs';
import {observeYoutubeSearchAuthors} from './youtube-authors.mjs';

// Exact reviewed OpenCLI release and entrypoints; keep native parsing and errors.
const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
const metadata = JSON.parse(await readFile(new URL('package.json', root), 'utf8'));
if (metadata.version !== '1.8.8') throw new Error('OneCrew search requires OpenCLI 1.8.8');
for (const [site, name] of [
  ['youtube','search'], ['twitter','search'],
  ['linkedin','people-search'], ['facebook','search'],
  ['douyin','search'], ['weibo','search'], ['tieba','search'],
  ['google','search'], ['yahoo','search'],
]) {
  const nativeModule = await import(new URL(`clis/${site}/${name}.js`, root));
  const native = getRegistry().get(`${site}/${name}`);
  if (native?.access !== 'read' || !native.browser || typeof native.func !== 'function')
    throw new Error('Reviewed native search unavailable');
  cli({...native, site:`onecrew-search-${site}`, navigateBefore:false,
    func:(page,args) => withOwnedSearchPage(page, async owned => {
      try {
        if (site === 'facebook') return await readFacebookCreatorResults(owned, args);
        if (site === 'douyin') {
          const limit = nativeModule.parseSearchLimit(args.limit);
          await owned.goto('https://www.douyin.com/search/' + encodeURIComponent(args.query) + '?type=video', {settleMs:0});
          const raw = await owned.evaluate(`(${readDouyinCardsWhenReady.toString()})(${readSearchStop.toString()})`);
          const result = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
          if (result.state === 'auth') throw new AuthRequiredError(native.domain, 'Douyin search requires sign-in');
          if (result.state === 'risk') throw new CommandExecutionError('Douyin CAPTCHA or rate limit requires human review');
          const projected = nativeModule.projectSearchCards(result.cards, limit);
          if (projected.invalidCount) throw new CommandExecutionError('Douyin search cards lack video URL or description');
          return normalizeBrowserRows(site, projected.rows, limit);
        }
        if (site === 'linkedin') {
          if (typeof args.keywords !== 'string' || !args.keywords.trim()
            || !Number.isInteger(args.limit) || args.limit < 1 || args.limit > 10)
            throw new Error('ONECREW_LINKEDIN_SEARCH_INPUT');
          await owned.goto('https://www.linkedin.com/search/results/people/?keywords=' + encodeURIComponent(args.keywords),{settleMs:0});
          const raw = await owned.evaluate(`(${readLinkedinPeopleWhenReady.toString()})(${JSON.stringify(args.keywords)},${args.limit},${observeLinkedinPeople.toString()},${readSearchStop.toString()})`);
          const rows = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
          return normalizeBrowserRows(site, rows, args.limit);
        }
        const rows = normalizeBrowserRows(site, await native.func(owned,args),args.limit);
        if (site === 'youtube') {
          const raw = await owned.evaluate(`(${observeYoutubeSearchAuthors.toString()})()`);
          const observations = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
          if (!Array.isArray(observations)) throw new Error('ONECREW_YOUTUBE_SEARCH_AUTHORS_INVALID');
          const authors = new Map(observations.map(row => [row.video_id, row.channel_id]));
          return rows.map(row => {
            const channel_id = authors.get(new URL(row.url).searchParams.get('v'));
            return channel_id ? {...row, channel_id} : row;
          });
        }
        return rows;
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
        if (site === 'yahoo' && error?.code === 'EMPTY_RESULT' && state === 'unknown') {
          const raw = await owned.evaluate(`(${readYahooEmptyResult.toString()})(${JSON.stringify(args.keyword)})`);
          const empty = raw && 'session' in Object(raw) && 'data' in Object(raw) ? raw.data : raw;
          if (empty === true) return [];
        }
        throw error;
      }
    })});
}

async function readDouyinCardsWhenReady(readStop) {
  for (let attempt = 0; attempt < 30; attempt++) {
    const cards = [...document.querySelectorAll('[data-e2e="scroll-list"] li')].map(li => {
      const anchor = li.querySelector('a[href*="/video/"]');
      if (!anchor) return null;
      return {href:anchor.getAttribute('href'),leafTexts:[...li.querySelectorAll('*')].filter(node=>!node.children.length).map(node=>node.textContent.trim()).filter(Boolean)};
    }).filter(Boolean);
    if (cards.length) return {state:'rendered',cards};
    const state = readStop();
    if (state === 'auth' || state === 'risk') return {state,cards:[]};
    if (/暂无相关搜索结果|没有找到相关结果|搜索结果为空|暂无结果/.test(document.body?.innerText ?? '')) return {state:'empty',cards:[]};
    if (attempt < 29) await new Promise(resolve=>setTimeout(resolve,500));
  }
  throw new Error('ONECREW_DOUYIN_SEARCH_TIMEOUT');
}
