import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readRedditSearch} from './reddit-search.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-search-reddit',name:'search',access:'read',browser:true,
  domain:'www.reddit.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Search public Reddit posts retaining observed native author identity',
  args:[{name:'query',positional:true,required:true},{name:'limit',type:'int',default:20}],
  columns:['title','author','url'],func:async(page,args)=>{
    try{return await readRedditSearch(page,args,withOwnedSearchPage);}
    catch(error){
      if(error?.message==='ONECREW_REDDIT_SEARCH_INPUT')throw new ArgumentError('Invalid Reddit search');
      if(error?.message==='ONECREW_REDDIT_SEARCH_AUTH')throw new AuthRequiredError('www.reddit.com','Search requires sign-in');
      if(error?.message==='ONECREW_REDDIT_SEARCH_RISK')throw new CommandExecutionError('Reddit verification or rate limit requires human review');
      throw error;
    }
  },
});
