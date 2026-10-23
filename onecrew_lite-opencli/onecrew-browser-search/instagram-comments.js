import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readInstagramComments} from './instagram-comments.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
cli({site:'onecrew-comments-instagram', name:'comments', access:'read', browser:true,
  domain:'www.instagram.com', strategy:Strategy.COOKIE, navigateBefore:false,
  description:'Read original caption and bounded visible Instagram comments',
  args:[{name:'id', positional:true, required:true}, {name:'limit', type:'int', default:20}],
  columns:['type','id','native_author_id','author','source_url','text'],
  func:async (page,args) => {
    try {return await readInstagramComments(page,args,withOwnedSearchPage);}
    catch(error) {
      if(error?.message==='ONECREW_COMMENTS_INPUT') throw new ArgumentError('Invalid Instagram post source');
      if(error?.message==='ONECREW_COMMENTS_AUTH') throw new AuthRequiredError('www.instagram.com','Comments require sign-in');
      if(error?.message==='ONECREW_COMMENTS_RISK') throw new CommandExecutionError('Instagram verification or rate limit requires human review');
      throw error;
    }
  },
});
