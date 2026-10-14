import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readProfile,readSearch,readPinSearch,readContent} from './pinterest-evidence.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
for (const name of ['profile','search-users','search-pins','content','content-pin']) cli({site:'onecrew-pinterest-reader',name,access:'read',strategy:Strategy.COOKIE,
  domain:'www.pinterest.com',navigateBefore:false,
  description:'Read bounded native Pinterest accounts, search evidence and public captions',
  args:name === 'profile' ? [{name:'username',required:true,positional:true}]
    : name === 'content-pin' ? [{name:'username',required:true,positional:true},{name:'author',required:true,positional:true},{name:'pin',required:true,positional:true}]
    : name === 'content' ? [{name:'username',required:true,positional:true},{name:'author',required:true,positional:true}]
    : [{name:'query',required:true,positional:true},{name:'limit',type:'int',default:10}],
  columns:['author_id','username','profile_url','follower_count'],
  func:async(page,args) => {
    try {return name === 'profile' ? await readProfile(page,args.username,withOwnedSearchPage)
      : name === 'content-pin' ? await readContent(page,args.author,args.username,withOwnedSearchPage,args.pin)
      : name === 'content' ? await readContent(page,args.author,args.username,withOwnedSearchPage)
      : name === 'search-pins' ? await readPinSearch(page,args.query,args.limit,withOwnedSearchPage)
      : await readSearch(page,args.query,args.limit,withOwnedSearchPage);}
    catch(error) {
      if (error?.message === 'ONECREW_PINTEREST_AUTH') throw new AuthRequiredError('www.pinterest.com','Pinterest sign-in required');
      if (error?.message === 'ONECREW_PINTEREST_RISK') throw new CommandExecutionError('Pinterest verification or rate limit; stop this source');
      if (error?.message === 'ONECREW_PINTEREST_INVALID') throw new ArgumentError('Invalid native Pinterest input or result');
      throw error;
    }
  },
});
