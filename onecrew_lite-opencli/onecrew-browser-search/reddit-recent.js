import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readRedditRecent} from './reddit-recent.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-reddit-profile',name:'recent',access:'read',browser:true,
  domain:'www.reddit.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read up to three original post locators bound to an observed Reddit author',
  args:[{name:'username',positional:true,required:true},{name:'author_id',positional:true,required:true}],
  columns:['source_url'],func:async(page,args)=>{
    try{return await readRedditRecent(page,args,withOwnedSearchPage);}
    catch(error){
      if(error?.message==='ONECREW_REDDIT_PROFILE_INPUT')throw new ArgumentError('Invalid observed Reddit author');
      if(error?.message==='ONECREW_REDDIT_RECENT_AUTH')throw new AuthRequiredError('www.reddit.com','Posts require sign-in');
      if(error?.message==='ONECREW_REDDIT_RECENT_RISK')throw new CommandExecutionError('Reddit rate limit requires human review');
      throw error;
    }
  },
});
