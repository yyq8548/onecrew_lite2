import { cli, Strategy } from '@jackwener/opencli/registry';
import { readFile } from 'node:fs/promises';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readPublic, lobstersRows } from './read.mjs';
const root = new URL('../../', import.meta.resolve('@jackwener/opencli'));
const metadata = JSON.parse(await readFile(new URL('package.json', root), 'utf8'));
if (metadata.version !== '1.8.8') throw new Error('OneCrew public search requires OpenCLI 1.8.8');
const { parseFeed, PRODUCTHUNT_CATEGORY_SLUGS } = await import(new URL('clis/producthunt/utils.js', root));

async function read(task) {
  try { return await task(); }
  catch (error) {
    if (error.message === 'PUBLIC_SCOPE') throw new ArgumentError('The selected public source scope does not exist');
    if (error.message === 'PUBLIC_AUTH') throw new AuthRequiredError('public search', 'Sign-in required');
    if (error.message === 'PUBLIC_RISK') throw new CommandExecutionError('Public search CAPTCHA or rate limit requires human review');
    throw new CommandExecutionError('Public search response could not be verified');
  }
}
cli({ site: 'onecrew-public-lobsters', name: 'tag', access: 'read', browser: false,
  domain: 'lobste.rs', strategy: Strategy.PUBLIC, description: 'Bounded public Lobsters tag stories',
  args: [{ name: 'tag', positional: true, required: true }, { name: 'limit', type: 'int', default: 5 }],
  columns: ['rank', 'id', 'title', 'author', 'url'],
  func: args => read(async () => {
    const tag = String(args.tag || '');
    if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(tag)) throw Error('PUBLIC_SCOPE');
    return lobstersRows(await readPublic(`https://lobste.rs/t/${encodeURIComponent(tag)}.json`), Math.min(10, Math.max(1, Number(args.limit) || 5)));
  }) });
cli({ site: 'onecrew-public-producthunt', name: 'posts', access: 'read', browser: false,
  domain: 'www.producthunt.com', strategy: Strategy.PUBLIC, description: 'Bounded public Product Hunt feed',
  args: [{ name: 'category', type: 'string', default: '' }, { name: 'limit', type: 'int', default: 5 }],
  columns: ['rank', 'name', 'tagline', 'author', 'date', 'url'],
  func: args => read(async () => {
    const category = String(args.category || '');
    if (category && !PRODUCTHUNT_CATEGORY_SLUGS.includes(category)) throw Error('PUBLIC_SCOPE');
    const xml = await readPublic(`https://www.producthunt.com/feed${category ? `?category=${encodeURIComponent(category)}` : ''}`);
    if (!/<feed[\s>]/.test(xml) || !/<\/feed>/.test(xml)) throw Error('PUBLIC_SHAPE');
    return parseFeed(xml).slice(0, Math.min(10, Math.max(1, Number(args.limit) || 5)));
  }) });
