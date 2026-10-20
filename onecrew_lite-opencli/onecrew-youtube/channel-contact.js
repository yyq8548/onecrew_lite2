import { cli, Strategy } from '@jackwener/opencli/registry';
import { ArgumentError, CommandExecutionError } from '@jackwener/opencli/errors';
import { readChannelEvidence } from './channel.mjs';
cli({ site:'onecrew-youtube',name:'channel-contact',access:'read',
 description:'onecrew_lite: public description first, verified business email reveal; stop hidden email reads for this task when limited',
 domain:'www.youtube.com',strategy:Strategy.COOKIE,navigateBefore:false,
 args:[{name:'channel',required:true,positional:true},{name:'task',required:true}],columns:['channelId','title','hiddenEmailAvailable','hiddenEmailStatus'],
 func:async(page,args)=> { if(typeof args.channel!=='string' || !/^UC[A-Za-z0-9_-]{22}$/.test(args.channel)) throw new ArgumentError('Exact channel ID required');
 try { return await readChannelEvidence(page,args.channel,args.task); } catch { throw new CommandExecutionError('Channel contact evidence unavailable'); } }
});
