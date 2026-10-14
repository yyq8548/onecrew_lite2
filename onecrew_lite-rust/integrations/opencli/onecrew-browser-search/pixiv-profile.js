import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readPixivEvidence} from './pixiv-evidence.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
for(const name of ['profile','content']){
  cli({site:'onecrew-pixiv-reader',name,access:'read',browser:true,domain:'www.pixiv.net',strategy:Strategy.COOKIE,navigateBefore:false,
    description:'Read bounded native evidence for an observed Pixiv author or artwork',
    args:[{name:'id',required:true,positional:true},...(name==='content'?[{name:'author',required:true,positional:true}]:[])],
    columns:name==='profile'?['author_id','author_label','profile_url','follower_count','bio','public_links']:['post_id','author_id','source_url','text','text_truncated'],
    func:async(page,args)=>{
      try{return [await readPixivEvidence(page,name,args.id,args.author,withOwnedSearchPage)];}
      catch(error){
        if(error?.message==='ONECREW_PIXIV_INPUT')throw new ArgumentError('Invalid observed Pixiv identity');
        if(error?.message==='ONECREW_PIXIV_AUTH')throw new AuthRequiredError('www.pixiv.net','Native evidence requires sign-in');
        if(error?.message==='ONECREW_PIXIV_RISK')throw new CommandExecutionError('Pixiv verification requires human review');
        throw error;
      }
    }});
}
