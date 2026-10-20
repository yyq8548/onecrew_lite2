import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError,TimeoutError} from '@jackwener/opencli/errors';
import {instagramProfileTarget,readInstagramWebProfile} from './instagram-profile.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
for(const operation of ['read','identity']) cli({site:'onecrew-instagram-profile',name:operation,access:'read',browser:true,
  domain:'www.instagram.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Verify the visible Instagram account, expand its biography and preserve public contacts through Computer Use',
  args:[{name:'username',positional:true,required:true},{name:'author_id',positional:true}],
  columns:['id','username','follower_count','profile_url'],
  func:async(page,args)=>{
    try{
      const username=instagramProfileTarget(args.username);
      const authorId=operation==='identity'||!args.author_id?`https://www.instagram.com/${username}/`:args.author_id;
      return [await readInstagramWebProfile(page,{...args,username,author_id:authorId},withOwnedSearchPage)];
    }
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
