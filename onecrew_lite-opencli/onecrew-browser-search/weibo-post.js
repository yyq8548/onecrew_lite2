import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, AuthRequiredError, CommandExecutionError } from '@jackwener/opencli/errors';
import { withOwnedSearchPage } from './owned-page.mjs';
import { fetchWeiboPost, nativeWeiboId } from './weibo-read.mjs';

cli({site:'onecrew-weibo-post',name:'post',access:'read',domain:'weibo.com',strategy:Strategy.COOKIE,navigateBefore:false,
  description:'Read an observed Weibo post with full-text status',
  args:[{name:'id',required:true,positional:true}],columns:['field','value'],
  func:(page,args)=>{
    if(typeof args.id!=='string'||!/^[A-Za-z0-9]{1,20}$/.test(args.id)) throw new ArgumentError('Expected an observed Weibo post ID');
    return withOwnedSearchPage(page,async owned=>{
      await owned.goto('https://weibo.com/');
      const raw=await owned.evaluate(`(${fetchWeiboPost.toString()})(${JSON.stringify(args.id)},${nativeWeiboId.toString()})`);
      const data=raw&&'session' in Object(raw)&&'data' in Object(raw)?raw.data:raw;
      return Object.entries(data).map(([field,value])=>({field,value:String(value)}));
    }).catch(error=>{
      const message=String(error?.message ?? error);
      if(message.includes('ONECREW_WEIBO_AUTH')) throw new AuthRequiredError('weibo.com',message);
      if(message.includes('ONECREW_WEIBO_RISK')) throw new CommandExecutionError('Weibo CAPTCHA or rate limit: '+message);
      throw error;
    });
  },
});
