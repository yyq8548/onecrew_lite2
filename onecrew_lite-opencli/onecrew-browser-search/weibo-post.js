import { readFile } from 'node:fs/promises';
import { cli, getRegistry } from '@jackwener/opencli/registry';
import { ArgumentError } from '@jackwener/opencli/errors';
import { withOwnedSearchPage } from './owned-page.mjs';

const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
if (JSON.parse(await readFile(new URL('package.json', root), 'utf8')).version !== '1.8.8')
  throw new Error('OneCrew Weibo post reads require OpenCLI 1.8.8');
await import(new URL('clis/weibo/post.js', root));
const native = getRegistry().get('weibo/post');
if (native?.access !== 'read' || !native.browser || typeof native.func !== 'function')
  throw new Error('Reviewed Weibo post reader unavailable');
cli({ ...native, site: 'onecrew-weibo-post', navigateBefore: false,
  func: (page, args) => {
    if (typeof args.id !== 'string' || !/^[A-Za-z0-9]{1,20}$/.test(args.id))
      throw new ArgumentError('Expected an observed Weibo post ID');
    return withOwnedSearchPage(page, owned => native.func(owned, {id: args.id}));
  },
});
