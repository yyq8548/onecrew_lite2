import { readFile } from 'node:fs/promises';
import { cli, getRegistry } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { commentReaders, readComments } from './comments.mjs';
import { withOwnedSearchPage } from './owned-page.mjs';

const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
if (JSON.parse(await readFile(new URL('package.json', root), 'utf8')).version !== '1.8.8')
  throw new Error('OneCrew comments require OpenCLI 1.8.8');
for (const [site, name] of commentReaders) {
  await import(new URL(`clis/${site}/${name}.js`, root));
  const native = getRegistry().get(`${site}/${name}`);
  if (native?.access !== 'read' || !native.browser || typeof native.func !== 'function')
    throw new Error('Reviewed comment reader unavailable');
  cli({ ...native, site: `onecrew-comments-${site}`, navigateBefore: false,
    func: async (page, args) => {
      try { return await readComments(page, args, site, native, withOwnedSearchPage); }
      catch (error) {
        if (error?.message === 'ONECREW_COMMENTS_INPUT') throw new ArgumentError('Invalid comment source ID');
        if (error?.message === 'ONECREW_COMMENTS_AUTH') throw new AuthRequiredError(native.domain, 'Comments require sign-in');
        if (error?.message === 'ONECREW_COMMENTS_RISK') throw new CommandExecutionError('Comment verification or rate limit requires human review');
        throw error;
      }
    } });
}
