import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readFacebookComments} from './facebook-comments.mjs';
import {readFacebookReelComments} from './facebook-reel-comments.mjs';
import {readFacebookRecent} from './facebook-recent.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-comments-facebook',name:'comments',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read original Facebook post and bounded visible comment language',
  args:[{name:'id',positional:true,required:true},{name:'limit',type:'int',default:20}],
  columns:['type','source_url','text'],
  func:async(page,args) => {
    try {return await readFacebookComments(page,args,withOwnedSearchPage);}
    catch(error) {
      if(error?.message==='ONECREW_COMMENTS_INPUT') throw new ArgumentError('Invalid Facebook post source');
      if(error?.message==='ONECREW_COMMENTS_AUTH') throw new AuthRequiredError('www.facebook.com','Comments require sign-in');
      if(error?.message==='ONECREW_COMMENTS_RISK') throw new CommandExecutionError('Facebook verification or rate limit requires human review');
      throw error;
    }
  },
});
cli({site:'onecrew-comments-facebook',name:'recent',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read up to three observed Reel links from a numeric author profile',
  args:[{name:'id',positional:true,required:true}],columns:['author_id','source_url'],
  func:async(page,args)=>{
    try {return await readFacebookRecent(page,args,withOwnedSearchPage);}
    catch(error) {
      if(error?.message==='ONECREW_FB_PROFILE_INPUT') throw new ArgumentError('Invalid Facebook numeric author');
      if(error?.message==='ONECREW_COMMENTS_AUTH') throw new AuthRequiredError('www.facebook.com','Profile requires sign-in');
      if(error?.message==='ONECREW_COMMENTS_RISK') throw new CommandExecutionError('Facebook verification or rate limit requires human review');
      throw error;
    }
  },
});
cli({site:'onecrew-comments-facebook',name:'content',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read only the original caption and numeric owner of an observed Facebook Reel',
  args:[{name:'id',positional:true,required:true}],columns:['type','source_url','text'],
  func:async(page,args)=>{
    try {return await readFacebookReelComments(page,{id:args.id,limit:1},withOwnedSearchPage,true);}
    catch(error) {
      if(error?.message==='ONECREW_COMMENTS_INPUT') throw new ArgumentError('Invalid Facebook Reel source');
      if(error?.message==='ONECREW_COMMENTS_AUTH') throw new AuthRequiredError('www.facebook.com','Content requires sign-in');
      if(error?.message==='ONECREW_COMMENTS_RISK') throw new CommandExecutionError('Facebook verification or rate limit requires human review');
      throw error;
    }
  },
});
