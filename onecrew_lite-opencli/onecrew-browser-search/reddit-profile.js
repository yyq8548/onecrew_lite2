import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readRedditProfile} from './reddit-profile.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-reddit-profile',name:'read',access:'read',browser:true,
  domain:'www.reddit.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read a public Reddit author bound to an observed native account ID',
  args:[{name:'username',positional:true,required:true},{name:'author_id',positional:true,required:true}],
  columns:['id','username','follower_count','profile_url'],
  func:async(page,args)=>{
    try{return [await readRedditProfile(page,args,withOwnedSearchPage)];}
    catch(error){
      if(error?.message==='ONECREW_REDDIT_PROFILE_INPUT')throw new ArgumentError('Invalid observed Reddit author');
      if(error?.message==='ONECREW_REDDIT_PROFILE_AUTH')throw new AuthRequiredError('www.reddit.com','Profile requires sign-in');
      if(error?.message==='ONECREW_REDDIT_PROFILE_RISK')throw new CommandExecutionError('Reddit rate limit requires human review');
      throw error;
    }
  },
});
