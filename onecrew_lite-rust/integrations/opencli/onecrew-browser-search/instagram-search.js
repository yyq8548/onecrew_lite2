import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError,TimeoutError} from '@jackwener/opencli/errors';
import {readInstagramSearch} from './instagram-search.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-search-instagram',name:'search',access:'read',browser:true,
  domain:'www.instagram.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Search Instagram users retaining observed native author identity',
  args:[{name:'query',positional:true,required:true},{name:'limit',type:'int',default:20}],
  columns:['username','name','url'],func:async(page,args)=>{
    try{return await readInstagramSearch(page,args,withOwnedSearchPage);}
    catch(error){
      if(error?.message==='ONECREW_INSTAGRAM_SEARCH_INPUT')throw new ArgumentError('Invalid Instagram search');
      if(error?.message==='ONECREW_INSTAGRAM_SEARCH_AUTH')throw new AuthRequiredError('www.instagram.com','Search requires sign-in');
      if(error?.message==='ONECREW_INSTAGRAM_SEARCH_RISK')throw new CommandExecutionError('Instagram verification or rate limit requires human review');
      if(error?.message==='ONECREW_INSTAGRAM_SEARCH_TIMEOUT')throw new TimeoutError('Instagram search response',8);
      throw error;
    }
  },
});
