import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError, TimeoutError} from '@jackwener/opencli/errors';
import {searchCards} from './search.mjs';
for (const site of ['nowcoder']) cli({
  site:`onecrew-${site}`,name:'search',access:'read',browser:true,navigateBefore:false,
  domain:`www.${site}.com`,strategy:Strategy.COOKIE,
  description:`OneCrew bounded ${site} visible search cards`,
  args:[{name:'query',positional:true,required:true},{name:'limit',type:'int',default:5}],
  columns:['title','author','url','author_id','author_url','id','uuid','post_type'],
  func:async (page,args) => {try {return await searchCards(page,site,args.query,args.limit);} catch(error) {
    if (error.message === 'SOURCE_INPUT') throw new ArgumentError('Invalid search input');
    if (error.message === 'SOURCE_AUTH') throw new AuthRequiredError(`www.${site}.com`,'Search requires sign-in');
    if (error.message === 'SOURCE_TIMEOUT') throw new TimeoutError(`${site} search hydration`,14);
    if (error.message === 'SOURCE_RISK') throw new CommandExecutionError('Search CAPTCHA or rate limit requires human review');
    throw new CommandExecutionError(`${site} search cards could not be verified`);
  }}
});
