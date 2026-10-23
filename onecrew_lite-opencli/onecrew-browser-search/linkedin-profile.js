import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, AuthRequiredError, CommandExecutionError} from '@jackwener/opencli/errors';
import {readLinkedinProfile, readLinkedinContent, readLinkedinContact, readLinkedinExperience} from './linkedin-evidence.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
import {readLinkedinComments} from './linkedin-comments.mjs';

for (const name of ['profile','content','experience','contact','comments']) cli({site:'onecrew-linkedin-reader', name, access:'read', browser:true,
  domain:'www.linkedin.com', strategy:Strategy.COOKIE, navigateBefore:false,
  description:'Read bounded public identity and contact locators from an observed LinkedIn profile',
  args:[{name:'handle', required:true, positional:true}, ...(name!=='profile'?[{name:'account',required:true,positional:true}]:[]), ...(name==='comments'?[{name:'source',required:true,positional:true}]:[])],
  columns:name==='profile'?['author_handle','account_id','author_label','profile_url','follower_count','bio','public_links','contact_links']:name==='content'?['account_id','author_handle','post_id','source_url','text','text_truncated']:name==='comments'?['account_id','author_handle','source_url','comments','evidence_source']:['account_id','author_handle','source_url','title','contact_text','contact_links','truncated'],
  func:async (page,args) => {
    try {return [name==='profile'?await readLinkedinProfile(page,args.handle,withOwnedSearchPage):name==='experience'?await readLinkedinExperience(page,args.handle,args.account,withOwnedSearchPage):name==='content'?await readLinkedinContent(page,args.handle,args.account,withOwnedSearchPage):name==='comments'?await readLinkedinComments(page,args.handle,args.account,args.source,withOwnedSearchPage):await readLinkedinContact(page,args.handle,args.account,withOwnedSearchPage)];}
    catch(error) {
      if(error?.message==='ONECREW_LINKEDIN_INPUT') throw new ArgumentError('Invalid observed LinkedIn profile');
      if(error?.message==='ONECREW_LINKEDIN_AUTH') throw new AuthRequiredError('www.linkedin.com','Public profile requires sign-in');
      if(error?.message==='ONECREW_LINKEDIN_RISK') throw new CommandExecutionError('LinkedIn verification requires human review');
      throw error;
    }
  }});
