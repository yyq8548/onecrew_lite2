import {cli,Strategy} from '@jackwener/opencli/registry';
import {ArgumentError,AuthRequiredError,CommandExecutionError,TimeoutError} from '@jackwener/opencli/errors';
import {readFacebookProfile,readFacebookContact} from './facebook-profile.mjs';
import {readFacebookIdentity} from './facebook-identity.mjs';
import {withOwnedSearchPage} from './owned-page.mjs';
import {readFacebookNativeProfile,readFacebookNativeContact} from './facebook-native-profile.mjs';
import {readFacebookNativeOriginal,readFacebookNativeTargets} from './facebook-native-original.mjs';

function nativeReadError(error) {
  if(['ONECREW_FB_IDENTITY_INPUT','ONECREW_FB_PROFILE_INPUT'].includes(error?.message)) throw new ArgumentError('Invalid observed Facebook profile source');
  if(error?.message==='ONECREW_FB_PROFILE_AUTH') throw new AuthRequiredError('www.facebook.com','Profile requires sign-in');
  if(error?.message==='ONECREW_FB_PROFILE_RISK') throw new CommandExecutionError('Facebook verification requires human review');
  if(error?.message==='ONECREW_FB_PROFILE_TIMEOUT') throw new TimeoutError('Facebook native profile header',6);
  throw error;
}
for(const [name,reader]of [['native-targets',readFacebookNativeTargets],['native-original',readFacebookNativeOriginal]]) {
  cli({site:'onecrew-facebook-profile',name,access:'read',browser:true,domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
    description:'Read one saved native author original source without comments or translated messages',
    args:[{name:'id',positional:true,required:true},{name:'url',positional:true,required:true},...(name==='native-original'?[{name:'source',positional:true,required:true}]:[])],
    columns:['author_id','source_url'],func:async(page,args)=>{try{const result=await reader(page,args,withOwnedSearchPage);return Array.isArray(result)?result:[result];}
      catch(error){if(error?.message==='ONECREW_COMMENTS_AUTH')throw new AuthRequiredError('www.facebook.com','Content requires sign-in');
        if(error?.message==='ONECREW_COMMENTS_RISK')throw new CommandExecutionError('Facebook verification requires human review');nativeReadError(error);}}
  });
}
cli({site:'onecrew-facebook-profile',name:'native-read',access:'read',browser:true,domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read native identity and observed About contact routes for one saved vanity profile',
  args:[{name:'id',positional:true,required:true},{name:'url',positional:true,required:true}],columns:['id','display_name','observed_profile_url','contact_links'],
  func:async(page,args)=>{try{return [await readFacebookNativeProfile(page,args,withOwnedSearchPage)];}catch(error){nativeReadError(error);}}
});
cli({site:'onecrew-facebook-profile',name:'native-contact',access:'read',browser:true,domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read one observed About field route after native numeric identity verification',
  args:[{name:'id',positional:true,required:true},{name:'url',positional:true,required:true},{name:'source',positional:true,required:true}],columns:['id','source_url','contact_text','contact_links'],
  func:async(page,args)=>{try{return [await readFacebookNativeContact(page,args,withOwnedSearchPage)];}catch(error){nativeReadError(error);}}
});
cli({site:'onecrew-facebook-profile',name:'identity',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Resolve one saved vanity profile from its native public profile-header identity',
  args:[{name:'url',positional:true,required:true}],columns:['id','display_name','source_url','profile_url'],
  func:async(page,args)=>{
    try{return [await readFacebookIdentity(page,args,withOwnedSearchPage)];}
    catch(error){
      if(error?.message==='ONECREW_FB_IDENTITY_INPUT')throw new ArgumentError('Invalid observed Facebook vanity profile URL');
      if(error?.message==='ONECREW_FB_IDENTITY_AUTH')throw new AuthRequiredError('www.facebook.com','Profile requires sign-in');
      if(error?.message==='ONECREW_FB_IDENTITY_RISK')throw new CommandExecutionError('Facebook verification requires human review');
      if(error?.message==='ONECREW_FB_IDENTITY_TIMEOUT')throw new TimeoutError('Facebook native profile identity',8);
      throw error;
    }
  },
});
cli({site:'onecrew-facebook-profile',name:'read',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read one observed numeric Facebook profile header without inferring exact abbreviated counts',
  args:[{name:'id',positional:true,required:true}],columns:['id','display_name','follower_count','follower_text','profile_url'],
  func:async(page,args)=>{
    try{return [await readFacebookProfile(page,args,withOwnedSearchPage)];}
    catch(error){
      if(error?.message==='ONECREW_FB_PROFILE_INPUT')throw new ArgumentError('Invalid observed Facebook profile ID');
      if(error?.message==='ONECREW_FB_PROFILE_AUTH')throw new AuthRequiredError('www.facebook.com','Profile requires sign-in');
      if(error?.message==='ONECREW_FB_PROFILE_RISK')throw new CommandExecutionError('Facebook verification requires human review');
      if(error?.message==='ONECREW_FB_PROFILE_TIMEOUT')throw new TimeoutError('Facebook public profile header',8);
      throw error;
    }
  },
});

cli({site:'onecrew-facebook-profile',name:'contact',access:'read',browser:true,
  domain:'www.facebook.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read only an observed same-author Facebook contact-information page',
  args:[{name:'id',positional:true,required:true},{name:'source',positional:true,required:false}],columns:['id','source_url','title','contact_text','contact_links','truncated'],
  func:async(page,args)=>{
    try{return [await readFacebookContact(page,args,withOwnedSearchPage)];}
    catch(error){
      if(error?.message==='ONECREW_FB_PROFILE_INPUT')throw new ArgumentError('Invalid observed Facebook profile ID');
      if(error?.message==='ONECREW_FB_PROFILE_AUTH')throw new AuthRequiredError('www.facebook.com','Contact page requires sign-in');
      if(error?.message==='ONECREW_FB_PROFILE_RISK')throw new CommandExecutionError('Facebook verification requires human review');
      if(error?.message==='ONECREW_FB_PROFILE_TIMEOUT')throw new TimeoutError('Facebook public contact page',9);
      throw error;
    }
  },
});
