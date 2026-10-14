import {cli,Strategy} from '@jackwener/opencli/registry';
import {AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readTikTokRecent} from './tiktok-language-recent.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-comments-tiktok',name:'recent',access:'read',browser:true,domain:'www.tiktok.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read up to three post locators from the bound creator profile',
  args:[{name:'username',positional:true,required:true},{name:'author-id',required:true}],columns:['source_url'],
  func:async(page,args)=>{
    try { return await readTikTokRecent(page,args.username,args['author-id'],withOwnedSearchPage); }
    catch(error) {
      if (error?.message==='ONECREW_TIKTOK_PROFILE_AUTH') throw new AuthRequiredError('www.tiktok.com','Profile posts require sign-in');
      if (error?.message==='ONECREW_TIKTOK_PROFILE_RISK') throw new CommandExecutionError('TikTok CAPTCHA or rate limit requires human review');
      throw error;
    }
  }});
