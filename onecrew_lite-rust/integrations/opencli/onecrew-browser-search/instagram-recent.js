import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError,TimeoutError} from '@jackwener/opencli/errors';
import {readInstagramRecent} from './instagram-recent.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-instagram-profile',name:'recent',access:'read',browser:true,
  domain:'www.instagram.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read up to three original posts bound to an observed Instagram native account',
  args:[{name:'username',positional:true,required:true},{name:'author_id',positional:true,required:true}],
  columns:['source_url'],func:async(page,args)=>{
    try{return await readInstagramRecent(page,args,withOwnedSearchPage);}
    catch(error){
      if(error?.message==='ONECREW_INSTAGRAM_RECENT_INPUT')throw new ArgumentError('Invalid observed Instagram author');
      if(error?.message==='ONECREW_INSTAGRAM_RECENT_AUTH')throw new AuthRequiredError('www.instagram.com','Posts require sign-in');
      if(error?.message==='ONECREW_INSTAGRAM_RECENT_RISK')throw new CommandExecutionError('Instagram verification or rate limit requires human review');
      if(error?.message==='ONECREW_INSTAGRAM_RECENT_TIMEOUT')throw new TimeoutError('Instagram recent posts response',8);
      throw error;
    }
  },
});
