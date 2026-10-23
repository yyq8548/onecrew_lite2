import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError} from '@jackwener/opencli/errors';
import {readDouyinEvidence} from './douyin-evidence.mjs';
import {readDouyinComments} from './douyin-comments.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
for(const name of ['profile','content','comments']) {
  cli({site:'onecrew-douyin-reader',name,access:'read',browser:true,domain:'www.douyin.com',strategy:Strategy.COOKIE,navigateBefore:false,
    description:'Read source-bound public author evidence or an explicitly requested same-video comment window',
    args:[{name:'id',required:true,positional:true},...(name!=='profile'?[{name:'author',required:name==='comments',positional:true}]:[])],
    columns:name==='profile'?['author_id','author_label','profile_url','follower_count','bio','public_links']:['post_id','author_id','profile_url','source_url','text'],
    func:async(page,args)=>{
      try{return [await (name==='comments'?readDouyinComments(page,args.id,args.author,withOwnedSearchPage):readDouyinEvidence(page,name,args.id,args.author,withOwnedSearchPage))];}
      catch(error){
        if(error?.message==='ONECREW_DOUYIN_INPUT')throw new ArgumentError('Invalid observed Douyin identity');
        if(error?.message==='ONECREW_DOUYIN_AUTH')throw new AuthRequiredError('www.douyin.com','Native evidence requires sign-in');
        if(error?.message==='ONECREW_DOUYIN_RISK')throw new CommandExecutionError('Douyin verification requires human review');
        throw error;
      }
    }});
}
