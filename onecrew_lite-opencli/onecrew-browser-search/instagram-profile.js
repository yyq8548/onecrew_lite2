import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError,TimeoutError} from '@jackwener/opencli/errors';
import {readInstagramProfile,readInstagramWebProfile} from './instagram-profile.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-instagram-profile',name:'read',access:'read',browser:true,
  domain:'www.instagram.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read native public Instagram author identity and observed follower count',
  args:[{name:'username',positional:true,required:true},{name:'author_id',positional:true}],
  columns:['id','username','follower_count','profile_url'],
  func:async(page,args)=>{
    try{return [await (args.author_id?readInstagramWebProfile:readInstagramProfile)(page,args,withOwnedSearchPage)];}
    catch(error){
      if(error?.message==='ONECREW_PROFILE_INPUT')throw new ArgumentError('Invalid Instagram username');
      if(error?.message==='ONECREW_PROFILE_TIMEOUT')throw new TimeoutError('Instagram public profile response',8);
      if(error?.message==='ONECREW_PROFILE_AUTH')throw new AuthRequiredError('www.instagram.com','Profile requires sign-in');
      if(error?.message==='ONECREW_PROFILE_RISK') {
        if(error.httpStatus===429)throw new CommandExecutionError('Instagram HTTP 429 rate limit; stop profile reads until the platform allows them');
        if(error.httpStatus===403)throw new CommandExecutionError('Instagram HTTP 403 forbidden; profile access denied');
        throw new CommandExecutionError('Instagram verification requires human review');
      }
      throw error;
    }
  },
});
